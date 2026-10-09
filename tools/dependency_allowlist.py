"""The allowlist both dependency checks read.

One file, `dependency-allowlist.toml`, holds two kinds of entry: a `[[release_age]]`
entry says a dependency may be older than the age limit, and an `[[advisory]]` entry says
a known advisory may stay unfixed. Both kinds carry the same four justifications — the
reason, who decided, and a date to look again — because an allowlist whose entries have no
expiry is a list that only ever grows.

**A malformed entry fails the check rather than being skipped.** An entry with a typo in a
key name would otherwise disable a failure silently, which is the one thing an allowlist
must never do: `look-again` instead of `look_again` would read as an entry with no expiry
and no reason, and the check would still pass. So unknown keys, missing keys, an empty
reason and an unparseable date are all failures, and they are reported as failures of the
allowlist rather than of the dependency.
"""

from __future__ import annotations

import datetime
import sys
from dataclasses import dataclass
from pathlib import Path

if sys.version_info < (3, 11):  # pragma: no cover - the guard exists so CI says why
    sys.exit(
        "the dependency checks need Python 3.11 or newer for tomllib; "
        f"this is {sys.version.split()[0]}"
    )

import tomllib  # noqa: E402  (below the guard, deliberately)

ECOSYSTEMS = ("cargo", "npm")

# How far ahead a look-again date may be set. Without a ceiling, `look_again = 2099-01-01`
# is a permanent exemption wearing the shape of a review date, and the field stops meaning
# anything. Eighteen months is long enough for "this is finished, leave it" and short
# enough that somebody still working here will see it again.
MAXIMUM_HORIZON_DAYS = 550

# A reason has to say something. Twenty characters is not a quality bar — it is the
# shortest thing that cannot be "ok", "later" or "n/a", which is what an allowlist with no
# minimum fills up with.
MINIMUM_REASON_LENGTH = 20

RELEASE_AGE_KEYS = frozenset({"dependency", "ecosystem", "reason", "decided_by", "look_again"})
ADVISORY_KEYS = frozenset({"advisory", "dependency", "ecosystem", "reason", "decided_by", "look_again"})


class AllowlistError(Exception):
    """The allowlist file itself could not be read. Distinct from an invalid entry: a
    missing or unparseable file means the check could not run, while an invalid entry
    means the check ran and the answer is no."""


@dataclass(frozen=True)
class ReleaseAgeEntry:
    dependency: str
    ecosystem: str
    reason: str
    decided_by: str
    look_again: datetime.date

    def expired(self, today: datetime.date) -> bool:
        return self.look_again < today


@dataclass(frozen=True)
class AdvisoryEntry:
    advisory: str
    dependency: str
    ecosystem: str
    reason: str
    decided_by: str
    look_again: datetime.date

    def expired(self, today: datetime.date) -> bool:
        return self.look_again < today


@dataclass(frozen=True)
class Allowlist:
    release_age: tuple[ReleaseAgeEntry, ...]
    advisories: tuple[AdvisoryEntry, ...]
    problems: tuple[str, ...]

    def release_age_for(self, dependency: str, ecosystem: str) -> ReleaseAgeEntry | None:
        wanted = normalise(dependency, ecosystem)
        for entry in self.release_age:
            if entry.ecosystem == ecosystem and normalise(entry.dependency, entry.ecosystem) == wanted:
                return entry
        return None

    def advisory_for(self, advisory: str, ecosystem: str) -> AdvisoryEntry | None:
        for entry in self.advisories:
            if entry.advisory.upper() == advisory.upper() and entry.ecosystem == ecosystem:
                return entry
        return None


EMPTY = Allowlist(release_age=(), advisories=(), problems=())


def _check_keys(table: dict, allowed: frozenset[str], label: str) -> list[str]:
    problems = []
    for key in sorted(set(table) - allowed):
        problems.append(f"{label}: unknown key {key!r} — the allowed keys are {', '.join(sorted(allowed))}")
    for key in sorted(allowed - set(table)):
        problems.append(f"{label}: missing {key!r}")
    return problems


def normalise(name: str, ecosystem: str) -> str:
    """The name to compare on. crates.io treats case and `-`/`_` as the same, so an entry
    spelled `serde-norway` must still match the manifest key `serde_norway` — otherwise the
    entry looks valid, fetches cleanly, and silently excuses nothing."""
    if ecosystem == "cargo":
        return name.lower().replace("-", "_")
    return name.lower()


def _check_shared(table: dict, label: str, today: datetime.date | None) -> list[str]:
    problems = []

    ecosystem = table.get("ecosystem")
    if ecosystem is not None and ecosystem not in ECOSYSTEMS:
        problems.append(f"{label}: ecosystem {ecosystem!r} is not one of {', '.join(ECOSYSTEMS)}")

    reason = table.get("reason")
    if isinstance(reason, str) and len(reason.strip()) < MINIMUM_REASON_LENGTH:
        problems.append(
            f"{label}: reason is {len(reason.strip())} characters; "
            f"an entry needs at least {MINIMUM_REASON_LENGTH} saying why this is acceptable"
        )

    decided_by = table.get("decided_by")
    if isinstance(decided_by, str) and not decided_by.strip():
        problems.append(f"{label}: decided_by is empty — an allowlist entry is somebody's decision")

    # A missing look_again is reported by _check_keys; everything here is about a value
    # that is present. The ordering matters: datetime is a subclass of date, so the
    # date-and-time case has to be tested before the horizon arithmetic, and the horizon
    # arithmetic must not run on a None that _check_keys has already complained about.
    look_again = table.get("look_again")
    if look_again is None:
        pass
    elif isinstance(look_again, datetime.datetime):
        problems.append(
            f"{label}: look_again is a date and time; it has to be a bare date, like 2027-01-31"
        )
    elif not isinstance(look_again, datetime.date):
        problems.append(
            f"{label}: look_again is {look_again!r}; it has to be a bare TOML date, like 2027-01-31"
        )
    elif today is not None and (look_again - today).days > MAXIMUM_HORIZON_DAYS:
        problems.append(
            f"{label}: look_again is {look_again}, {(look_again - today).days} days away. "
            f"The furthest an entry may be set is {MAXIMUM_HORIZON_DAYS} days, because a date "
            "nobody will reach is an exemption, not a review."
        )

    for key in ("dependency", "advisory", "ecosystem", "reason", "decided_by"):
        value = table.get(key)
        if value is not None and not isinstance(value, str):
            problems.append(f"{label}: {key} is {type(value).__name__}, not a string")

    return problems


def parse(text: str, today: datetime.date | None = None) -> Allowlist:
    """Read an allowlist from TOML text. Raises AllowlistError if the text is not TOML;
    returns problems rather than raising for an entry that is TOML but not a valid entry."""
    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError as exc:
        raise AllowlistError(f"the allowlist is not valid TOML: {exc}") from exc

    problems: list[str] = []
    for key in sorted(set(data) - {"release_age", "advisory"}):
        problems.append(f"the allowlist has an unknown top-level table {key!r}")

    release_age: list[ReleaseAgeEntry] = []
    for index, table in enumerate(_entries(data, "release_age", problems), start=1):
        label = f"release_age entry {index} ({table.get('dependency', 'unnamed')})"
        found = _check_keys(table, RELEASE_AGE_KEYS, label) + _check_shared(table, label, today)
        if found:
            problems.extend(found)
            continue
        release_age.append(
            ReleaseAgeEntry(
                dependency=table["dependency"],
                ecosystem=table["ecosystem"],
                reason=table["reason"].strip(),
                decided_by=table["decided_by"].strip(),
                look_again=table["look_again"],
            )
        )

    advisories: list[AdvisoryEntry] = []
    for index, table in enumerate(_entries(data, "advisory", problems), start=1):
        label = f"advisory entry {index} ({table.get('advisory', 'unnamed')})"
        found = _check_keys(table, ADVISORY_KEYS, label) + _check_shared(table, label, today)
        if found:
            problems.extend(found)
            continue
        advisories.append(
            AdvisoryEntry(
                advisory=table["advisory"],
                dependency=table["dependency"],
                ecosystem=table["ecosystem"],
                reason=table["reason"].strip(),
                decided_by=table["decided_by"].strip(),
                look_again=table["look_again"],
            )
        )

    duplicate_problems = _duplicates(
        [(normalise(e.dependency, e.ecosystem), e.ecosystem) for e in release_age], "release_age"
    ) + _duplicates([(e.advisory.upper(), e.ecosystem) for e in advisories], "advisory")

    return Allowlist(
        release_age=tuple(release_age),
        advisories=tuple(advisories),
        problems=tuple(problems + duplicate_problems),
    )


def _entries(data: dict, key: str, problems: list[str]) -> list[dict]:
    """The entries under `[[key]]`, or none plus a problem.

    Writing `[release_age]` instead of `[[release_age]]` is the commonest slip in TOML and
    exactly the typo class this module exists to catch. Iterating it yields the table's
    *keys* as strings, and `table.get(...)` then raised AttributeError -- a traceback and
    exit 1, which reads as "the check ran and the answer is no" rather than "the allowlist
    could not be read"."""
    value = data.get(key, [])
    if isinstance(value, dict):
        problems.append(
            f"the allowlist has [{key}] where it needs [[{key}]] — each entry is one of a "
            "list, so the heading takes double brackets"
        )
        return []
    if not isinstance(value, list):
        problems.append(f"the allowlist's {key} is {type(value).__name__}, not a list of entries")
        return []
    out = []
    for index, item in enumerate(value, start=1):
        if isinstance(item, dict):
            out.append(item)
        else:
            problems.append(f"{key} entry {index} is {type(item).__name__}, not a table")
    return out


def _duplicates(keys: list[tuple[str, str]], kind: str) -> list[str]:
    seen: set[tuple[str, str]] = set()
    problems = []
    for key in keys:
        if key in seen:
            problems.append(f"{kind}: {key[0]} ({key[1]}) is allowlisted twice")
        seen.add(key)
    return problems


def read(path: Path, today: datetime.date | None = None) -> Allowlist:
    """Read the allowlist from a file. An absent file is an empty allowlist — a repository
    with nothing to excuse should not need the file — but an unreadable one is an error."""
    if not path.exists():
        return EMPTY
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise AllowlistError(f"could not read {path}: {exc}") from exc
    return parse(text, today)
