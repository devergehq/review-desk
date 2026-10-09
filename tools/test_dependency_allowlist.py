"""Tests for the allowlist both checks read. No network, no filesystem except a temp file.

The point of most of these is the same: **a malformed entry must fail, never be skipped.**
An allowlist that silently ignores an entry it cannot understand is worse than no
allowlist, because the entry looks like it is doing something.

    python3 -m unittest discover --start-directory tools
"""

from __future__ import annotations

import datetime
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

import dependency_allowlist as allowlist

TODAY = datetime.date(2026, 10, 9)

GOOD = """
[[release_age]]
dependency = "serde_norway"
ecosystem = "cargo"
reason = "test-only, and the best-maintained serde-integrated YAML crate that exists"
decided_by = "Patrick"
look_again = 2027-04-09
"""


def parse(text: str):
    return allowlist.parse(text, today=TODAY)


class AValidEntry(unittest.TestCase):
    def test_is_read(self):
        result = parse(GOOD)
        self.assertEqual(result.problems, ())
        self.assertEqual(len(result.release_age), 1)
        self.assertEqual(result.release_age[0].dependency, "serde_norway")
        self.assertEqual(result.release_age[0].decided_by, "Patrick")
        self.assertEqual(result.release_age[0].look_again, datetime.date(2027, 4, 9))

    def test_is_found_by_name_and_ecosystem(self):
        result = parse(GOOD)
        self.assertIsNotNone(result.release_age_for("serde_norway", "cargo"))
        self.assertIsNone(result.release_age_for("serde_norway", "npm"))

    def test_is_found_however_the_name_is_hyphenated(self):
        """crates.io treats `-` and `_` as the same character and answers a request for
        `serde-norway` with `"name": "serde_norway"`. An entry spelled either way has to
        match, or it looks valid, fetches cleanly, and excuses nothing."""
        result = parse(GOOD.replace("serde_norway", "serde-norway"))
        self.assertEqual(result.problems, ())
        self.assertIsNotNone(result.release_age_for("serde_norway", "cargo"))

    def test_is_found_however_it_is_capitalised(self):
        result = parse(GOOD.replace("serde_norway", "Serde_Norway"))
        self.assertIsNotNone(result.release_age_for("serde_norway", "cargo"))

    def test_an_npm_name_keeps_its_hyphens(self):
        """npm is not crates.io: `ajv-formats` and `ajv_formats` are different packages."""
        text = GOOD.replace("serde_norway", "ajv-formats").replace('"cargo"', '"npm"')
        result = parse(text)
        self.assertIsNotNone(result.release_age_for("ajv-formats", "npm"))
        self.assertIsNone(result.release_age_for("ajv_formats", "npm"))


class AMalformedEntry(unittest.TestCase):
    def assert_rejected(self, text: str, expect: str = ""):
        result = parse(text)
        self.assertTrue(result.problems, f"expected a problem, got none for:\n{text}")
        self.assertEqual(result.release_age, (), "a rejected entry must not be in effect")
        if expect:
            self.assertTrue(
                any(expect in problem for problem in result.problems),
                f"expected {expect!r} in {result.problems}",
            )

    def test_a_misspelled_key_is_rejected(self):
        """`look-again` for `look_again` would otherwise read as an entry with no expiry,
        and the check would still pass. This is the failure mode the validation exists for."""
        self.assert_rejected(GOOD.replace("look_again", "look-again"), "unknown key")

    def test_a_missing_reason_is_rejected(self):
        self.assert_rejected(
            "\n".join(line for line in GOOD.splitlines() if not line.startswith("reason")),
            "missing 'reason'",
        )

    def test_a_missing_decided_by_is_rejected(self):
        self.assert_rejected(
            "\n".join(line for line in GOOD.splitlines() if not line.startswith("decided_by")),
            "missing 'decided_by'",
        )

    def test_a_missing_look_again_is_rejected_and_does_not_crash(self):
        self.assert_rejected(
            "\n".join(line for line in GOOD.splitlines() if not line.startswith("look_again")),
            "missing 'look_again'",
        )

    def test_a_reason_that_says_nothing_is_rejected(self):
        self.assert_rejected(GOOD.replace(
            '"test-only, and the best-maintained serde-integrated YAML crate that exists"', '"later"'
        ), "characters")

    def test_an_empty_decided_by_is_rejected(self):
        self.assert_rejected(GOOD.replace('"Patrick"', '"   "'), "decided_by is empty")

    def test_a_look_again_that_is_a_string_is_rejected(self):
        self.assert_rejected(GOOD.replace("look_again = 2027-04-09", 'look_again = "2027-04-09"'))

    def test_a_look_again_with_a_time_is_rejected(self):
        self.assert_rejected(
            GOOD.replace("look_again = 2027-04-09", "look_again = 2027-04-09T10:00:00"),
            "date and time",
        )

    def test_a_look_again_nobody_will_reach_is_rejected(self):
        """Without a ceiling, `2099-01-01` is a permanent exemption wearing the shape of a
        review date, and the field stops meaning anything."""
        self.assert_rejected(GOOD.replace("2027-04-09", "2099-01-01"), "furthest an entry may be set")

    def test_an_unknown_ecosystem_is_rejected(self):
        self.assert_rejected(GOOD.replace('"cargo"', '"maven"'), "not one of")

    def test_an_unknown_top_level_table_is_reported(self):
        result = parse(GOOD + '\n[[something_else]]\nx = 1\n')
        self.assertTrue(any("unknown top-level table" in p for p in result.problems))

    def test_the_same_dependency_twice_is_rejected(self):
        result = parse(GOOD + GOOD)
        self.assertTrue(any("allowlisted twice" in p for p in result.problems))

    def test_the_same_dependency_twice_under_different_spellings_is_rejected(self):
        result = parse(GOOD + GOOD.replace("serde_norway", "serde-norway"))
        self.assertTrue(any("allowlisted twice" in p for p in result.problems))

    def test_a_single_bracket_heading_is_rejected_rather_than_crashing(self):
        """`[release_age]` for `[[release_age]]` is the commonest slip in TOML. Iterating it
        yields the table's keys as strings, which used to raise AttributeError: a traceback
        and exit 1, reading as "the check ran and the answer is no"."""
        self.assert_rejected(GOOD.replace("[[release_age]]", "[release_age]"), "double brackets")

    def test_an_entry_that_is_not_a_table_is_rejected(self):
        self.assert_rejected('release_age = ["serde_norway"]\n', "not a table")

    def test_text_that_is_not_toml_cannot_be_read_at_all(self):
        """Distinct from an invalid entry: a file that is not TOML means the check could
        not run, not that the answer is no."""
        with self.assertRaises(allowlist.AllowlistError):
            parse("[[release_age]\nbroken")


class Expiry(unittest.TestCase):
    def entry(self, date: str):
        return parse(GOOD.replace("2027-04-09", date)).release_age[0]

    def test_a_future_date_has_not_expired(self):
        self.assertFalse(self.entry("2027-04-09").expired(TODAY))

    def test_today_has_not_expired(self):
        self.assertFalse(self.entry("2026-10-09").expired(TODAY))

    def test_yesterday_has_expired(self):
        self.assertTrue(self.entry("2026-10-08").expired(TODAY))


class ReadingTheFile(unittest.TestCase):
    def test_an_absent_file_is_an_empty_allowlist(self):
        """A repository with nothing to excuse should not need the file at all."""
        with TemporaryDirectory() as tmp:
            result = allowlist.read(Path(tmp) / "nothing.toml", today=TODAY)
        self.assertEqual(result, allowlist.EMPTY)

    def test_the_repositorys_own_allowlist_is_valid(self):
        """The one test here that reads the real file. If an entry in
        `dependency-allowlist.toml` is malformed, both checks fail — so it is worth knowing
        from a 13-millisecond test rather than from a CI run."""
        path = Path(__file__).resolve().parent.parent / "dependency-allowlist.toml"
        result = allowlist.read(path, today=TODAY)
        self.assertEqual(result.problems, (), "the repository's own allowlist has problems")
        self.assertTrue(result.release_age or result.advisories)

    def test_every_entry_in_the_repositorys_allowlist_was_decided_by_a_person(self):
        path = Path(__file__).resolve().parent.parent / "dependency-allowlist.toml"
        result = allowlist.read(path, today=TODAY)
        for entry in list(result.release_age) + list(result.advisories):
            self.assertTrue(entry.decided_by.strip(), entry)
            self.assertNotIn("claude", entry.decided_by.lower())
            self.assertNotIn("agent", entry.decided_by.lower())


if __name__ == "__main__":
    unittest.main()
