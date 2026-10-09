# Dependencies

**The bar: a dependency must be active, maintained and popular, and that is checked before
it is adopted, not after.** — Patrick, 9 October 2026.

Review Desk shipped an HTTP server on `tiny_http`, a crate with no release since 6 October
2022, and found out when its connection handling hung the dashboard. Nothing in the
repository was watching. Three things watch now.

| Piece | What it does | Where |
| -- | -- | -- |
| Update bot | Opens one grouped pull request per ecosystem per week | `.github/dependabot.yml` |
| Advisory scan | Fails on a known vulnerability, direct or indirect | `tools/advisory_scan.py` |
| Release-age check | Fails when a direct dependency's latest release is over a year old | `tools/release_age.py` |
| The allowlist | What is knowingly lived with, and until when | `dependency-allowlist.toml` |

The advisory scan and the release-age check run in
`.github/workflows/dependencies.yml`, alongside a job that runs their own tests offline.
The update bot is Dependabot, which GitHub runs from `.github/dependabot.yml` and not in
any workflow of ours.

## Adding a dependency

1. **Check it before you write the code that needs it.** Open the registry page. Four
   questions: when was the last release, how many downloads, is it deprecated, and who
   maintains it. More than a year since the last release and the release-age check will
   fail your pull request — find out now rather than from CI.
2. **Prefer no new dependency.** A short script in the repository is better than a package,
   and this file is itself the example: both checks are standard-library Python, because a
   tool that watches dependencies should not arrive with a dependency tree.
3. **Prefer a first-party or widely used tool over a small one.** `cargo-audit` is the
   RustSec project's own tool. `ajv-formats` is the `ajv` project's own companion. A
   one-maintainer package that does the same thing is a worse bet even when it is newer.
4. **Say in the pull request what you checked.** The latest release date and the repository,
   for each thing you added. A reviewer should not have to go and look.
5. **Run the checks before you push.** `python3 tools/release_age.py` and
   `python3 tools/advisory_scan.py`.

## Running the checks

```sh
python3 -m unittest discover --start-directory tools   # the checks' own logic, offline
python3 tools/release_age.py                           # ask the registries, ~30 seconds
python3 tools/release_age.py --format markdown         # the same thing as a table
python3 tools/advisory_scan.py                         # cargo, pnpm and npm audit
```

Both need Python 3.11 or newer, for `tomllib`. Nothing else. On an older interpreter they
refuse with that sentence rather than a traceback, and a test asserts the import order that
makes that true — getting it wrong once meant the guard could never fire.

## What the release-age check measures, and what it does not

**It measures whether upstream is still alive.** It asks each registry for the newest
release of every direct dependency and fails over 365 days. That is the question the
`tiny_http` incident asked: nothing was wrong with the code on offer, there simply was no
newer code and nobody to write any.

**It does not measure how far behind you are.** A dependency three majors out of date
passes if upstream shipped last month. When a manifest pins an exact version, the check
reports the age of *that* release as a note, but it does not fail on it. The case that made
this worth saying was `contract/`'s own: `ajv` was pinned at `8.17.1` from July 2024, 818
days old, while `8.20.0` was 168 — and the release-age check passed, because it was reading
`8.20.0`. The same change that added the check bumped the pin, so the example no longer
exists in this tree; it is kept here because the gap it describes still can. Keeping up to
date is Dependabot's job; being alive is this check's.

Three further decisions, written down because each was argued:

- **A pre-release is not a release.** Measuring from any published version at all reported
  `@redocly/cli` as `0.0.0-snapshot.1791443981` and `typescript` as `7.1.0-dev.20261008.1`:
  true, unrecognisable, and the wrong question, since you cannot depend on a nightly. The
  newest pre-release is still printed, so a reader can see the project is moving.
- **A dependency no registry can date is a failure, not a skip.** A `git` dependency, an
  alternative registry, a `github:` npm specifier. Skipping them would make the check
  trivial to walk around; they have to be allowlisted deliberately.
- **365 days exactly passes; 366 fails.** `tools/test_release_age.py` holds both, so it is
  not argued about in a pull request.

## The allowlist

An entry in `dependency-allowlist.toml` is the only thing that makes either check pass over
a finding, with one exception: **a dependency the registry has never heard of cannot be
allowlisted.** Every other finding is a judgement call about something old; that one is
about something that cannot be installed at all, and there is nothing to weigh. Every entry carries the same fields:

| Field | What it is |
| -- | -- |
| `dependency` / `advisory` | What is being excused |
| `ecosystem` | `cargo` or `npm` |
| `reason` | Why this is acceptable. Not "known issue": the failure it does not cause. |
| `decided_by` | Who decided. A person. |
| `look_again` | When to look again. Past that date, the check fails. |

**A malformed entry fails the check rather than being skipped**, and that is the most
important property in the file. `look-again` written for `look_again` would otherwise read
as an entry with no expiry and no reason, and the check would still pass — so an unknown
key, a missing key, a reason under twenty characters, a date that is not a bare TOML date,
and a date more than 550 days away are all failures. So is an entry for a dependency or an
advisory that nothing reports any more: without that, an entry lands in one pull request as
housekeeping and the thing it excuses lands in the next, and both are green.

**An allowlist entry is a decision about the product, not a way to get a build green.**
Where a maintained replacement exists and swapping to it is a small change, swap.

**Do not use the audit tools' own ignore lists.** `pnpm`'s `auditConfig.ignoreGhsas` and
`ignoreCves`, and `cargo audit`'s `[advisories] ignore`, all hide an advisory from the tool
that reports it — with no reason, no decider and no expiry. The advisory scan refuses to run
when it finds one and names the file, because a scan that reads a filtered answer reports
the configuration rather than the dependencies. Worse, a hidden advisory leaves its
allowlist entry with nothing to match, so the entry then fails as `ORPHANED` and the fix
looks like deleting the decision. That whole sequence was reproduced before the refusal was
written.

## What runs when, and what that misses

| Trigger | Why |
| -- | -- |
| Pull request touching a manifest, a lockfile, the allowlist or `tools/` | A dependency arriving through any of the four files it can arrive through |
| Weekly, Monday 07:17 UTC, on `master` | Both checks' inputs change with nobody pushing: a new advisory lands against an untouched lockfile, and a dependency passes a year by the calendar moving |
| By hand (`workflow_dispatch`) | See below |

**The weekly run on `master` is the authoritative one.** A pull-request run executes the
pull request's own copy of the tool and the allowlist, so a pull request can weaken the
check it is being judged by — raise the limit, add its own entry, edit the tool. The weekly
run is what catches that, a week later. There is no branch protection on this repository
today; if there ever is, `tools/`, `dependency-allowlist.toml` and `.github/` are the paths
worth owning.

**GitHub disables a scheduled workflow after 60 days of repository inactivity.** This is a
one-developer repository whose history arrives in bursts, and the quiet stretch after a
burst is exactly when a dependency rots. After a quiet spell, check that the Dependencies
workflow is still scheduled, or run it by hand from the Actions tab. A check that has
stopped running is a check that passes without information, which is the thing none of this
is allowed to do.

## What the update bot produces

Four ecosystems — the Cargo workspace, `web/`, `contract/` and the workflows — grouped so
that minor and patch updates arrive as one pull request per ecosystem, with majors arriving
individually.

**Expect two to four pull requests in a typical week.** The Cargo group and the `web` group
most weeks; `contract` (four pinned tools) and the actions (five) roughly monthly; plus the
occasional single-package major. An ungrouped bot on this set would open around ten a week.

**Majors are not grouped on purpose.** A single group per ecosystem looked tidier and was
worse: by the third month the `web` group arrives with eleven members and one breaking
change among them, Dependabot cannot merge a group partially, and the whole weekly stream
sits behind the one pull request nobody can take.

**Nothing auto-merges.** Every update is reviewed.

**Dependabot could not have caught `tiny_http`.** Abandonment means no new releases, so
there was never a pull request for it to open. That is why the release-age check exists
alongside it, and why removing the release-age check as redundant would be a mistake.

## Decisions on record

| Dependency | Decision | Why | Date |
| -- | -- | -- | -- |
| `typescript` | **Stay on `^6.0.3`.** Decided by Patrick, 9 October 2026. **Re-check early December 2026.** | TypeScript 7.0 ships no programmatic compiler API, so `vue-tsc` — and therefore `nuxt typecheck` — cannot run on it at all: `nuxt/nuxt#36440` is this exact command failing with `ERR_PACKAGE_PATH_NOT_EXPORTED ('./lib/tsc')`, closed `not_planned` on 30 September 2026. Microsoft's own 7.0 announcement tells Vue projects to stay on 6.0, and `typescript-eslint` 8.71.1 declares `>=4.8.4 <6.1.0`, which excludes 7. The pin is deliberate, not drift. Re-check once TypeScript 7.1 (planned 24 November 2026) has shipped the API **and** `vue-tsc` declares support — both, not either. `.github/dependabot.yml` ignores the major until then, with the same date on it. | 9 Oct 2026 |
| `serde_norway` | Allowlisted, not replaced | Test-only, and the best-maintained serde-integrated YAML crate that exists: upstream `serde_yaml` is archived, `serde_yaml_ng` is 200 days older, `serde_yml` is on `0.0.x`, and `yaml-rust2`/`saphyr` are parsers with no serde integration. | 9 Oct 2026 |
| `ajv-formats` | Allowlisted, not replaced | Finished rather than abandoned, first-party to `ajv`, 140M downloads a week, and the only way to get format assertions out of `ajv` 8. CI-only. | 9 Oct 2026 |
| `cargo-audit` over `rustsec/audit-check` | The binary, pinned | The action's most recent release is 2024-09-23, 746 days old. It would fail the check this repository now runs. | 9 Oct 2026 |

`dependency-allowlist.toml` holds the two release-age entries in full, with their look-again
dates, and the advisory entries alongside them.

**One advisory entry is not settled.** `katex` (`GHSA-238p-pmpm-9mq7`) is the only package of
the seven that is actually present in the shipped build — `_nuxt/ZlcWpGUi.js`, 258,685 bytes,
fetched when a diagram contains maths. An earlier version of the allowlist called all seven
"build-time tooling", which was wrong for that one. Its entry says so and awaits a decision
on the corrected facts. The lesson is in the allowlist's own comment: `braces` and `katex`
both occur as ordinary English, so presence in a bundle is established by license banners,
error strings and API symbols, never by grepping the package name.
