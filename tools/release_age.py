#!/usr/bin/env python3
"""Fail when a direct dependency's latest release is more than a year old.

**An advisory scan would not have caught `tiny_http`.** It had no open advisory; it was
simply abandoned, with no release since 6 October 2022, and Review Desk found out when its
connection handling hung the dashboard. This check is the one that would have.

It walks every Cargo manifest and every `package.json` a person wrote, asks crates.io and
the npm registry when each direct dependency last published a real release, and fails on
anything over `--max-age-days` (365 by default) that is not in `dependency-allowlist.toml`.

Four rules shape it, and the first three are Patrick's:

1. **It never passes without having run.** Every registry failure — DNS, connection,
   timeout, a 429, a 5xx, a payload that is not the shape it should be — exits 2 with a
   message saying the check could not run. There is no path from an unreachable registry
   to a pass, because a check that passes without running carries no information. The
   count of dependencies judged is asserted against the count enumerated, and both are
   printed, so "it ran" is a number on the page and not a belief.
2. **A dependency it cannot ask a registry about is a failure, not a skip.** A `git`
   dependency, an alternative registry, a `github:` npm specifier: each is a way to add a
   dependency no registry can date. Skipping them would make the check trivial to walk
   around, so they fail and have to be allowlisted deliberately.
3. **It identifies itself and nobody else.** Requests carry a User-Agent naming this
   repository. No person's name or email address appears in any header or URL.
4. **It looks everywhere, not where it was told.** The manifests are found by walking the
   tree. Naming `web/package.json` and `contract/package.json` would mean a new
   `e2e/package.json` gave you a job that runs, reads nothing new, and passes.

**What it measures, and what it does not.** This is the age of the newest release upstream
has published — whether anyone is still working on the dependency. It is not how far behind
that release you are. A dependency three majors out of date passes if upstream shipped last
month. Where a manifest pins an exact version, the age of *that* release is reported too,
as a note rather than a failure; `docs/dependencies.md` says why.

Standard library only, so the repository gains no dependency for the sake of a check that
watches dependencies.

    python3 tools/release_age.py                      # check, human-readable
    python3 tools/release_age.py --format markdown    # the audit table, for a pull request

Exit codes: 0 everything within the limit, 1 the check ran and the answer is no, 2 the
check could not run.
"""

from __future__ import annotations

import argparse
import datetime
import gzip
import json
import os
import re
import ssl
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path

# Before `tomllib` is imported, not after. Importing it first meant this guard could never
# fire: on 3.10 the script died with `ModuleNotFoundError: No module named 'tomllib'`, a
# traceback instead of the sentence, while the workflow and the docs both claimed otherwise.
if sys.version_info < (3, 11):  # pragma: no cover - the guard exists so CI says why
    sys.exit(
        "release_age.py needs Python 3.11 or newer for tomllib; "
        f"this is {sys.version.split()[0]}"
    )

import tomllib  # noqa: E402  (below the guard, deliberately)

sys.path.insert(0, str(Path(__file__).resolve().parent))

import dependency_allowlist  # noqa: E402  (after the sys.path line, deliberately)

EXIT_OK = 0
EXIT_POLICY = 1
EXIT_COULD_NOT_RUN = 2

DEFAULT_MAX_AGE_DAYS = 365

# Generic on purpose. crates.io answers 403 with a policy message when a request carries no
# User-Agent, or one that gives it no way to get in touch; the repository is that way. A
# person's name or email address here would be sent to two third parties on every run,
# which is why neither is here.
USER_AGENT = "review-desk-release-age-check (+https://github.com/devergehq/review-desk)"

CRATES_API = os.environ.get("REVIEW_DESK_CRATES_API", "https://crates.io/api/v1/crates")
NPM_REGISTRY = os.environ.get("REVIEW_DESK_NPM_REGISTRY", "https://registry.npmjs.org")

# A courtesy, not a workaround: 45 back-to-back requests to crates.io were not throttled
# when this was measured, and the job makes thirteen. The pause costs five seconds and
# keeps a future rate limit from reading as a registry outage.
CRATES_PAUSE_SECONDS = float(os.environ.get("REVIEW_DESK_CRATES_PAUSE", "0.4"))
NPM_PAUSE_SECONDS = float(os.environ.get("REVIEW_DESK_NPM_PAUSE", "0.1"))

ATTEMPTS = 3
BACKOFF_SECONDS = (1.0, 3.0)
TIMEOUT_SECONDS = 30

CARGO_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies")
# `peerDependencies` is here because leaving it out was a way to declare a direct
# dependency that nothing watched -- the hole this tool argues against.
NPM_SECTIONS = ("dependencies", "devDependencies", "optionalDependencies", "peerDependencies")

# npm specifiers naming something inside this repository rather than a published package.
NPM_LOCAL_PREFIXES = ("workspace:", "file:", "link:", "portal:", "catalog:")

# Directories holding installed or generated code rather than a manifest somebody wrote.
NOT_SOURCE = {"node_modules", "target", ".git", ".nuxt", ".output", "dist", ".data", "__pycache__"}

EXACT_NPM_VERSION = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.\-+]+)?$")


class CouldNotRun(Exception):
    """The check could not reach a conclusion. Always exit 2, never a pass."""


class NoSuchPackage(Exception):
    """The registry answered, and the answer is that there is no such package.

    A conclusion, not an outage, so it fails the check rather than reporting that the check
    could not run. Both registries return the same 404 for a name that never existed, a
    name spelled with the wrong case, and a package that has been unpublished — and an
    unpublished dependency is a harder problem than a stale one, not a flaky runner."""


@dataclass(frozen=True)
class Dependency:
    name: str
    ecosystem: str
    requirement: str
    where: str

    @property
    def key(self) -> tuple[str, str]:
        return (normalise(self.name, self.ecosystem), self.ecosystem)

    @property
    def label(self) -> str:
        return f"{self.name} ({self.ecosystem})"

    @property
    def exact_pin(self) -> str | None:
        """The one version this manifest allows, when it allows only one.

        Cargo spells that `=1.2.3`; a bare `"1.2"` is a caret range. npm spells it as a
        bare version; anything with `^`, `~`, a comparison or a wildcard is a range."""
        value = self.requirement.strip()
        if self.ecosystem == "cargo":
            return value[1:].strip() if value.startswith("=") else None
        if self.ecosystem == "npm":
            return value if EXACT_NPM_VERSION.match(value) else None
        return None


def normalise(name: str, ecosystem: str) -> str:
    """The name to compare on.

    crates.io normalises case and treats `-` and `_` as the same character: a GET for
    `serde-norway` answers 200 with `"name": "serde_norway"`. An allowlist entry spelled
    the other way round would fetch cleanly and then never match the manifest key, and no
    field would be missing or unknown for the allowlist's own validation to catch. So both
    sides are normalised before they are compared."""
    if ecosystem == "cargo":
        return name.lower().replace("-", "_")
    return name.lower()


@dataclass(frozen=True)
class Release:
    version: str
    published: datetime.date
    # The newest pre-release, when it is newer than the release above. Reported, never
    # used for the verdict.
    prerelease: str | None = None
    prerelease_published: datetime.date | None = None
    # The publish date of the exact version a manifest pins, when it pins one.
    pinned: str | None = None
    pinned_published: datetime.date | None = None

    def age_days(self, today: datetime.date) -> int:
        return (today - self.published).days

    def pinned_age_days(self, today: datetime.date) -> int | None:
        if self.pinned_published is None:
            return None
        return (today - self.pinned_published).days


def is_prerelease(version: str) -> bool:
    """Semver says a pre-release is anything with a hyphen after the version core, and
    neither registry allows a hyphen anywhere else. `0.0.0-snapshot.1791443981` and
    `7.1.0-dev.20261008.1` are both pre-releases; `0.8.9` and `1.0.0+build` are not."""
    return "-" in version.split("+", 1)[0]


def _version_key(version: str) -> tuple:
    """Enough version order to break a tie between two versions published the same day.
    Not a semver implementation, and it does not need to be."""
    core = version.split("+", 1)[0].split("-", 1)[0]
    return tuple(int(part) if part.isdigit() else -1 for part in core.split("."))


def newest_release(
    published: list[tuple[str, datetime.date]], what: str, pin: str | None = None
) -> Release:
    """The release an age is measured from, given every published version and its date.

    **Pre-releases do not count towards being maintained.** Measuring from any version at
    all made this check report `@redocly/cli` as `0.0.0-snapshot.1791443981` and
    `typescript` as `7.1.0-dev.20261008.1` — true, unrecognisable, and the wrong question.
    You cannot depend on a nightly, so a package whose last real release was two years ago
    is two years old however many snapshots it has pushed since. The newest pre-release is
    still reported, so a reader can see the project is moving. Only a package with nothing
    but pre-releases is dated from one, because the alternative is calling a package with
    no stable release infinitely old.

    `max` is taken over the publish date rather than the version number on purpose: a crate
    that publishes a patch to an older minor line is being maintained, and taking the
    highest version number would call it abandoned."""
    if not published:
        raise CouldNotRun(f"{what}: no published versions at all")

    stable = [(v, d) for v, d in published if not is_prerelease(v)]
    pre = [(v, d) for v, d in published if is_prerelease(v)]

    version, date = max(stable or pre, key=lambda item: (item[1], _version_key(item[0])))

    newest_pre = max(pre, key=lambda item: (item[1], _version_key(item[0]))) if pre else None
    if newest_pre is not None and (not stable or newest_pre[1] > date):
        prerelease, prerelease_published = newest_pre
    else:
        prerelease, prerelease_published = None, None

    pinned_published = None
    if pin is not None:
        pinned_published = dict(published).get(pin)

    return Release(
        version=version,
        published=date,
        prerelease=prerelease,
        prerelease_published=prerelease_published,
        pinned=pin if pinned_published is not None else None,
        pinned_published=pinned_published,
    )


# --------------------------------------------------------------------------- manifests


def _cargo_entry(name: str, value: object, where: str) -> Dependency | None:
    """One `[dependencies]` entry. None for entries counted somewhere else; a Dependency
    with a non-registry ecosystem for ones no registry can date, which become failures
    rather than being dropped."""
    if isinstance(value, str):
        return Dependency(name=name, ecosystem="cargo", requirement=value, where=where)

    if not isinstance(value, dict):
        return Dependency(name=name, ecosystem="unknown", requirement=repr(value), where=where)

    # `foo.workspace = true` inherits the workspace entry, which is read separately.
    if value.get("workspace") is True:
        return None

    # A path dependency is a crate in this repository.
    if "path" in value:
        return None

    # The real crate name, when the key is a rename: `foo = { package = "bar" }`.
    published_name = value.get("package", name)
    if not isinstance(published_name, str):
        published_name = name

    if "git" in value:
        return Dependency(
            name=published_name, ecosystem="git", requirement=str(value.get("git")), where=where
        )

    if "registry" in value or "registry-index" in value:
        return Dependency(
            name=published_name,
            ecosystem="other-registry",
            requirement=str(value.get("registry") or value.get("registry-index")),
            where=where,
        )

    return Dependency(
        name=published_name,
        ecosystem="cargo",
        requirement=str(value.get("version", "*")),
        where=where,
    )


def _cargo_tables(data: dict) -> list[tuple[str, dict]]:
    """Every table in a Cargo manifest that names dependencies, including the ones under
    `[target.'cfg(...)']`, which are as real as any other."""
    tables: list[tuple[str, dict]] = []

    workspace = data.get("workspace")
    if isinstance(workspace, dict) and isinstance(workspace.get("dependencies"), dict):
        tables.append(("workspace.dependencies", workspace["dependencies"]))

    for section in CARGO_SECTIONS:
        if isinstance(data.get(section), dict):
            tables.append((section, data[section]))

    targets = data.get("target")
    if isinstance(targets, dict):
        for triple, table in sorted(targets.items()):
            if not isinstance(table, dict):
                continue
            for section in CARGO_SECTIONS:
                if isinstance(table.get(section), dict):
                    tables.append((f"target.{triple}.{section}", table[section]))

    return tables


def _source_files(root: Path, filename: str) -> list[Path]:
    found = []
    for path in root.rglob(filename):
        if set(path.relative_to(root).parts) & NOT_SOURCE:
            continue
        found.append(path)
    return sorted(found)


def cargo_manifests(root: Path) -> list[Path]:
    return _source_files(root, "Cargo.toml")


def npm_manifests(root: Path) -> list[Path]:
    return _source_files(root, "package.json")


def read_cargo_dependencies(root: Path) -> list[Dependency]:
    out: dict[tuple[str, str], Dependency] = {}
    for path in cargo_manifests(root):
        relative = path.relative_to(root).as_posix()
        try:
            data = tomllib.loads(path.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError) as exc:
            raise CouldNotRun(f"could not read {relative}: {exc}") from exc
        for section, table in _cargo_tables(data):
            for name, value in sorted(table.items()):
                dependency = _cargo_entry(name, value, f"{relative} [{section}]")
                if dependency is not None:
                    out.setdefault(dependency.key, dependency)
    return sorted(out.values(), key=lambda d: (d.ecosystem, d.name))


def _npm_entry(name: str, requirement: object, where: str) -> Dependency | None:
    if not isinstance(requirement, str):
        return Dependency(name=name, ecosystem="unknown", requirement=repr(requirement), where=where)

    value = requirement.strip()
    if not value:
        # `"foo": ""` means any version to npm. It is still a published package.
        return Dependency(name=name, ecosystem="npm", requirement="*", where=where)

    if any(value.startswith(prefix) for prefix in NPM_LOCAL_PREFIXES):
        return None

    # `"foo": "npm:@scope/bar@^1"` — an alias. The published package is the aliased one.
    if value.startswith("npm:"):
        aliased = value[len("npm:") :]
        at = aliased.rfind("@")
        return Dependency(
            name=aliased[:at] if at > 0 else aliased, ecosystem="npm", requirement=value, where=where
        )

    if (
        value.startswith(("git+", "git:", "github:", "gitlab:", "bitbucket:", "http://", "https://"))
        or "://" in value
        or (value.count("/") == 1 and not value.startswith("@") and not value[0].isdigit())
    ):
        return Dependency(name=name, ecosystem="git", requirement=value, where=where)

    return Dependency(name=name, ecosystem="npm", requirement=value, where=where)


def read_npm_dependencies(root: Path) -> list[Dependency]:
    out: dict[tuple[str, str], Dependency] = {}
    for path in npm_manifests(root):
        relative = path.relative_to(root).as_posix()
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise CouldNotRun(f"could not read {relative}: {exc}") from exc
        for section in NPM_SECTIONS:
            table = data.get(section)
            if not isinstance(table, dict):
                continue
            for name, requirement in sorted(table.items()):
                dependency = _npm_entry(name, requirement, f"{relative} [{section}]")
                if dependency is not None:
                    out.setdefault(dependency.key, dependency)
    return sorted(out.values(), key=lambda d: (d.ecosystem, d.name))


# --------------------------------------------------------------------------- registries


def http_get(url: str) -> bytes:
    """One GET, retried on the failures worth retrying.

    `Accept-Encoding: gzip` is not optional here: urllib does not send it, and the npm
    packuments for these dependencies are 44 MB uncompressed against 6.4 MB gzipped. The
    abbreviated packument is smaller still but carries no `time` map at all, so the full
    document is the only one that can answer this question."""
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": USER_AGENT,
            "Accept": "application/json",
            "Accept-Encoding": "gzip",
        },
    )

    last = ""
    for attempt in range(ATTEMPTS):
        try:
            with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
                body = response.read()
                if response.headers.get("Content-Encoding") == "gzip":
                    body = gzip.decompress(body)
                return body
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                raise NoSuchPackage(f"{url} — the registry has no such package") from exc
            last = f"HTTP {exc.code} {exc.reason}"
            # Retrying will not talk a 403 policy response or a 400 into changing its mind.
            retryable = exc.code == 429 or 500 <= exc.code < 600
        # Before the OSError clause: BadGzipFile subclasses OSError, so ordering it after
        # made it unreachable and reported a corrupt body as an unreachable registry.
        except (gzip.BadGzipFile, EOFError) as exc:
            last = f"malformed response body: {exc}"
            retryable = True
        except (urllib.error.URLError, ssl.SSLError, OSError) as exc:
            last = str(getattr(exc, "reason", exc)) or exc.__class__.__name__
            retryable = True

        if not retryable or attempt == ATTEMPTS - 1:
            break
        time.sleep(BACKOFF_SECONDS[min(attempt, len(BACKOFF_SECONDS) - 1)])

    raise CouldNotRun(f"could not reach {url}: {last}")


def ask(fetch, url: str, what: str) -> dict:
    """Call the fetcher and normalise everything it can throw.

    The translation lives here rather than only in `http_get` because the fetcher is a
    function argument: a caller that passes its own has to get the same contract, or the
    tests prove something the real run does not do. Three outcomes only — a document, a
    NoSuchPackage, or a CouldNotRun. There is no fourth that means "carry on"."""
    try:
        body = fetch(url)
    except (NoSuchPackage, CouldNotRun):
        raise
    # Before the OSError clause: BadGzipFile subclasses OSError, so ordering it after
    # reports a corrupt payload as an unreachable registry. The two are different problems
    # and a reader has to be able to tell them apart.
    except (gzip.BadGzipFile, EOFError) as exc:
        raise CouldNotRun(f"{what}: malformed response body: {exc}") from exc
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            raise NoSuchPackage(f"{url} — the registry has no such package") from exc
        raise CouldNotRun(f"could not reach {url}: HTTP {exc.code} {exc.reason}") from exc
    except (urllib.error.URLError, ssl.SSLError, OSError) as exc:
        raise CouldNotRun(f"could not reach {url}: {getattr(exc, 'reason', exc)}") from exc

    try:
        payload = json.loads(body)
    except json.JSONDecodeError as exc:
        raise CouldNotRun(f"{what} sent something that is not JSON: {exc}") from exc
    if not isinstance(payload, dict):
        raise CouldNotRun(f"{what} sent {type(payload).__name__}, not an object")
    return payload


def _parse_timestamp(value: object, what: str) -> datetime.date:
    if not isinstance(value, str):
        raise CouldNotRun(f"{what}: expected a timestamp, got {value!r}")
    try:
        return (
            datetime.datetime.fromisoformat(value.strip().replace("Z", "+00:00"))
            .astimezone(datetime.timezone.utc)
            .date()
        )
    except ValueError as exc:
        raise CouldNotRun(f"{what}: {value!r} is not a timestamp I can read: {exc}") from exc


def crates_io_release(name: str, fetch=http_get, pin: str | None = None) -> Release:
    """The newest release of a crate, ignoring yanked versions.

    Reads the top-level `versions` array: `crate.versions` is a list of integer ids, not
    objects, and reaching for it is the mistake this comment exists to prevent."""
    url = f"{CRATES_API}/{urllib.parse.quote(name, safe='')}"
    payload = ask(fetch, url, f"crates.io for {name}")

    versions = payload.get("versions")
    if not isinstance(versions, list) or not versions:
        raise CouldNotRun(f"crates.io listed no versions for {name}")

    published = [
        (str(v.get("num", "?")), _parse_timestamp(v.get("created_at"), f"crates.io {name}"))
        for v in versions
        if isinstance(v, dict) and not v.get("yanked")
    ]
    if not published:
        raise CouldNotRun(f"every published version of {name} is yanked")
    return newest_release(published, f"crates.io {name}", pin)


def npm_release(name: str, fetch=http_get, pin: str | None = None) -> Release:
    """The newest release of an npm package.

    Reads the whole `time` map rather than `time[dist-tags.latest]`: a package whose
    `latest` tag points at an old version while work continues under `next` is maintained,
    and `dist-tags` can be absent altogether. `time.modified` is deliberately not used —
    DefinitelyTyped republishes tags, which moved `@types/markdown-it`'s `modified` 48 days
    ahead of its newest actual release."""
    url = f"{NPM_REGISTRY}/{urllib.parse.quote(name, safe='@')}"
    payload = ask(fetch, url, f"the npm registry for {name}")

    times = payload.get("time")
    if not isinstance(times, dict):
        raise CouldNotRun(f"the npm registry gave {name} no publish times")

    published = [
        (version, _parse_timestamp(stamp, f"npm {name} {version}"))
        for version, stamp in times.items()
        # `created`, `modified` and `unpublished` sit in the same map and are not versions.
        if version not in ("created", "modified", "unpublished")
    ]
    if not published:
        raise CouldNotRun(f"the npm registry listed no published version of {name}")
    return newest_release(published, f"npm {name}", pin)


def fetch_releases(
    dependencies: list[Dependency], fetch=http_get, sleep=time.sleep
) -> tuple[dict[tuple[str, str], Release], dict[tuple[str, str], str]]:
    """Ask the registries. Returns what was found and, separately, the dependencies the
    registry says do not exist — which is an answer, not an outage."""
    releases: dict[tuple[str, str], Release] = {}
    missing: dict[tuple[str, str], str] = {}
    first = True
    for dependency in dependencies:
        if dependency.ecosystem not in ("cargo", "npm"):
            continue
        if not first:
            sleep(CRATES_PAUSE_SECONDS if dependency.ecosystem == "cargo" else NPM_PAUSE_SECONDS)
        first = False
        ask = crates_io_release if dependency.ecosystem == "cargo" else npm_release
        try:
            releases[dependency.key] = ask(dependency.name, fetch, dependency.exact_pin)
        except NoSuchPackage as exc:
            missing[dependency.key] = str(exc)
    return releases, missing


# --------------------------------------------------------------------------- the verdict


@dataclass(frozen=True)
class Row:
    dependency: Dependency
    release: Release | None
    age_days: int | None
    allowlisted: dependency_allowlist.ReleaseAgeEntry | None
    verdict: str  # ok | allowed | too-old | expired | not-datable | missing
    detail: str
    note: str = ""


def evaluate(
    dependencies: list[Dependency],
    releases: dict[tuple[str, str], Release],
    allowlist: dependency_allowlist.Allowlist,
    today: datetime.date,
    max_age_days: int = DEFAULT_MAX_AGE_DAYS,
    missing: dict[tuple[str, str], str] | None = None,
) -> tuple[list[Row], list[str], list[str]]:
    """Judge each dependency. Pure: no clock, no network, no filesystem.

    Returns the rows, the failures, and the notes — notes being things worth reading that
    do not fail the check."""
    missing = missing or {}
    rows: list[Row] = []
    failures: list[str] = list(allowlist.problems)
    notes: list[str] = []

    for dependency in dependencies:
        entry = allowlist.release_age_for(dependency.name, dependency.ecosystem)

        if dependency.ecosystem not in ("cargo", "npm"):
            # A git or alternative-registry dependency is excused under the ecosystem of
            # the manifest it came from, so the allowlist stays readable.
            entry = entry or allowlist.release_age_for(
                dependency.name, "cargo" if "Cargo.toml" in dependency.where else "npm"
            )
            detail = (
                f"a {dependency.ecosystem} dependency ({dependency.requirement}), "
                "so no registry can say when it was last released"
            )
            if entry is None:
                failures.append(
                    f"NOT DATABLE  {dependency.label} in {dependency.where} is {detail}. "
                    "Depend on a published release, or allowlist it with a reason."
                )
                rows.append(Row(dependency, None, None, None, "not-datable", detail))
            elif entry.expired(today):
                failures.append(_expired(dependency, entry, today, None))
                rows.append(Row(dependency, None, None, entry, "expired", detail))
            else:
                rows.append(Row(dependency, None, None, entry, "allowed", detail))
            continue

        if dependency.key in missing:
            # Deliberately not allowlistable. Every other finding is about a dependency
            # being old, which is a judgement call; this one is about a dependency that
            # cannot be installed at all, and there is nothing to weigh.
            failures.append(
                f"NO SUCH      {dependency.label} from {dependency.where}: "
                f"{missing[dependency.key]}. Check the spelling, or the package has been "
                "unpublished or renamed — either way nothing can be installed from it, so "
                "no allowlist entry excuses this."
            )
            rows.append(Row(dependency, None, None, entry, "missing", "no such package"))
            continue

        release = releases.get(dependency.key)
        if release is None:
            # Unreachable: fetch_releases either returns a release or records it missing.
            # If it ever happens it is a could-not-run, so it must not read as a pass.
            failures.append(
                f"NOT CHECKED  {dependency.label} has no release date in the results, "
                "so nothing was concluded about it"
            )
            rows.append(Row(dependency, None, None, entry, "not-datable", "no release date"))
            continue

        age = release.age_days(today)
        detail = f"{release.version} released {release.published} ({age} days ago)"
        if release.prerelease:
            detail += f"; newest pre-release {release.prerelease} on {release.prerelease_published}"

        note = ""
        pinned_age = release.pinned_age_days(today)
        # Only worth saying when the pin is behind. A manifest pinned at the newest release
        # is the same fact twice.
        if pinned_age is not None and pinned_age > max_age_days and release.pinned != release.version:
            note = (
                f"pinned at {release.pinned}, released {release.pinned_published}, "
                f"{pinned_age} days ago"
            )
            notes.append(
                f"{dependency.label} is pinned at {release.pinned} from "
                f"{release.pinned_published} ({pinned_age} days ago) while {release.version} "
                f"is {age} days old. This check does not fail on that — it measures whether "
                "upstream is alive, not how far behind you are."
            )

        if age <= max_age_days:
            if entry is not None and not entry.expired(today):
                notes.append(
                    f"{dependency.label} has an allowlist entry it no longer needs: its "
                    f"latest release is {age} days old, inside the {max_age_days}-day limit."
                )
            rows.append(Row(dependency, release, age, None, "ok", detail, note))
        elif entry is None:
            failures.append(
                f"TOO OLD      {dependency.label}: latest release {release.version} on "
                f"{release.published}, {age} days ago, over the {max_age_days}-day limit "
                f"(from {dependency.where})"
            )
            rows.append(Row(dependency, release, age, None, "too-old", detail, note))
        elif entry.expired(today):
            failures.append(_expired(dependency, entry, today, age))
            rows.append(Row(dependency, release, age, entry, "expired", detail, note))
        else:
            rows.append(Row(dependency, release, age, entry, "allowed", detail, note))

    failures.extend(_orphaned_entries(dependencies, allowlist))

    # The invariant that makes "it ran" checkable rather than assumed.
    if len(rows) != len(dependencies):
        failures.append(
            f"NOT CHECKED  {len(dependencies)} dependencies were enumerated but "
            f"{len(rows)} were judged"
        )

    return rows, failures, notes


def _expired(
    dependency: Dependency,
    entry: dependency_allowlist.ReleaseAgeEntry,
    today: datetime.date,
    age: int | None,
) -> str:
    was = f"is {age} days old and its" if age is not None else "has an"
    return (
        f"EXPIRED      {dependency.label} {was} allowlist entry ran out on "
        f"{entry.look_again}, {(today - entry.look_again).days} days ago. "
        f"{entry.decided_by} decided: {entry.reason}"
    )


def _orphaned_entries(
    dependencies: list[Dependency], allowlist: dependency_allowlist.Allowlist
) -> list[str]:
    """An allowlist entry for a dependency no manifest names any more.

    A failure, not a notice. Without it an entry can be added in one pull request as
    housekeeping and the dependency it excuses added in the next, which is green on both."""
    present = {d.key for d in dependencies}
    present |= {
        (normalise(d.name, "cargo" if "Cargo.toml" in d.where else "npm"), "cargo" if "Cargo.toml" in d.where else "npm")
        for d in dependencies
    }
    return [
        f"ORPHANED     the allowlist excuses {entry.dependency} ({entry.ecosystem}), which "
        "no manifest depends on. Delete the entry, or add the dependency in the same change."
        for entry in allowlist.release_age
        if (normalise(entry.dependency, entry.ecosystem), entry.ecosystem) not in present
    ]


# --------------------------------------------------------------------------- reporting


def render_text(
    rows: list[Row],
    failures: list[str],
    notes: list[str],
    today: datetime.date,
    max_age_days: int,
    manifests: list[str],
    enumerated: int | None = None,
) -> str:
    width = max((len(r.dependency.label) for r in rows), default=10)
    mark = {"ok": "  ", "allowed": "A ", "too-old": "! ", "expired": "! ", "not-datable": "? ", "missing": "! "}
    lines = [
        f"Release ages as at {today}, limit {max_age_days} days.",
        f"Read {len(manifests)} manifest(s): {', '.join(manifests)}",
        # Two counts from two places. Printing len(rows) twice was tautological: the line
        # existed to show a discrepancy and could never show one.
        f"Enumerated {len(rows) if enumerated is None else enumerated} direct "
        f"dependencies, judged {len(rows)}.",
        "",
    ]
    for row in sorted(rows, key=lambda r: (-(r.age_days if r.age_days is not None else 10**6), r.dependency.label)):
        line = f"{mark[row.verdict]}{row.dependency.label:<{width}}  {row.detail}"
        if row.note:
            line += f"\n    note: {row.note}"
        lines.append(line)
    lines.append("")
    if notes:
        lines.append(f"{len(notes)} note(s), none of which fail this check:")
        lines.extend(f"  - {note}" for note in notes)
        lines.append("")
    if failures:
        lines.append(f"{len(failures)} problem(s):")
        lines.extend(f"  {failure}" for failure in failures)
    else:
        lines.append(f"All {len(rows)} direct dependencies are within the limit.")
    return "\n".join(lines)


def render_markdown(rows: list[Row], today: datetime.date, max_age_days: int) -> str:
    verdicts = {
        "ok": "Within the limit",
        "allowed": "Allowlisted",
        "too-old": f"**Over {max_age_days} days**",
        "expired": "**Allowlist entry expired**",
        "not-datable": "**No registry can date it**",
        "missing": "**No such package**",
    }
    lines = ["| Dependency | Where | Latest release | Age | Finding |", "| -- | -- | -- | -- | -- |"]
    for row in sorted(rows, key=lambda r: (-(r.age_days if r.age_days is not None else 10**6), r.dependency.label)):
        release = f"{row.release.version} on {row.release.published}" if row.release else "—"
        age = f"{row.age_days} days" if row.age_days is not None else "—"
        finding = verdicts[row.verdict]
        if row.note:
            finding += f" ({row.note})"
        lines.append(
            f"| `{row.dependency.name}` | {row.dependency.ecosystem}, {row.dependency.where} "
            f"| {release} | {age} | {finding} |"
        )
    lines += ["", f"{len(rows)} direct dependencies, checked {today} against a {max_age_days}-day limit."]
    return "\n".join(lines)


# --------------------------------------------------------------------------- entry point


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Fail when a direct dependency's latest release is more than a year old.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--max-age-days", type=int, default=DEFAULT_MAX_AGE_DAYS)
    parser.add_argument("--allowlist", type=Path, default=None)
    parser.add_argument("--format", choices=("text", "markdown"), default="text")
    parser.add_argument(
        "--summary",
        type=Path,
        default=None,
        help="also write the markdown table here, whatever the verdict. One run, one pass "
        "over the registries: asking for the table separately would double the requests.",
    )
    parser.add_argument(
        "--today",
        type=datetime.date.fromisoformat,
        default=None,
        help="pretend today is this date; for testing the check itself",
    )
    args = parser.parse_args(argv)

    root: Path = args.root.resolve()
    today = args.today or datetime.datetime.now(datetime.timezone.utc).date()
    allowlist_path = args.allowlist or root / "dependency-allowlist.toml"

    try:
        allowlist = dependency_allowlist.read(allowlist_path, today=today)
        manifests = [p.relative_to(root).as_posix() for p in cargo_manifests(root) + npm_manifests(root)]
        dependencies = read_cargo_dependencies(root) + read_npm_dependencies(root)
        if not dependencies:
            raise CouldNotRun(f"found no direct dependencies under {root}; that cannot be right")
        releases, missing = fetch_releases(dependencies)
    except (CouldNotRun, dependency_allowlist.AllowlistError) as exc:
        print(f"release-age check could not run: {exc}", file=sys.stderr)
        print(
            "Nothing was concluded about any dependency. This is a failure, not a pass: a "
            "check that passes without running carries no information.",
            file=sys.stderr,
        )
        return EXIT_COULD_NOT_RUN

    rows, failures, notes = evaluate(
        dependencies, releases, allowlist, today, args.max_age_days, missing
    )

    if args.format == "markdown":
        print(render_markdown(rows, today, args.max_age_days))
    else:
        print(
            render_text(
                rows, failures, notes, today, args.max_age_days, manifests, len(dependencies)
            )
        )

    # Written before the verdict is returned, so a failing run still leaves the table. The
    # path is given by the caller and is outside the repository in CI; nothing is written
    # into the tree, because a generated report is not committed here.
    if args.summary is not None:
        try:
            with args.summary.open("a", encoding="utf-8") as handle:
                handle.write(render_markdown(rows, today, args.max_age_days) + "\n")
        except OSError as exc:
            print(f"could not write the table to {args.summary}: {exc}", file=sys.stderr)

    if failures:
        print(
            f"\nrelease-age check failed: {len(failures)} problem(s). Fix the dependency, or "
            f"add an entry to {allowlist_path.name} with a reason, who decided and a date to "
            "look again.",
            file=sys.stderr,
        )
        return EXIT_POLICY
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
