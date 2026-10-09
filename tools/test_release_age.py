"""Tests for the release-age check. Every one runs with no network.

The fetcher is a function argument throughout `release_age`, so these tests hand it a
dictionary and assert on what comes back. The clock is a `--today` argument for the same
reason: a test that depends on the day it runs is a test that starts failing on its own.

    python3 -m unittest discover --start-directory tools
"""

from __future__ import annotations

import datetime
import gzip
import json
import unittest
import unittest.mock
import urllib.error
from pathlib import Path
from tempfile import TemporaryDirectory

import dependency_allowlist
import release_age

TODAY = datetime.date(2026, 10, 9)
LIMIT = release_age.DEFAULT_MAX_AGE_DAYS


def crate_payload(versions: list[tuple[str, str]], yanked: set[str] | None = None) -> bytes:
    yanked = yanked or set()
    return json.dumps(
        {
            "crate": {"name": "x", "versions": [1, 2, 3]},
            "versions": [
                {"num": num, "created_at": stamp, "yanked": num in yanked}
                for num, stamp in versions
            ],
        }
    ).encode()


def npm_payload(versions: list[tuple[str, str]], latest: str | None = None) -> bytes:
    times = {"created": "2019-01-01T00:00:00.000Z", "modified": "2026-10-08T00:00:00.000Z"}
    times.update(dict(versions))
    document: dict = {"name": "x", "time": times}
    if latest:
        document["dist-tags"] = {"latest": latest}
    return json.dumps(document).encode()


def fetcher(response: bytes | Exception):
    """A stand-in for `http_get`. The URL is ignored on purpose: these tests are about what
    the parsers do with a payload, and keying on the URL would make them depend on which
    registry base the environment happens to be pointing at."""

    def fetch(url: str) -> bytes:
        if isinstance(response, Exception):
            raise response
        return response

    return fetch


def dependency(name="widget", ecosystem="cargo", requirement="1.0", where="Cargo.toml [dependencies]"):
    return release_age.Dependency(name=name, ecosystem=ecosystem, requirement=requirement, where=where)


def allowlist(text: str) -> dependency_allowlist.Allowlist:
    return dependency_allowlist.parse(text, today=TODAY)


ENTRY = """
[[release_age]]
dependency = "widget"
ecosystem = "cargo"
reason = "test-only, and the best-maintained option that exists today"
decided_by = "Patrick"
look_again = {date}
"""


class WhatCountsAsTheLatestRelease(unittest.TestCase):
    def test_takes_the_newest_publish_date_not_the_highest_version(self):
        """A crate that patches an old minor line is being maintained. Reading the highest
        version number instead would call it abandoned."""
        release = release_age.newest_release(
            [("2.0.0", datetime.date(2025, 1, 1)), ("1.0.69", datetime.date(2026, 9, 1))], "x"
        )
        self.assertEqual(release.version, "1.0.69")
        self.assertEqual(release.published, datetime.date(2026, 9, 1))

    def test_a_prerelease_is_not_a_release(self):
        """The bug this was written for: the check reported @redocly/cli as
        0.0.0-snapshot.1791443981 and typescript as 7.1.0-dev.20261008.1."""
        release = release_age.newest_release(
            [
                ("2.60.0", datetime.date(2026, 10, 7)),
                ("0.0.0-snapshot.1791443981", datetime.date(2026, 10, 8)),
            ],
            "x",
        )
        self.assertEqual(release.version, "2.60.0")
        self.assertEqual(release.published, datetime.date(2026, 10, 7))

    def test_the_newest_prerelease_is_still_reported(self):
        release = release_age.newest_release(
            [("2.60.0", datetime.date(2026, 10, 7)), ("3.0.0-rc.1", datetime.date(2026, 10, 8))], "x"
        )
        self.assertEqual(release.prerelease, "3.0.0-rc.1")
        self.assertEqual(release.prerelease_published, datetime.date(2026, 10, 8))

    def test_a_package_with_only_prereleases_is_dated_from_one(self):
        release = release_age.newest_release([("1.0.0-alpha.1", datetime.date(2026, 5, 1))], "x")
        self.assertEqual(release.version, "1.0.0-alpha.1")

    def test_build_metadata_is_not_a_prerelease(self):
        self.assertFalse(release_age.is_prerelease("1.0.0+build.7"))
        self.assertTrue(release_age.is_prerelease("1.0.0-rc.1"))
        self.assertFalse(release_age.is_prerelease("0.8.9"))

    def test_same_day_publishes_break_the_tie_on_version(self):
        release = release_age.newest_release(
            [("3.0.0", datetime.date(2026, 10, 1)), ("3.0.1", datetime.date(2026, 10, 1))], "x"
        )
        self.assertEqual(release.version, "3.0.1")

    def test_no_versions_at_all_cannot_run(self):
        with self.assertRaises(release_age.CouldNotRun):
            release_age.newest_release([], "x")


class ReadingTheRegistries(unittest.TestCase):
    def test_crates_io_ignores_yanked_versions(self):
        payload = crate_payload(
            [("1.0.0", "2026-01-01T00:00:00Z"), ("1.0.1", "2026-09-01T00:00:00Z")],
            yanked={"1.0.1"},
        )
        release = release_age.crates_io_release("x", fetcher(payload))
        self.assertEqual(release.version, "1.0.0")

    def test_crates_io_every_version_yanked_cannot_run(self):
        payload = crate_payload([("1.0.0", "2026-01-01T00:00:00Z")], yanked={"1.0.0"})
        with self.assertRaises(release_age.CouldNotRun):
            release_age.crates_io_release("x", fetcher(payload))

    def test_npm_ignores_the_created_and_modified_keys(self):
        """`time.modified` moved @types/markdown-it 48 days ahead of its newest release,
        because DefinitelyTyped republishes tags. It is not a publish date."""
        release = release_age.npm_release(
            "x", fetcher(npm_payload([("14.2.0", "2026-08-17T00:00:00Z")]))
        )
        self.assertEqual(release.version, "14.2.0")
        self.assertEqual(release.published, datetime.date(2026, 8, 17))

    def test_npm_needs_no_dist_tags(self):
        release = release_age.npm_release(
            "x", fetcher(npm_payload([("1.2.3", "2026-07-01T00:00:00Z")]))
        )
        self.assertEqual(release.version, "1.2.3")

    def test_a_payload_that_is_not_json_cannot_run(self):
        with self.assertRaises(release_age.CouldNotRun):
            release_age.crates_io_release("x", fetcher(b"<html>503</html>"))

    def test_a_payload_of_the_wrong_shape_cannot_run(self):
        with self.assertRaises(release_age.CouldNotRun):
            release_age.npm_release("x", fetcher(b'{"name":"x"}'))

    def test_a_200_that_is_an_empty_object_cannot_run(self):
        """A registry answering 200 with the wrong thing must not read as a pass."""
        with self.assertRaises(release_age.CouldNotRun):
            release_age.crates_io_release("x", fetcher(b"{}"))

    def test_an_unreadable_timestamp_cannot_run(self):
        payload = crate_payload([("1.0.0", "the day before yesterday")])
        with self.assertRaises(release_age.CouldNotRun):
            release_age.crates_io_release("x", fetcher(payload))

    def test_the_exact_pin_is_dated_when_the_manifest_pins_one(self):
        payload = npm_payload(
            [("8.17.1", "2024-07-13T00:00:00Z"), ("8.20.0", "2026-04-24T00:00:00Z")]
        )
        release = release_age.npm_release("x", fetcher(payload), pin="8.17.1")
        self.assertEqual(release.version, "8.20.0")
        self.assertEqual(release.pinned_published, datetime.date(2024, 7, 13))


class TheUserAgentAndTheUrl(unittest.TestCase):
    def test_the_user_agent_names_the_repository(self):
        self.assertIn("github.com/devergehq/review-desk", release_age.USER_AGENT)

    def test_no_person_is_named_in_the_user_agent(self):
        """Patrick's rule: no person's name or email address in a request header or URL."""
        self.assertNotIn("@", release_age.USER_AGENT.replace("(+https://", ""))
        for forbidden in ("patrick", "dorival", "deverge.com", "accounts@"):
            self.assertNotIn(forbidden, release_age.USER_AGENT.lower())

    def test_a_scoped_npm_name_has_its_slash_encoded(self):
        seen = []

        def fetch(url):
            seen.append(url)
            return npm_payload([("1.0.0", "2026-09-01T00:00:00Z")])

        release_age.npm_release("@nuxt/eslint", fetch)
        self.assertIn("%2F", seen[0])
        self.assertTrue(seen[0].endswith("%40nuxt%2Feslint") or seen[0].endswith("@nuxt%2Feslint"))


class TheAgeVerdict(unittest.TestCase):
    def judge(self, deps, releases, text="", limit=LIMIT, missing=None):
        return release_age.evaluate(deps, releases, allowlist(text), TODAY, limit, missing)

    def test_a_fresh_dependency_passes(self):
        releases = {("widget", "cargo"): release_age.Release("1.0", datetime.date(2026, 9, 1))}
        rows, failures, _ = self.judge([dependency()], releases)
        self.assertEqual(failures, [])
        self.assertEqual(rows[0].verdict, "ok")

    def test_an_old_dependency_fails_and_is_named(self):
        releases = {("widget", "cargo"): release_age.Release("0.12.0", datetime.date(2022, 10, 6))}
        _, failures, _ = self.judge([dependency()], releases)
        self.assertEqual(len(failures), 1)
        self.assertIn("widget", failures[0])

    def test_the_failure_states_the_release_date_and_the_age(self):
        releases = {("widget", "cargo"): release_age.Release("0.12.0", datetime.date(2022, 10, 6))}
        _, failures, _ = self.judge([dependency()], releases)
        self.assertIn("2022-10-06", failures[0])
        self.assertIn("1464 days", failures[0])

    def test_exactly_at_the_limit_passes(self):
        """"More than 365 days" means 366 fails and 365 does not. Written down here so it
        is not argued about in a pull request."""
        at = TODAY - datetime.timedelta(days=LIMIT)
        rows, failures, _ = self.judge(
            [dependency()], {("widget", "cargo"): release_age.Release("1.0", at)}
        )
        self.assertEqual(failures, [])
        self.assertEqual(rows[0].age_days, LIMIT)

    def test_one_day_past_the_limit_fails(self):
        past = TODAY - datetime.timedelta(days=LIMIT + 1)
        _, failures, _ = self.judge(
            [dependency()], {("widget", "cargo"): release_age.Release("1.0", past)}
        )
        self.assertEqual(len(failures), 1)

    def test_an_allowlist_entry_with_a_reason_makes_it_pass(self):
        releases = {("widget", "cargo"): release_age.Release("0.12.0", datetime.date(2022, 10, 6))}
        rows, failures, _ = self.judge([dependency()], releases, ENTRY.format(date="2027-01-31"))
        self.assertEqual(failures, [])
        self.assertEqual(rows[0].verdict, "allowed")

    def test_the_same_entry_with_a_past_look_again_date_fails(self):
        releases = {("widget", "cargo"): release_age.Release("0.12.0", datetime.date(2022, 10, 6))}
        _, failures, _ = self.judge([dependency()], releases, ENTRY.format(date="2026-09-01"))
        self.assertEqual(len(failures), 1)
        self.assertIn("EXPIRED", failures[0])
        self.assertIn("Patrick", failures[0])

    def test_look_again_today_is_not_yet_expired(self):
        releases = {("widget", "cargo"): release_age.Release("0.12.0", datetime.date(2022, 10, 6))}
        _, failures, _ = self.judge([dependency()], releases, ENTRY.format(date="2026-10-09"))
        self.assertEqual(failures, [])

    def test_an_entry_spelled_with_a_hyphen_still_matches(self):
        """crates.io treats `-` and `_` as the same character, so `serde-norway` fetches
        cleanly. An entry spelled that way must still match `serde_norway`."""
        releases = {("serde_norway", "cargo"): release_age.Release("0.9.42", datetime.date(2024, 12, 21))}
        text = ENTRY.format(date="2027-01-31").replace("widget", "serde-norway")
        rows, failures, _ = self.judge(
            [dependency(name="serde_norway")], releases, text
        )
        self.assertEqual(failures, [])
        self.assertEqual(rows[0].verdict, "allowed")

    def test_an_orphaned_entry_fails(self):
        """Otherwise an entry lands in one pull request as housekeeping and the dependency
        it excuses lands in the next, and both are green."""
        _, failures, _ = self.judge([], {}, ENTRY.format(date="2027-01-31"))
        self.assertTrue(any("ORPHANED" in f for f in failures))

    def test_an_entry_that_is_no_longer_needed_is_a_note_not_a_failure(self):
        releases = {("widget", "cargo"): release_age.Release("1.0", datetime.date(2026, 9, 1))}
        _, failures, notes = self.judge([dependency()], releases, ENTRY.format(date="2027-01-31"))
        self.assertEqual(failures, [])
        self.assertTrue(any("no longer needs" in n for n in notes))

    def test_a_git_dependency_fails_rather_than_being_skipped(self):
        """A git dependency is a way to add something no registry can date. Skipping it
        would make the check trivial to walk around."""
        dep = dependency(ecosystem="git", requirement="https://github.com/x/y")
        _, failures, _ = self.judge([dep], {})
        self.assertEqual(len(failures), 1)
        self.assertIn("NOT DATABLE", failures[0])

    def test_a_git_dependency_can_be_allowlisted(self):
        dep = dependency(ecosystem="git", requirement="https://github.com/x/y")
        _, failures, _ = self.judge([dep], {}, ENTRY.format(date="2027-01-31"))
        self.assertEqual(failures, [])

    def test_a_package_the_registry_does_not_have_fails_by_name(self):
        missing = {("widget", "cargo"): "the registry has no such package"}
        _, failures, _ = self.judge([dependency()], {}, missing=missing)
        self.assertEqual(len(failures), 1)
        self.assertIn("NO SUCH", failures[0])
        self.assertIn("widget", failures[0])

    def test_a_dependency_with_no_release_in_the_results_is_not_a_pass(self):
        _, failures, _ = self.judge([dependency()], {})
        self.assertEqual(len(failures), 1)
        self.assertIn("NOT CHECKED", failures[0])

    def test_a_malformed_allowlist_entry_fails_the_check(self):
        text = ENTRY.format(date="2027-01-31").replace("look_again", "look-again")
        releases = {("widget", "cargo"): release_age.Release("1.0", datetime.date(2026, 9, 1))}
        _, failures, _ = self.judge([dependency()], releases, text)
        self.assertTrue(failures)
        self.assertTrue(any("look_again" in f for f in failures))

    def test_a_pinned_version_behind_the_limit_is_a_note_not_a_failure(self):
        release = release_age.Release(
            "8.20.0",
            datetime.date(2026, 4, 24),
            pinned="8.17.1",
            pinned_published=datetime.date(2024, 7, 13),
        )
        _, failures, notes = self.judge(
            [dependency(name="ajv", ecosystem="npm", where="contract/package.json [devDependencies]")],
            {("ajv", "npm"): release},
        )
        self.assertEqual(failures, [])
        self.assertTrue(any("pinned at 8.17.1" in n for n in notes))


class TheInterpreterGuard(unittest.TestCase):
    def test_tomllib_is_imported_below_the_version_guard(self):
        """Importing tomllib above the guard meant the guard could never fire: on 3.10 the
        script died with ModuleNotFoundError instead of the sentence the workflow and the
        docs both promise."""
        for name in ("release_age.py", "dependency_allowlist.py"):
            source = (Path(__file__).resolve().parent / name).read_text(encoding="utf-8")
            guard = source.index("sys.version_info < (3, 11)")
            self.assertGreater(
                source.index("import tomllib"), guard, f"{name} imports tomllib too early"
            )


class ReadingTheManifests(unittest.TestCase):
    def manifest(self, name: str, text: str) -> Path:
        path = Path(self.tmp.name) / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def setUp(self):
        self.tmp = TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def test_a_workspace_inherited_entry_is_counted_once(self):
        self.manifest("Cargo.toml", '[workspace.dependencies]\nserde = "1.0"\n')
        self.manifest("crates/a/Cargo.toml", "[dependencies]\nserde.workspace = true\n")
        found = release_age.read_cargo_dependencies(self.root)
        self.assertEqual([d.name for d in found], ["serde"])

    def test_a_path_dependency_is_not_a_registry_dependency(self):
        self.manifest("Cargo.toml", '[dependencies]\nstore = { path = "../store" }\n')
        self.assertEqual(release_age.read_cargo_dependencies(self.root), [])

    def test_a_renamed_crate_is_looked_up_under_its_published_name(self):
        self.manifest("Cargo.toml", '[dependencies]\nlocal = { package = "real-crate", version = "1" }\n')
        found = release_age.read_cargo_dependencies(self.root)
        self.assertEqual([d.name for d in found], ["real-crate"])

    def test_a_target_specific_dependency_is_read(self):
        self.manifest(
            "Cargo.toml", "[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n"
        )
        found = release_age.read_cargo_dependencies(self.root)
        self.assertEqual([d.name for d in found], ["libc"])

    def test_a_build_dependency_is_read(self):
        self.manifest("Cargo.toml", '[build-dependencies]\ncc = "1.0"\n')
        self.assertEqual([d.name for d in release_age.read_cargo_dependencies(self.root)], ["cc"])

    def test_a_git_cargo_dependency_is_kept_with_a_non_registry_ecosystem(self):
        self.manifest("Cargo.toml", '[dependencies]\nx = { git = "https://example.invalid/x" }\n')
        found = release_age.read_cargo_dependencies(self.root)
        self.assertEqual(found[0].ecosystem, "git")

    def test_an_alternative_registry_is_kept_too(self):
        self.manifest("Cargo.toml", '[dependencies]\nx = { version = "1", registry = "inhouse" }\n')
        self.assertEqual(release_age.read_cargo_dependencies(self.root)[0].ecosystem, "other-registry")

    def test_a_cargo_exact_pin_is_recognised(self):
        self.manifest("Cargo.toml", '[dependencies]\nx = "=1.2.3"\n')
        self.assertEqual(release_age.read_cargo_dependencies(self.root)[0].exact_pin, "1.2.3")

    def test_a_cargo_caret_range_is_not_a_pin(self):
        self.manifest("Cargo.toml", '[dependencies]\nx = "1.2"\n')
        self.assertIsNone(release_age.read_cargo_dependencies(self.root)[0].exact_pin)

    def test_every_package_json_is_found_not_just_the_two_we_have(self):
        """Naming web/package.json and contract/package.json would mean a new
        e2e/package.json gave you a job that runs, reads nothing new, and passes."""
        self.manifest("web/package.json", '{"dependencies":{"nuxt":"^4.6.0"}}')
        self.manifest("e2e/package.json", '{"dependencies":{"smuggled":"^1.0.0"}}')
        found = release_age.read_npm_dependencies(self.root)
        self.assertIn("smuggled", [d.name for d in found])

    def test_node_modules_is_not_walked(self):
        self.manifest("web/node_modules/left-pad/package.json", '{"dependencies":{"x":"1"}}')
        self.assertEqual(release_age.read_npm_dependencies(self.root), [])

    def test_a_workspace_specifier_is_local(self):
        self.manifest("web/package.json", '{"dependencies":{"shared":"workspace:*"}}')
        self.assertEqual(release_age.read_npm_dependencies(self.root), [])

    def test_an_npm_alias_is_looked_up_under_the_aliased_package(self):
        self.manifest("web/package.json", '{"dependencies":{"vue":"npm:dead-fork@1.0.0"}}')
        found = release_age.read_npm_dependencies(self.root)
        self.assertEqual([d.name for d in found], ["dead-fork"])

    def test_a_github_specifier_is_not_datable(self):
        self.manifest("web/package.json", '{"dependencies":{"x":"github:someone/x"}}')
        self.assertEqual(release_age.read_npm_dependencies(self.root)[0].ecosystem, "git")

    def test_an_optional_dependency_is_read(self):
        self.manifest("web/package.json", '{"optionalDependencies":{"fsevents":"^2.3.3"}}')
        self.assertEqual([d.name for d in release_age.read_npm_dependencies(self.root)], ["fsevents"])

    def test_a_peer_dependency_is_read(self):
        """Leaving peerDependencies out was a way to declare a direct dependency that
        nothing watched — the hole this tool argues against."""
        self.manifest("web/package.json", '{"peerDependencies":{"vue":"^3.5.0"}}')
        self.assertEqual([d.name for d in release_age.read_npm_dependencies(self.root)], ["vue"])

    def test_an_npm_exact_pin_is_recognised(self):
        self.manifest("web/package.json", '{"devDependencies":{"ajv":"8.17.1"}}')
        self.assertEqual(release_age.read_npm_dependencies(self.root)[0].exact_pin, "8.17.1")

    def test_an_npm_caret_range_is_not_a_pin(self):
        self.manifest("web/package.json", '{"devDependencies":{"ajv":"^8.17.1"}}')
        self.assertIsNone(release_age.read_npm_dependencies(self.root)[0].exact_pin)

    def test_a_manifest_that_is_not_json_cannot_run(self):
        self.manifest("web/package.json", "{not json")
        with self.assertRaises(release_age.CouldNotRun):
            release_age.read_npm_dependencies(self.root)

    def test_a_manifest_that_is_not_toml_cannot_run(self):
        self.manifest("Cargo.toml", "[dependencies\nbroken")
        with self.assertRaises(release_age.CouldNotRun):
            release_age.read_cargo_dependencies(self.root)


class WhenTheRegistryCannotBeReached(unittest.TestCase):
    """Patrick's rule: a check that passes without running carries no information."""

    def test_a_connection_failure_cannot_run(self):
        error = urllib.error.URLError("Connection refused")
        with self.assertRaises(release_age.CouldNotRun):
            release_age.crates_io_release("x", fetcher(error))

    def test_no_dependencies_at_all_cannot_run(self):
        """An empty result is the shape a parsing bug takes, and it must not read as a
        clean bill of health for a repository with 35 dependencies."""
        with TemporaryDirectory() as tmp:
            code = release_age.main(["--root", tmp, "--today", "2026-10-09"])
        self.assertEqual(code, release_age.EXIT_COULD_NOT_RUN)

    def test_a_corrupt_gzip_body_is_reported_as_malformed_not_as_an_outage(self):
        """BadGzipFile subclasses OSError, so its clause was unreachable and a corrupt body
        read as an unreachable registry — wrong for a tool whose contract is that a reader
        can tell an outage from a bad payload."""

        def fetch(url):
            raise gzip.BadGzipFile("Not a gzipped file")

        with self.assertRaises(release_age.CouldNotRun) as caught:
            release_age.crates_io_release("x", fetch)
        self.assertIn("malformed response body", str(caught.exception))

    def test_a_404_is_not_an_outage(self):
        """Both registries answer 404 for a name that never existed, a name spelled with
        the wrong case, and a package that has been unpublished. That is an answer."""
        def fetch(url):
            raise urllib.error.HTTPError("http://x", 404, "Not Found", {}, None)

        with self.assertRaises(release_age.NoSuchPackage):
            release_age.crates_io_release("x", fetch)

    def test_fetch_releases_records_a_missing_package_rather_than_raising(self):
        def fetch(url):
            raise urllib.error.HTTPError("http://x", 404, "Not Found", {}, None)

        releases, missing = release_age.fetch_releases([dependency()], fetch, sleep=lambda _: None)
        self.assertEqual(releases, {})
        self.assertIn(("widget", "cargo"), missing)


class TheCountInvariant(unittest.TestCase):
    def test_every_enumerated_dependency_is_judged(self):
        deps = [dependency(name=f"d{i}") for i in range(5)]
        releases = {
            (d.name, "cargo"): release_age.Release("1.0", datetime.date(2026, 9, 1)) for d in deps
        }
        rows, failures, _ = release_age.evaluate(deps, releases, dependency_allowlist.EMPTY, TODAY)
        self.assertEqual(len(rows), len(deps))
        self.assertEqual(failures, [])

    def test_the_report_states_both_counts(self):
        deps = [dependency()]
        releases = {("widget", "cargo"): release_age.Release("1.0", datetime.date(2026, 9, 1))}
        rows, failures, notes = release_age.evaluate(
            deps, releases, dependency_allowlist.EMPTY, TODAY
        )
        text = release_age.render_text(
            rows, failures, notes, TODAY, LIMIT, ["Cargo.toml"], len(deps)
        )
        self.assertIn("Enumerated 1 direct dependencies, judged 1", text)

    def test_the_report_shows_a_discrepancy_between_the_counts(self):
        """The line existed to show a mismatch and printed the same number twice, so it
        could never show one. Three enumerated, one judged, and the report has to say so."""
        rows, _, _ = release_age.evaluate(
            [dependency()],
            {("widget", "cargo"): release_age.Release("1.0", datetime.date(2026, 9, 1))},
            dependency_allowlist.EMPTY,
            TODAY,
        )
        text = release_age.render_text(rows, [], [], TODAY, LIMIT, ["Cargo.toml"], 3)
        self.assertIn("Enumerated 3 direct dependencies, judged 1", text)


class TheMarkdownTable(unittest.TestCase):
    def test_it_renders_one_row_per_dependency(self):
        rows, _, _ = release_age.evaluate(
            [dependency()],
            {("widget", "cargo"): release_age.Release("1.0", datetime.date(2026, 9, 1))},
            dependency_allowlist.EMPTY,
            TODAY,
        )
        table = release_age.render_markdown(rows, TODAY, LIMIT)
        self.assertIn("| `widget` |", table)
        self.assertIn("1 direct dependencies", table)


if __name__ == "__main__":
    unittest.main()
