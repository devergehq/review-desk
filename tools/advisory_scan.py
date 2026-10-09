#!/usr/bin/env python3
"""Fail on a known vulnerability in any dependency, Rust or npm, direct or indirect.

`cargo audit`, `pnpm audit` and `npm audit` each already know the answer. What none of them
has is a way to say *why* a particular advisory is being lived with, who decided that, and
when to look at it again. Their suppression mechanisms are three files in three syntaxes
with no field for a reason: `audit.toml`'s `ignore` list, `pnpm.auditConfig.ignoreGhsas` in
a `package.json`, and for `npm audit` nothing at all.

**That gap is what gets an advisory scan switched off.** On the day this landed, `web/` had
seven advisories — two critical — every one of them inside the `nuxt` dependency tree and
none fixable by changing a direct dependency. A job that is red on arrival gets quietened,
and the cheapest way to quieten it is a one-line edit to a package manifest nobody diffs.
Eight months later nobody can tell which suppression is still the Nuxt tree and which is a
new critical that was folded into the same list. So this runs the three native tools, reads
their JSON, and judges it against `dependency-allowlist.toml` — the same five fields, the
same expiry, the same file as the release-age check.

    python3 tools/advisory_scan.py                  # everything available
    python3 tools/advisory_scan.py --only cargo     # one ecosystem

Exit codes: 0 no unexcused advisory, 1 the scan ran and found one, 2 the scan could not run.
"""

from __future__ import annotations

import argparse
import datetime
import json
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

if sys.version_info < (3, 11):  # pragma: no cover - the guard exists so CI says why
    sys.exit(f"advisory_scan.py needs Python 3.11 or newer; this is {sys.version.split()[0]}")

sys.path.insert(0, str(Path(__file__).resolve().parent))

import dependency_allowlist  # noqa: E402  (after the sys.path line, deliberately)

EXIT_OK = 0
EXIT_POLICY = 1
EXIT_COULD_NOT_RUN = 2

SEVERITY_ORDER = {"critical": 0, "high": 1, "moderate": 2, "medium": 2, "low": 3, "info": 4, "": 5}


class CouldNotRun(Exception):
    """The scan could not reach a conclusion. Always exit 2, never a pass."""


@dataclass(frozen=True)
class Advisory:
    identifier: str
    package: str
    ecosystem: str
    severity: str
    title: str
    url: str
    path: str = ""

    @property
    def sort_key(self) -> tuple:
        return (SEVERITY_ORDER.get(self.severity.lower(), 9), self.ecosystem, self.package, self.identifier)


@dataclass
class Scan:
    name: str
    ecosystem: str
    directory: str
    command: list[str]
    advisories: list[Advisory] = field(default_factory=list)


# --------------------------------------------------------------------------- the parsers
#
# Three tools, three shapes. Each parser is pure: it takes the JSON document and returns
# advisories, so the tests exercise real captured payloads with no subprocess and no network.


def parse_cargo_audit(document: dict) -> list[Advisory]:
    """`cargo audit --json`. `vulnerabilities.list[]` carries the advisory and the package
    it was found in. `warnings` holds the unmaintained and yanked notices, which are a
    different question and belong to the release-age check."""
    found: list[Advisory] = []
    vulnerabilities = document.get("vulnerabilities")
    if not isinstance(vulnerabilities, dict):
        raise CouldNotRun("cargo audit's JSON has no `vulnerabilities` object")
    for item in vulnerabilities.get("list") or []:
        if not isinstance(item, dict):
            continue
        advisory = item.get("advisory") if isinstance(item.get("advisory"), dict) else {}
        package = item.get("package") if isinstance(item.get("package"), dict) else {}
        found.append(
            Advisory(
                identifier=str(advisory.get("id", "unknown")),
                package=str(package.get("name", advisory.get("package", "unknown"))),
                ecosystem="cargo",
                severity=str((advisory.get("cvss") and "high") or advisory.get("severity") or ""),
                title=str(advisory.get("title", "")).strip(),
                url=str(advisory.get("url") or f"https://rustsec.org/advisories/{advisory.get('id', '')}"),
                path=str(package.get("version", "")),
            )
        )
    return found


def parse_pnpm_audit(document: dict) -> list[Advisory]:
    """`pnpm audit --json`. An `advisories` map keyed by npm advisory id; the GitHub id is
    the stable one to allowlist against, because the numeric npm id is not what the
    advisory is called anywhere a person would look it up."""
    found: list[Advisory] = []
    advisories = document.get("advisories")
    if advisories is None and isinstance(document.get("metadata"), dict):
        # A clean run can omit the map entirely rather than sending an empty one.
        advisories = {}
    if not isinstance(advisories, dict):
        raise CouldNotRun("pnpm audit's JSON has no `advisories` object")
    for key, item in sorted(advisories.items()):
        if not isinstance(item, dict):
            continue
        findings = item.get("findings") or []
        path = ""
        if findings and isinstance(findings[0], dict):
            paths = findings[0].get("paths") or []
            path = str(paths[0]) if paths else ""
        found.append(
            Advisory(
                identifier=str(item.get("github_advisory_id") or key),
                package=str(item.get("module_name", "unknown")),
                ecosystem="npm",
                severity=str(item.get("severity", "")),
                title=str(item.get("title", "")).strip(),
                url=str(item.get("url", "")),
                path=path,
            )
        )
    return found


def parse_npm_audit(document: dict) -> list[Advisory]:
    """`npm audit --json`. A `vulnerabilities` map keyed by package name, whose `via` list
    holds either an advisory object or the name of another package that brought it in. Only
    the objects are advisories; the strings are edges in the graph."""
    found: list[Advisory] = []
    vulnerabilities = document.get("vulnerabilities")
    if not isinstance(vulnerabilities, dict):
        raise CouldNotRun("npm audit's JSON has no `vulnerabilities` object")
    seen: set[tuple[str, str]] = set()
    for name, item in sorted(vulnerabilities.items()):
        if not isinstance(item, dict):
            continue
        for via in item.get("via") or []:
            if not isinstance(via, dict):
                continue  # a string names another package, not an advisory
            url = str(via.get("url", ""))
            identifier = url.rstrip("/").rsplit("/", 1)[-1] if "/advisories/" in url else str(via.get("source", ""))
            package = str(via.get("name", name))
            if (identifier, package) in seen:
                continue
            seen.add((identifier, package))
            found.append(
                Advisory(
                    identifier=identifier or "unknown",
                    package=package,
                    ecosystem="npm",
                    severity=str(via.get("severity", item.get("severity", ""))),
                    title=str(via.get("title", "")).strip(),
                    url=url,
                    path=str(item.get("range", "")),
                )
            )
    return found


# --------------------------------------------------------------------------- running them


def run(command: list[str], cwd: Path) -> tuple[int, str, str]:
    try:
        completed = subprocess.run(  # noqa: S603 - a fixed command, no shell
            command, cwd=cwd, capture_output=True, text=True, timeout=600, check=False
        )
    except FileNotFoundError as exc:
        raise CouldNotRun(f"{command[0]} is not installed, so {' '.join(command)} could not run") from exc
    except subprocess.TimeoutExpired as exc:
        raise CouldNotRun(f"{' '.join(command)} did not finish within ten minutes") from exc
    return completed.returncode, completed.stdout, completed.stderr


def scan_with(scan: Scan, root: Path, parser) -> Scan:
    """Run one tool and parse it.

    **A non-zero exit is not evidence of failure, and a zero exit is not evidence of a
    pass.** Each of these tools exits non-zero precisely when it found something, so the
    exit code carries no information about whether the tool worked. The JSON does: if it
    cannot be parsed into the shape expected, the scan could not run."""
    directory = root / scan.directory
    if not directory.exists():
        raise CouldNotRun(f"{scan.directory} does not exist, so {scan.name} could not run")

    code, out, err = run(scan.command, directory)
    text = out.strip()
    if not text:
        raise CouldNotRun(
            f"{' '.join(scan.command)} in {scan.directory} printed no JSON "
            f"(exit {code}). stderr: {err.strip()[:400] or 'empty'}"
        )
    try:
        document = json.loads(text)
    except json.JSONDecodeError as exc:
        raise CouldNotRun(
            f"{' '.join(scan.command)} in {scan.directory} printed something that is not "
            f"JSON (exit {code}): {exc}. First 200 characters: {text[:200]!r}"
        ) from exc

    scan.advisories = parser(document)
    return scan


def available_scans(root: Path, only: str | None) -> list[tuple[Scan, object]]:
    planned: list[tuple[Scan, object]] = [
        (
            Scan("cargo audit", "cargo", ".", ["cargo", "audit", "--json"]),
            parse_cargo_audit,
        ),
        (
            Scan("pnpm audit (web)", "npm", "web", ["pnpm", "audit", "--json"]),
            parse_pnpm_audit,
        ),
        (
            Scan("npm audit (contract)", "npm", "contract", ["npm", "audit", "--json", "--package-lock-only"]),
            parse_npm_audit,
        ),
    ]
    if only:
        planned = [pair for pair in planned if pair[0].ecosystem == only or pair[0].name.startswith(only)]
        if not planned:
            raise CouldNotRun(f"no scan matches --only {only!r}")
    return planned


# --------------------------------------------------------------------------- the verdict


def evaluate(
    scans: list[Scan],
    allowlist: dependency_allowlist.Allowlist,
    today: datetime.date,
) -> tuple[list[str], list[str]]:
    """Judge the advisories found. Pure: no clock, no subprocess, no filesystem."""
    failures: list[str] = list(allowlist.problems)
    excused: list[str] = []

    for scan in scans:
        for advisory in sorted(scan.advisories, key=lambda a: a.sort_key):
            entry = allowlist.advisory_for(advisory.identifier, advisory.ecosystem)
            where = f"{advisory.package} via {scan.name}"
            summary = f"{advisory.severity or 'unrated'} {advisory.identifier} in {where}: {advisory.title}"
            if entry is None:
                failures.append(f"ADVISORY     {summary}. {advisory.url}")
            elif entry.expired(today):
                failures.append(
                    f"EXPIRED      {advisory.identifier} in {where} was allowlisted until "
                    f"{entry.look_again}, {(today - entry.look_again).days} days ago. "
                    f"{entry.decided_by} decided: {entry.reason}"
                )
            else:
                excused.append(
                    f"{advisory.identifier} ({advisory.severity or 'unrated'}) in {where} — "
                    f"allowed by {entry.decided_by} until {entry.look_again}: {entry.reason}"
                )

    seen = {(a.identifier.upper(), a.ecosystem) for scan in scans for a in scan.advisories}
    for entry in allowlist.advisories:
        if (entry.advisory.upper(), entry.ecosystem) not in seen:
            failures.append(
                f"ORPHANED     the allowlist excuses {entry.advisory} ({entry.ecosystem}), which "
                "no scan reports any more. Delete the entry."
            )

    return failures, excused


def render(scans: list[Scan], failures: list[str], excused: list[str]) -> str:
    lines = []
    for scan in scans:
        lines.append(f"{scan.name}: {len(scan.advisories)} advisory/advisories in {scan.directory}")
    lines.append("")
    if excused:
        lines.append(f"{len(excused)} allowlisted, each with a reason and a date to look again:")
        lines.extend(f"  - {item}" for item in excused)
        lines.append("")
    if failures:
        lines.append(f"{len(failures)} problem(s):")
        lines.extend(f"  {failure}" for failure in failures)
    else:
        total = sum(len(s.advisories) for s in scans)
        lines.append(
            f"{len(scans)} scan(s) ran and found {total} advisory/advisories, "
            f"{len(excused)} of them allowlisted. Nothing unexcused."
        )
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Fail on a known vulnerability in any dependency, direct or indirect."
    )
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--allowlist", type=Path, default=None)
    parser.add_argument("--only", default=None, help="run one ecosystem: cargo or npm")
    parser.add_argument("--today", type=datetime.date.fromisoformat, default=None)
    args = parser.parse_args(argv)

    root: Path = args.root.resolve()
    today = args.today or datetime.datetime.now(datetime.timezone.utc).date()
    allowlist_path = args.allowlist or root / "dependency-allowlist.toml"

    try:
        allowlist = dependency_allowlist.read(allowlist_path, today=today)
        for tool in ("cargo", "pnpm", "npm"):
            if args.only is None and shutil.which(tool) is None:
                raise CouldNotRun(
                    f"{tool} is not on the PATH. The scan needs all three audit tools; "
                    "running only some of them would report a clean bill of health for "
                    "dependencies nothing looked at."
                )
        scans = [scan_with(scan, root, parse) for scan, parse in available_scans(root, args.only)]
    except (CouldNotRun, dependency_allowlist.AllowlistError) as exc:
        print(f"advisory scan could not run: {exc}", file=sys.stderr)
        print(
            "Nothing was concluded about any dependency. This is a failure, not a pass: a "
            "check that passes without running carries no information.",
            file=sys.stderr,
        )
        return EXIT_COULD_NOT_RUN

    failures, excused = evaluate(scans, allowlist, today)
    print(render(scans, failures, excused))

    if failures:
        print(
            f"\nadvisory scan failed: {len(failures)} problem(s). Update the dependency, or "
            f"add an entry to {allowlist_path.name} with a reason, who decided and a date to "
            "look again.",
            file=sys.stderr,
        )
        return EXIT_POLICY
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
