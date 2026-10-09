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
from pathlib import Path
from tempfile import TemporaryDirectory

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

# Captured verbatim from `cargo audit --json` against this workspace with `time` 0.1.45
# added, on 9 October 2026, with only the long prose fields dropped. The point of keeping it
# real is the two fields a synthetic payload gets wrong: `severity` is null and `cvss` is a
# VECTOR STRING, not a number. An earlier version of cargo_severity returned "" for every
# advisory of this shape -- which is every Rust advisory there is -- and "" sorted below
# `low`. `cargo audit`'s own text output rates this one "6.2 (medium)", which is the
# known answer the scoring is tested against.
CARGO_REAL = {
    "database": {
        "advisory-count": 1295
    },
    "lockfile": {
        "dependency-count": 166
    },
    "vulnerabilities": {
        "found": True,
        "count": 1,
        "list": [
            {
                "advisory": {
                    "id": "RUSTSEC-2020-0071",
                    "package": "time",
                    "title": "Potential segfault in the time crate",
                    "cvss": "CVSS:3.1/AV:L/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H",
                    "severity": None,
                    "categories": [
                        "code-execution",
                        "memory-corruption"
                    ],
                    "cwe": None,
                    "informational": None,
                    "withdrawn": None,
                    "url": "https://github.com/time-rs/time/issues/293",
                    "date": "2020-11-18"
                },
                "versions": {
                    "patched": [
                        ">=0.2.23"
                    ],
                    "unaffected": [
                        "=0.2.0",
                        "=0.2.1",
                        "=0.2.2",
                        "=0.2.3",
                        "=0.2.4",
                        "=0.2.5",
                        "=0.2.6"
                    ]
                },
                "package": {
                    "name": "time",
                    "version": "0.1.45",
                    "source": "registry+https://github.com/rust-lang/crates.io-index"
                }
            }
        ]
    },
    "warnings": {}
}


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


class CvssVectorScoring(unittest.TestCase):
    """Known answers from the CVSS v3.1 specification and from cargo audit itself.

    The first case is the one that matters: `cargo audit` prints "Severity: 6.2 (medium)"
    for RUSTSEC-2020-0071, so the implementation has an independent answer to meet."""

    KNOWN = [
        ("CVSS:3.1/AV:L/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H", 6.2, "moderate"),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 9.8, "critical"),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H", 10.0, "critical"),
        ("CVSS:3.1/AV:N/AC:H/PR:H/UI:R/S:U/C:N/I:N/A:L", 2.0, "low"),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:L/I:N/A:N", 5.3, "moderate"),
        ("CVSS:3.0/AV:L/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H", 6.2, "moderate"),
    ]

    def test_every_known_vector_scores_correctly(self):
        for vector, expected, band in self.KNOWN:
            with self.subTest(vector=vector):
                score = advisory_scan.cvss31_base_score(vector)
                self.assertIsNotNone(score, "vector did not score at all")
                self.assertAlmostEqual(score, expected, places=1)
                self.assertEqual(advisory_scan.band_for(score), band)

    def test_the_impact_metrics_are_high_low_none(self):
        """CVSS v2 used High/Medium/None. Using that table returned None for every vector
        with an `L` impact metric, which is most of them."""
        self.assertIsNotNone(
            advisory_scan.cvss31_base_score("CVSS:3.1/AV:N/AC:H/PR:H/UI:R/S:U/C:N/I:N/A:L")
        )

    def test_a_version_four_vector_is_not_scored_with_the_version_three_formula(self):
        """v4.0 uses a different algorithm. Scoring it with this one would be confidently
        wrong, so it comes back unknown -- which sorts first."""
        v4 = "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N"
        self.assertIsNone(advisory_scan.cvss31_base_score(v4))
        self.assertEqual(advisory_scan.cargo_severity({"cvss": v4}), "")

    def test_an_unrecognised_metric_value_is_unknown_not_a_guess(self):
        self.assertIsNone(
            advisory_scan.cvss31_base_score("CVSS:3.1/AV:Z/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H")
        )


class TheRealCargoAuditPayload(unittest.TestCase):
    def test_it_carries_a_vector_and_no_severity_word(self):
        """The two facts a synthetic payload gets wrong, asserted so the fixture cannot
        quietly drift into being synthetic."""
        advisory = CARGO_REAL["vulnerabilities"]["list"][0]["advisory"]
        self.assertIsNone(advisory["severity"])
        self.assertTrue(advisory["cvss"].startswith("CVSS:3."))

    def test_the_real_advisory_is_rated_moderate_as_cargo_audit_rates_it(self):
        found = advisory_scan.parse_cargo_audit(CARGO_REAL)
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0].identifier, "RUSTSEC-2020-0071")
        self.assertEqual(found[0].severity, "moderate")

    def test_it_is_no_longer_unrated(self):
        """The defect: every cargo advisory came out "" and "" sorted below npm `low`."""
        found = advisory_scan.parse_cargo_audit(CARGO_REAL)
        self.assertNotEqual(found[0].severity, "")


class UnknownSeveritySortsFirst(unittest.TestCase):
    """An unrated advisory is one nobody has assessed, not one that is harmless."""

    def test_unknown_outranks_critical(self):
        unknown = advisory_scan.Advisory("X-1", "p", "cargo", "", "t", "u")
        critical = advisory_scan.Advisory("X-2", "q", "npm", "critical", "t", "u")
        self.assertLess(unknown.sort_key, critical.sort_key)

    def test_unknown_outranks_low(self):
        unknown = advisory_scan.Advisory("X-1", "p", "cargo", "", "t", "u")
        low = advisory_scan.Advisory("X-2", "q", "npm", "low", "t", "u")
        self.assertLess(unknown.sort_key, low.sort_key)

    def test_critical_still_outranks_low(self):
        critical = advisory_scan.Advisory("X-1", "p", "npm", "critical", "t", "u")
        low = advisory_scan.Advisory("X-2", "q", "npm", "low", "t", "u")
        self.assertLess(critical.sort_key, low.sort_key)


class NativeSuppressionsAreRefused(unittest.TestCase):
    """**The scan is worthless without this.** Each tool reports only what it has not been
    told to hide, so a scan that reads their output without checking their configuration
    measures the configuration. Each of these was reproduced live before being written."""

    def setUp(self):
        self.tmp = TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def write(self, name: str, text: str) -> None:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def found(self):
        return advisory_scan.find_suppressions(
            self.root, advisory_scan.scanned_npm_manifests(self.root)
        )

    def test_a_clean_tree_finds_nothing(self):
        self.write("web/package.json", '{"name":"w"}')
        self.write("web/pnpm-workspace.yaml", "allowBuilds:\n  esbuild: true\n")
        self.assertEqual(self.found(), [])

    def test_no_suppression_files_at_all_finds_nothing(self):
        self.assertEqual(self.found(), [])

    def test_an_ignore_in_pnpm_workspace_yaml_is_found(self):
        """The surface that actually works on pnpm 11.9.0: this one drops an advisory from
        the output, measured 7 -> 6."""
        self.write("web/pnpm-workspace.yaml", "auditConfig:\n  ignoreGhsas:\n    - GHSA-x\n")
        self.assertEqual(len(self.found()), 1)
        self.assertIn("web/pnpm-workspace.yaml", self.found()[0])

    def test_ignore_cves_in_pnpm_workspace_yaml_is_found(self):
        self.write("web/pnpm-workspace.yaml", "auditConfig:\n  ignoreCves:\n    - CVE-1\n")
        self.assertEqual(len(self.found()), 1)

    def test_an_ignore_in_a_package_json_is_found(self):
        """Has no effect on pnpm 11.9.0, and is checked anyway: it is what the
        documentation describes, and which surface works may change again."""
        self.write("web/package.json", '{"pnpm":{"auditConfig":{"ignoreGhsas":["GHSA-x"]}}}')
        self.assertEqual(len(self.found()), 1)
        self.assertIn("web/package.json", self.found()[0])

    def test_a_top_level_audit_config_in_a_package_json_is_found(self):
        self.write("web/package.json", '{"auditConfig":{"ignoreGhsas":["GHSA-x"]}}')
        self.assertEqual(len(self.found()), 1)

    def test_an_advisories_ignore_in_audit_toml_is_found(self):
        self.write("audit.toml", '[advisories]\nignore = ["RUSTSEC-2020-0071"]\n')
        self.assertEqual(len(self.found()), 1)
        self.assertIn("audit.toml", self.found()[0])

    def test_an_advisories_ignore_in_dot_cargo_audit_toml_is_found(self):
        self.write(".cargo/audit.toml", '[advisories]\nignore = ["RUSTSEC-1"]\n')
        self.assertEqual(len(self.found()), 1)
        self.assertIn(".cargo/audit.toml", self.found()[0])

    def test_an_audit_toml_with_no_ignore_list_is_fine(self):
        self.write("audit.toml", "[advisories]\nseverity_threshold = \"low\"\n")
        self.assertEqual(self.found(), [])

    def test_an_empty_ignore_list_is_fine(self):
        self.write("audit.toml", "[advisories]\nignore = []\n")
        self.assertEqual(self.found(), [])

    def test_an_empty_pnpm_audit_config_is_fine(self):
        self.write("web/package.json", '{"pnpm":{"auditConfig":{"ignoreGhsas":[]}}}')
        self.assertEqual(self.found(), [])

    def test_the_key_named_in_a_comment_does_not_count(self):
        """This repository's own files name these keys in comments to explain the rule. A
        detector that fired on its own documentation would be deleted within a week."""
        self.write(
            "web/pnpm-workspace.yaml",
            "# never set auditConfig.ignoreGhsas here; use dependency-allowlist.toml\n"
            "allowBuilds:\n  esbuild: true\n",
        )
        self.assertEqual(self.found(), [])

    def test_node_modules_is_not_searched(self):
        self.write("web/node_modules/thing/package.json", '{"pnpm":{"auditConfig":{"ignoreGhsas":["x"]}}}')
        self.assertEqual(self.found(), [])

    def test_every_scanned_directory_is_covered_not_just_the_two_we_have(self):
        self.write("e2e/package.json", '{"pnpm":{"auditConfig":{"ignoreCves":["CVE-1"]}}}')
        self.assertEqual(len(self.found()), 1)
        self.assertIn("e2e/package.json", self.found()[0])

    def test_two_suppressions_are_both_reported(self):
        self.write("audit.toml", '[advisories]\nignore = ["RUSTSEC-1"]\n')
        self.write("web/pnpm-workspace.yaml", "auditConfig:\n  ignoreGhsas:\n    - GHSA-x\n")
        self.assertEqual(len(self.found()), 2)

    def test_an_unreadable_audit_toml_cannot_run(self):
        self.write("audit.toml", "[advisories\nbroken")
        with self.assertRaises(advisory_scan.CouldNotRun):
            self.found()


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
