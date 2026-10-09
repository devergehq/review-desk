"""Tests for the advisory scan. Every one runs with no network and no subprocess.

The three parsers are pure functions over a JSON document, so the payloads below are the
real shapes, trimmed: `cargo audit --json`, `pnpm audit --json` and `npm audit --json
--package-lock-only` were each run against this repository and the documents reduced to the
smallest thing that still has the structure. Keeping them real matters — `npm audit`'s
`via` list mixes advisory objects with bare package names, and a hand-invented payload
would not have shown that.

    python3 -m unittest discover --start-directory tools
"""

from __future__ import annotations

import datetime
import json
import unittest

import advisory_scan
import dependency_allowlist

TODAY = datetime.date(2026, 10, 9)

CARGO_CLEAN = {
    "database": {"advisory-count": 1295},
    "lockfile": {"dependency-count": 161},
    "vulnerabilities": {"found": False, "count": 0, "list": []},
    "warnings": {},
}

CARGO_ONE = {
    "vulnerabilities": {
        "found": True,
        "count": 1,
        "list": [
            {
                "advisory": {
                    "id": "RUSTSEC-2020-0071",
                    "package": "time",
                    "title": "Potential segfault in the time crate",
                    "url": "https://rustsec.org/advisories/RUSTSEC-2020-0071",
                },
                "package": {"name": "time", "version": "0.1.45"},
            }
        ],
    },
    "warnings": {},
}

PNPM_ONE = {
    "advisories": {
        "1240912": {
            "id": 1240912,
            "github_advisory_id": "GHSA-86w9-cpqp-85rv",
            "module_name": "node-forge",
            "severity": "high",
            "title": "node-forge RSA PKCS#1 v1.5 signature verification accepts extra elements",
            "url": "https://github.com/advisories/GHSA-86w9-cpqp-85rv",
            "vulnerable_versions": "<=1.4.0",
            "findings": [
                {
                    "version": "1.4.0",
                    "paths": [".>nuxt>@nuxt/nitro-server>nitropack>listhen>node-forge"],
                }
            ],
        }
    },
    "metadata": {"vulnerabilities": {"high": 1}},
}

PNPM_CLEAN = {"metadata": {"vulnerabilities": {}}}

NPM_ONE = {
    "auditReportVersion": 2,
    "vulnerabilities": {
        "braces": {
            "name": "braces",
            "severity": "high",
            "range": "<=3.0.3",
            "via": [
                {
                    "source": 1098094,
                    "name": "braces",
                    "title": "braces vulnerable to stack exhaustion",
                    "url": "https://github.com/advisories/GHSA-vfj7-8cjw-p6xm",
                    "severity": "high",
                    "range": "<=3.0.3",
                }
            ],
        },
        "chokidar": {
            "name": "chokidar",
            "severity": "high",
            "range": "2.0.0 - 3.6.0",
            # A bare string names the package that brought the advisory in. It is an edge
            # in the graph, not an advisory, and counting it would double-report.
            "via": ["braces"],
        },
    },
    "metadata": {"vulnerabilities": {"high": 2}},
}

NPM_CLEAN = {"auditReportVersion": 2, "vulnerabilities": {}, "metadata": {"vulnerabilities": {}}}

ENTRY = """
[[advisory]]
advisory = "{id}"
dependency = "node-forge"
ecosystem = "npm"
reason = "transitive under nuxt, dev-time only, and not in the shipped static build"
decided_by = "Patrick"
look_again = {date}
"""


def allowlist(text: str = "") -> dependency_allowlist.Allowlist:
    return dependency_allowlist.parse(text, today=TODAY)


def scan(name="pnpm audit (web)", ecosystem="npm", advisories=()):
    s = advisory_scan.Scan(name, ecosystem, "web", ["pnpm", "audit", "--json"])
    s.advisories = list(advisories)
    return s


class ParsingCargoAudit(unittest.TestCase):
    def test_a_clean_run_finds_nothing(self):
        self.assertEqual(advisory_scan.parse_cargo_audit(CARGO_CLEAN), [])

    def test_one_advisory_is_read_with_its_rustsec_id(self):
        found = advisory_scan.parse_cargo_audit(CARGO_ONE)
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0].identifier, "RUSTSEC-2020-0071")
        self.assertEqual(found[0].package, "time")
        self.assertEqual(found[0].ecosystem, "cargo")

    def test_a_document_with_no_vulnerabilities_object_cannot_run(self):
        """An empty or unexpected document must not read as a clean bill of health."""
        with self.assertRaises(advisory_scan.CouldNotRun):
            advisory_scan.parse_cargo_audit({})

    def test_warnings_are_not_advisories(self):
        """`cargo audit` reports unmaintained and yanked crates as warnings. That is the
        release-age check's question, and counting them here would report the same fact
        twice under two names."""
        document = dict(CARGO_CLEAN)
        document["warnings"] = {"unmaintained": [{"advisory": {"id": "RUSTSEC-2021-0139"}}]}
        self.assertEqual(advisory_scan.parse_cargo_audit(document), [])


class RustSecSeverityBands(unittest.TestCase):
    """`cargo audit` gives a score or a vector, not a word. Reading "has a cvss field" as
    `high` printed a 9.8 critical as high and sorted it below an npm `critical`."""

    def test_a_high_score_is_critical(self):
        self.assertEqual(advisory_scan.cargo_severity({"cvss": 9.8}), "critical")

    def test_a_mid_score_is_moderate(self):
        self.assertEqual(advisory_scan.cargo_severity({"cvss": 6.2}), "moderate")

    def test_a_score_as_a_string_is_read(self):
        self.assertEqual(advisory_scan.cargo_severity({"cvss": "7.5"}), "high")

    def test_a_vector_string_carries_no_score(self):
        self.assertEqual(advisory_scan.cargo_severity({"cvss": "CVSS:3.1/AV:N/AC:L"}), "")

    def test_an_explicit_word_wins(self):
        self.assertEqual(advisory_scan.cargo_severity({"severity": "Critical", "cvss": 1.0}), "critical")

    def test_nothing_at_all_is_unrated(self):
        self.assertEqual(advisory_scan.cargo_severity({}), "")

    def test_a_scored_cargo_advisory_sorts_above_an_npm_low(self):
        document = json.loads(json.dumps(CARGO_ONE))
        document["vulnerabilities"]["list"][0]["advisory"]["cvss"] = 9.8
        found = advisory_scan.parse_cargo_audit(document)
        self.assertEqual(found[0].severity, "critical")
        low = advisory_scan.Advisory("GHSA-low", "katex", "npm", "low", "t", "u")
        self.assertLess(found[0].sort_key, low.sort_key)


class ParsingPnpmAudit(unittest.TestCase):
    def test_a_clean_run_finds_nothing(self):
        self.assertEqual(advisory_scan.parse_pnpm_audit(PNPM_CLEAN), [])

    def test_the_github_id_is_preferred_over_the_npm_id(self):
        """The numeric npm id is not what the advisory is called anywhere a person would
        look it up, so it is not what the allowlist keys on."""
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        self.assertEqual(found[0].identifier, "GHSA-86w9-cpqp-85rv")

    def test_the_dependency_path_is_kept(self):
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        self.assertIn("nuxt", found[0].path)

    def test_a_document_of_the_wrong_shape_cannot_run(self):
        with self.assertRaises(advisory_scan.CouldNotRun):
            advisory_scan.parse_pnpm_audit({"something": "else"})


class ParsingNpmAudit(unittest.TestCase):
    def test_a_clean_run_finds_nothing(self):
        self.assertEqual(advisory_scan.parse_npm_audit(NPM_CLEAN), [])

    def test_a_string_in_via_is_an_edge_not_an_advisory(self):
        """npm reports braces twice: once as the advisory and once as the reason chokidar
        is vulnerable. Counting both would report one problem as two."""
        found = advisory_scan.parse_npm_audit(NPM_ONE)
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0].identifier, "GHSA-vfj7-8cjw-p6xm")

    def test_the_identifier_comes_out_of_the_advisory_url(self):
        found = advisory_scan.parse_npm_audit(NPM_ONE)
        self.assertTrue(found[0].identifier.startswith("GHSA-"))

    def test_a_document_with_no_vulnerabilities_object_cannot_run(self):
        with self.assertRaises(advisory_scan.CouldNotRun):
            advisory_scan.parse_npm_audit({"auditReportVersion": 2})


class TheAdvisoryVerdict(unittest.TestCase):
    def judge(self, scans, text=""):
        return advisory_scan.evaluate(scans, allowlist(text), TODAY)

    def test_no_advisories_passes(self):
        failures, excused = self.judge([scan()])
        self.assertEqual(failures, [])
        self.assertEqual(excused, [])

    def test_an_advisory_with_no_entry_fails(self):
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        failures, _ = self.judge([scan(advisories=found)])
        self.assertEqual(len(failures), 1)
        self.assertIn("GHSA-86w9-cpqp-85rv", failures[0])
        self.assertIn("node-forge", failures[0])

    def test_the_failure_states_the_severity(self):
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        failures, _ = self.judge([scan(advisories=found)])
        self.assertIn("high", failures[0])

    def test_an_entry_with_a_reason_makes_it_pass(self):
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        failures, excused = self.judge(
            [scan(advisories=found)], ENTRY.format(id="GHSA-86w9-cpqp-85rv", date="2026-12-11")
        )
        self.assertEqual(failures, [])
        self.assertEqual(len(excused), 1)
        self.assertIn("Patrick", excused[0])

    def test_the_same_entry_with_a_past_look_again_date_fails(self):
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        failures, _ = self.judge(
            [scan(advisories=found)], ENTRY.format(id="GHSA-86w9-cpqp-85rv", date="2026-09-01")
        )
        self.assertEqual(len(failures), 1)
        self.assertIn("EXPIRED", failures[0])

    def test_an_entry_for_the_wrong_ecosystem_does_not_excuse_it(self):
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        text = ENTRY.format(id="GHSA-86w9-cpqp-85rv", date="2026-12-11").replace(
            'ecosystem = "npm"', 'ecosystem = "cargo"'
        )
        failures, _ = self.judge([scan(advisories=found)], text)
        self.assertTrue(any("ADVISORY" in f for f in failures))

    def test_an_advisory_id_in_the_wrong_case_still_matches(self):
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        text = ENTRY.format(id="ghsa-86w9-cpqp-85rv", date="2026-12-11")
        failures, excused = self.judge([scan(advisories=found)], text)
        self.assertEqual(failures, [])
        self.assertEqual(len(excused), 1)

    def test_an_orphaned_entry_fails(self):
        failures, _ = self.judge([scan()], ENTRY.format(id="GHSA-dead-beef", date="2026-12-11"))
        self.assertTrue(any("ORPHANED" in f for f in failures))

    def test_an_entry_for_an_ecosystem_this_run_did_not_scan_is_not_orphaned(self):
        """With `--only cargo` the npm scans do not run. Calling every npm entry orphaned
        would be a partial run inventing failures about things nothing looked at."""
        cargo_only = scan(name="cargo audit", ecosystem="cargo")
        failures, _ = self.judge([cargo_only], ENTRY.format(id="GHSA-86w9-cpqp-85rv", date="2026-12-11"))
        self.assertEqual(failures, [])

    def test_a_malformed_entry_fails_the_scan(self):
        text = ENTRY.format(id="GHSA-86w9-cpqp-85rv", date="2026-12-11").replace("reason", "why")
        found = advisory_scan.parse_pnpm_audit(PNPM_ONE)
        failures, _ = self.judge([scan(advisories=found)], text)
        self.assertTrue(failures)

    def test_critical_is_reported_before_low(self):
        advisories = [
            advisory_scan.Advisory("GHSA-low", "katex", "npm", "low", "t", "u"),
            advisory_scan.Advisory("GHSA-crit", "simple-git", "npm", "critical", "t", "u"),
        ]
        failures, _ = self.judge([scan(advisories=advisories)])
        self.assertIn("GHSA-crit", failures[0])
        self.assertIn("GHSA-low", failures[1])

    def test_the_report_says_how_many_scans_ran(self):
        failures, excused = self.judge([scan(), scan(name="cargo audit", ecosystem="cargo")])
        text = advisory_scan.render(
            [scan(), scan(name="cargo audit", ecosystem="cargo")], failures, excused
        )
        self.assertIn("2 scan(s) ran", text)


class WhenAToolCannotRun(unittest.TestCase):
    """Patrick's rule: a check that passes without running carries no information."""

    def test_a_tool_that_prints_nothing_cannot_run(self):
        planned = advisory_scan.Scan("pnpm audit (web)", "npm", ".", ["true"])
        with self.assertRaises(advisory_scan.CouldNotRun) as caught:
            advisory_scan.scan_with(planned, _repo_root(), advisory_scan.parse_pnpm_audit)
        self.assertIn("printed no JSON", str(caught.exception))

    def test_a_tool_that_prints_something_that_is_not_json_cannot_run(self):
        planned = advisory_scan.Scan("pnpm audit (web)", "npm", ".", ["echo", "ENOTFOUND"])
        with self.assertRaises(advisory_scan.CouldNotRun) as caught:
            advisory_scan.scan_with(planned, _repo_root(), advisory_scan.parse_pnpm_audit)
        self.assertIn("not", str(caught.exception))

    def test_a_tool_that_is_not_installed_cannot_run(self):
        planned = advisory_scan.Scan("nope", "npm", ".", ["definitely-not-a-real-tool-xyz"])
        with self.assertRaises(advisory_scan.CouldNotRun):
            advisory_scan.scan_with(planned, _repo_root(), advisory_scan.parse_pnpm_audit)

    def test_a_missing_directory_cannot_run(self):
        planned = advisory_scan.Scan("x", "npm", "no-such-directory", ["echo", "{}"])
        with self.assertRaises(advisory_scan.CouldNotRun):
            advisory_scan.scan_with(planned, _repo_root(), advisory_scan.parse_pnpm_audit)

    def test_a_non_zero_exit_with_good_json_is_not_a_failure_to_run(self):
        """Each of these tools exits non-zero precisely when it found something, so the
        exit code says nothing about whether the tool worked."""
        planned = advisory_scan.Scan(
            "x", "npm", ".", ["sh", "-c", 'echo \'{"advisories":{}}\'; exit 1']
        )
        done = advisory_scan.scan_with(planned, _repo_root(), advisory_scan.parse_pnpm_audit)
        self.assertEqual(done.advisories, [])


def _repo_root():
    from pathlib import Path

    return Path(__file__).resolve().parent.parent


if __name__ == "__main__":
    unittest.main()
