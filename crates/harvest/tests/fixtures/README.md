# Recorded GitHub responses

Captured from **`devergehq/locus#64`**, a merged pull request carrying all four facts
the harvester reads: a merge, six review comment threads each with one reply, every
thread resolved, and three commits after the commit the review read.

Nothing here was hand-written. Each file is what `gh` printed, narrowed with `jq` to the
fields `crates/harvest/src/wire.rs` reads — the patches, bodies, logins, shas and times
are verbatim.

| File | Captured with |
| -- | -- |
| `pull_request.json` | `gh api repos/devergehq/locus/pulls/64 \| jq '{number, state, merged, merged_at, closed_at, head: {sha: .head.sha}, user: {login: .user.login}}'` |
| `review_threads.json` | `gh api graphql -f query='…reviewThreads(first:50){nodes{isResolved resolvedBy{login} comments(first:20){nodes{databaseId author{login} body createdAt}}}}…'` |
| `compare.json` | `gh api repos/devergehq/locus/compare/9535132…...46f6ea1… \| jq '{status, ahead_by, total_commits, commits: [.commits[] \| {sha, commit: {committer: {date: .commit.committer.date}}}], files: [.files[] \| {filename, status, patch}]}'` |
| `commit-SHA.json` | `gh api repos/devergehq/locus/commits/SHA \| jq '{sha, commit: {committer: {date: .commit.committer.date}}, files: [.files[] \| {filename}]}'` |

The reviewed commit is `953513212a0e1400d7911b204150585661f5aab1` — the
`original_commit_id` the review comments are anchored to — and the pull request's head
is now `46f6ea1645a9bbc9308f791411b2cac1a093367b`.

## What the recording happens to contain

The tests lean on these, so they are worth stating:

| In `skills/dispatcher/dispatcher.py` | At the reviewed commit |
| -- | -- |
| Lines 471 and 1282 | Removed or replaced later — a finding there has had **its own lines** changed |
| Line 873 | Untouched — a finding there has had **only the file** change around it |

`skills/dispatcher/workers/implement.md` has a review thread but does not appear in the
comparison at all: a finding there gets no `code_changed_after` outcome.
