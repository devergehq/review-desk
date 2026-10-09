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

import tomllib  # noqa: E402  (below the guard, deliberately)

sys.path.insert(0, str(Path(__file__).resolve().parent))

import dependency_allowlist  # noqa: E402  (after the sys.path line, deliberately)

EXIT_OK = 0
EXIT_POLICY = 1
EXIT_COULD_NOT_RUN = 2

# **Unknown sorts FIRST.** An unrated advisory is one nobody has assessed, not one that is
# harmless, and it had been sitting below `low` -- so a RustSec advisory, which is unrated
# until its vector is scored, printed beneath an npm `low`.
NOT_SOURCE = {"node_modules", "target", ".git", ".nuxt", ".output", "dist", ".data", "__pycache__"}

SEVERITY_ORDER = {
    "": 0,
    "unknown": 0,
    "critical": 1,
    "high": 2,
    "moderate": 3,
    "medium": 3,
    "low": 4,
    "info": 5,
}


class CouldNotRun(Exception):
    """The scan could not reach a conclusion. Always exit 2, never a pass."""


# Every way the three native tools can be told to stay quiet about an advisory.
#
# **The point of this list is that the scan is worthless without it.** Each tool reports
# only what it has not been configured to hide, so a scan that reads their output without
# checking their configuration measures the configuration, not the dependencies. Six lines
# in a file nobody diffs and the advisory is gone -- with no reason, no decider and no
# expiry, which is everything `dependency-allowlist.toml` exists to require. Worse, the
# advisory then has no match for its allowlist entry and the entry reports as ORPHANED,
# which actively pushes a reviewer to delete the documented decision. That full sequence was
# reproduced on pnpm 11.9.0 before this was written.
#
# **Measured, not assumed.** On pnpm 11.9.0, `pnpm.auditConfig.ignoreGhsas` in a
# `package.json` has NO effect -- the surface that works is `auditConfig` in
# `pnpm-workspace.yaml`, and the advisory is deleted from the output rather than moved to a
# `muted` key. Both locations are checked anyway: the one that works today, and the one the
# documentation describes, because which is which may change again.
PNPM_IGNORE_KEYS = ("ignoreGhsas", "ignoreCves")
CARGO_AUDIT_FILES = (".cargo/audit.toml", "audit.toml")


class SuppressionFound(Exception):
    """A native suppression surface is in use. The scan refuses to run rather than report a
    filtered answer as a clean one."""


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
        return (SEVERITY_ORDER.get(self.severity.lower(), 0), self.ecosystem, self.package, self.identifier)


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


# CVSS v3.1 base-metric weights, from the specification. PR depends on Scope, which is why
# it is a pair of tables rather than one.
_AV = {"N": 0.85, "A": 0.62, "L": 0.55, "P": 0.20}
_AC = {"L": 0.77, "H": 0.44}
_UI = {"N": 0.85, "R": 0.62}
# High / Low / None. CVSS v2 used H/M/N and getting that wrong returned None for every
# vector with an `L` impact metric, which is most of them.
_CIA = {"H": 0.56, "L": 0.22, "N": 0.0}
_PR_UNCHANGED = {"N": 0.85, "L": 0.62, "H": 0.27}
_PR_CHANGED = {"N": 0.85, "L": 0.68, "H": 0.50}


def cvss31_base_score(vector: str) -> float | None:
    """The CVSS v3.1 base score for a vector string, or None if it cannot be scored.

    **This exists because real `cargo audit --json` gives a vector, not a number.** The
    advisory object carries `"cvss": "CVSS:3.1/AV:L/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H"` and
    `"severity": null`, so an earlier version of this function returned "" for every Rust
    advisory there has ever been -- and "" sorted *below* `low`, which put a RustSec
    critical underneath an npm `low`.

    Only v3.0 and v3.1 are scored. A v4.0 vector uses a different algorithm and scoring it
    with this one would be confidently wrong, so it comes back None and is reported as
    unknown -- which now sorts first."""
    if not isinstance(vector, str) or not vector.startswith("CVSS:3."):
        return None

    metrics = {}
    for part in vector.split("/")[1:]:
        key, _, value = part.partition(":")
        metrics[key] = value

    try:
        scope_changed = metrics["S"] == "C"
        privileges = (_PR_CHANGED if scope_changed else _PR_UNCHANGED)[metrics["PR"]]
        exploitability = (
            8.22 * _AV[metrics["AV"]] * _AC[metrics["AC"]] * privileges * _UI[metrics["UI"]]
        )
        confidentiality = _CIA[metrics["C"]]
        integrity = _CIA[metrics["I"]]
        availability = _CIA[metrics["A"]]
    except KeyError:
        return None

    impact_sub = 1 - ((1 - confidentiality) * (1 - integrity) * (1 - availability))
    if scope_changed:
        impact = 7.52 * (impact_sub - 0.029) - 3.25 * (impact_sub - 0.02) ** 15
    else:
        impact = 6.42 * impact_sub

    if impact <= 0:
        return 0.0

    raw = min((1.08 if scope_changed else 1.0) * (impact + exploitability), 10.0)
    # The specification's Roundup: round up to one decimal place, done on integers so that
    # floating point cannot round 6.2 down to 6.1.
    scaled = int(round(raw * 100000))
    if scaled % 10000 == 0:
        return scaled / 100000.0
    return (scaled // 10000 + 1) / 10.0


def band_for(score: float) -> str:
    """The CVSS v3 qualitative rating for a base score."""
    for threshold, band in ((9.0, "critical"), (7.0, "high"), (4.0, "moderate"), (0.1, "low")):
        if score >= threshold:
            return band
    return "info"


def cargo_severity(advisory: dict) -> str:
    """RustSec's severity band for one advisory, or "" when it genuinely cannot be known.

    "" is not a synonym for harmless: see SEVERITY_ORDER, where it sorts first."""
    word = advisory.get("severity")
    if isinstance(word, str) and word.strip():
        return word.strip().lower()

    score = advisory.get("cvss")
    if isinstance(score, (int, float)):
        return band_for(float(score))
    if isinstance(score, str):
        # Either a bare number, or -- as cargo audit actually sends it -- a vector.
        try:
            return band_for(float(score))
        except ValueError:
            pass
        computed = cvss31_base_score(score)
        if computed is not None:
            return band_for(computed)
    return ""


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
                severity=cargo_severity(advisory),
                title=str(advisory.get("title", "")).strip(),
                url=str(advisory.get("url") or f"https://rustsec.org/advisories/{advisory.get('id', '')}"),
                path=str(package.get("version", "")),
            )
        )
    return found


def parse_pnpm_audit(document: dict) -> list[Advisory]:
    """`pnpm audit --json`. An `advisories` map keyed by npm advisory id; the GitHub id is
    the stable one to allowlist against, because the numeric npm id is not what the
    advisory is called anywhere a person would look it up.

    **A `muted` list, if present, is read as advisories rather than ignored.** pnpm's
    documentation describes silenced advisories moving to `muted`; measured on pnpm 11.9.0
    they are simply deleted from `advisories` instead, which is why `find_suppressions`
    exists and is the real defence. This reads `muted` anyway so that a pnpm which does
    produce it reports the advisory rather than letting it vanish -- a vanished advisory
    takes its allowlist entry down with it, as an ORPHANED failure."""
    found: list[Advisory] = []
    for muted in document.get("muted") or []:
        if not isinstance(muted, dict):
            continue
        found.append(
            Advisory(
                identifier=str(muted.get("github_advisory_id") or muted.get("id") or "unknown"),
                package=str(muted.get("module_name", "unknown")),
                ecosystem="npm",
                severity=str(muted.get("severity", "")),
                title=str(muted.get("title", "")).strip() + " [muted by configuration]",
                url=str(muted.get("url", "")),
            )
        )
    advisories = document.get("advisories")
    if advisories is None and isinstance(document.get("metadata"), dict):
        # A clean run can omit the map entirely rather than sending an empty one.
        advisories = {}
    if not isinstance(advisories, dict):
        if found:
            return found
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


def _uncommented(text: str) -> str:
    """The file with `#` comments removed, so that naming a key in a comment -- as this
    repository's own files do, to explain the rule -- is not read as using it."""
    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())


def find_suppressions(root: Path, npm_manifests: list[Path]) -> list[str]:
    """Every native suppression surface in use, as a list of complaints.

    Deliberately crude for the YAML: the standard library has no YAML parser, so
    `pnpm-workspace.yaml` is scanned by line. A detector for a safety assertion should err
    towards firing, and the cost of a false positive is one comment moved, while the cost of
    a false negative is a silenced critical."""
    found = []

    for manifest in npm_manifests:
        relative = manifest.relative_to(root).as_posix()
        try:
            data = json.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise CouldNotRun(f"could not read {relative}: {exc}") from exc
        config = (data.get("pnpm") or {}).get("auditConfig") if isinstance(data.get("pnpm"), dict) else None
        if isinstance(config, dict):
            for key in PNPM_IGNORE_KEYS:
                if config.get(key):
                    found.append(f"{relative} sets pnpm.auditConfig.{key}")
        for key in PNPM_IGNORE_KEYS:
            if isinstance(data.get("auditConfig"), dict) and data["auditConfig"].get(key):
                found.append(f"{relative} sets auditConfig.{key}")

    for workspace in sorted(root.rglob("pnpm-workspace.yaml")):
        if set(workspace.relative_to(root).parts) & NOT_SOURCE:
            continue
        relative = workspace.relative_to(root).as_posix()
        body = _uncommented(workspace.read_text(encoding="utf-8"))
        for key in PNPM_IGNORE_KEYS:
            if key in body:
                found.append(f"{relative} sets auditConfig.{key}")

    for name in CARGO_AUDIT_FILES:
        path = root / name
        if not path.exists():
            continue
        try:
            config = tomllib.loads(path.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError) as exc:
            raise CouldNotRun(f"could not read {name}: {exc}") from exc
        advisories = config.get("advisories")
        if isinstance(advisories, dict) and advisories.get("ignore"):
            found.append(f"{name} sets [advisories] ignore")

    return found


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


def scanned_npm_manifests(root: Path) -> list[Path]:
    """The `package.json` files whose suppression config matters. Walked, not named, so a
    new npm directory is covered the day it appears."""
    found = []
    for path in root.rglob("package.json"):
        if set(path.relative_to(root).parts) & NOT_SOURCE:
            continue
        found.append(path)
    return sorted(found)


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
            summary = f"{advisory.severity or 'UNRATED'} {advisory.identifier} in {where}: {advisory.title}"
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
                    f"{advisory.identifier} ({advisory.severity or 'UNRATED'}) in {where} — "
                    f"allowed by {entry.decided_by} until {entry.look_again}: {entry.reason}"
                )

    # An orphan can only be judged for an ecosystem this run actually scanned. With
    # `--only cargo` the npm scans do not run, and without this every npm entry would read
    # as orphaned -- a partial run inventing failures about dependencies nothing looked at.
    scanned = {scan.ecosystem for scan in scans}
    seen = {(a.identifier.upper(), a.ecosystem) for scan in scans for a in scan.advisories}
    for entry in allowlist.advisories:
        if entry.ecosystem not in scanned:
            continue
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

        # Before running anything: the three tools report only what they have not been told
        # to hide, so their configuration has to be checked or the scan measures the
        # configuration rather than the dependencies.
        suppressions = find_suppressions(root, scanned_npm_manifests(root))
        if suppressions:
            raise SuppressionFound("; ".join(suppressions))

        for tool in ("cargo", "pnpm", "npm"):
            if args.only is None and shutil.which(tool) is None:
                raise CouldNotRun(
                    f"{tool} is not on the PATH. The scan needs all three audit tools; "
                    "running only some of them would report a clean bill of health for "
                    "dependencies nothing looked at."
                )
        scans = [scan_with(scan, root, parse) for scan, parse in available_scans(root, args.only)]
    except SuppressionFound as exc:
        print("advisory scan refused to run: a native suppression is in use.", file=sys.stderr)
        for complaint in str(exc).split("; "):
            print(f"  {complaint}", file=sys.stderr)
        print(
            "\nThese hide an advisory from the tool that reports it, so the scan would have "
            f"reported a filtered answer as a clean one. Remove them and record the decision "
            f"in {allowlist_path.name} instead, which requires a reason, who decided and a "
            "date to look again.",
            file=sys.stderr,
        )
        return EXIT_POLICY
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
