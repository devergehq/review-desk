import { describe, expect, it } from 'vitest'

import { commentUrl, prUrl, ticketUrl } from '../app/utils/links'

/*
 * **The three addresses a review's header and rows link to.**
 *
 * `app/utils/links.ts` says why they differ: the pull request's is *built* from `repo` and
 * `pr`, and the ticket's is *stored*, because nothing can turn an opaque `ACME-2045` into an
 * address.
 *
 * ## Why `prUrl` needs a list of its own
 *
 * The rule "a repository name is in `[A-Za-z0-9._-]`, and neither half is exactly `.` or
 * `..`" has **three** implementations: the contract's `RepoName` pattern,
 * `normalise::is_repo_name` in the store, and `prUrl` here. The first two are held to each
 * other by `crates/server/tests/conformance.rs`, over a list that deliberately includes the
 * dot-leading names the rule *accepts* — `.../x`, `..a/b`, `.a/.b` — so that neither side
 * can be quietly tightened.
 *
 * `prUrl` was held to none of that. The component tests cover only the refused side, and no
 * fixture anywhere carries a dot-leading or three-dot repository name, so
 * `if (part.startsWith('.')) return null` would have kept every test in the repository green
 * while the dashboard drew a `.../x` review as unlinkable text that the store, the server
 * and `review open` all accept. That is the same gap the conformance test closed, one
 * boundary over, and this is the same list.
 */

/** The names the rule accepts, including the ones only a careless tightening would lose. */
const ACCEPTED = [
  'acme/portal',
  'devergehq/review-desk.v2_beta-1',
  'a/b',
  '.../x',
  'x/...',
  '..a/b',
  'a/b..',
  '.a/.b',
  '-/_',
]

/** The names it refuses, and `../..` is the one with teeth. */
const REFUSED = [
  '../..',
  './portal',
  'acme/..',
  'acme/.',
  '../portal',
  'acme/portal#x',
  'acme/portal?x=1',
  'acme/port al',
  'acme/portal/extra',
  'acme/',
  '/portal',
  'acme',
  '',
]

describe('where a review’s pull request is', () => {
  it('builds the address from the repository and the number', () => {
    for (const repo of ACCEPTED) {
      expect(prUrl(repo, 412), repo).toBe(`https://github.com/${repo}/pull/412`)
    }
  })

  it('refuses a name it cannot build a link out of', () => {
    for (const repo of REFUSED) {
      expect(prUrl(repo, 412), repo).toBeNull()
    }
  })

  /*
   * The two with teeth, spelled out because the reason is not obvious from the list: a
   * browser resolves the dot segments rather than fetching them, so the address goes
   * somewhere other than the repository the link's text names.
   */
  it('refuses the names whose link would go somewhere else', () => {
    expect(new URL('https://github.com/../../pull/1').href).toBe('https://github.com/pull/1')
    expect(prUrl('../..', 1)).toBeNull()
    expect(prUrl('./portal', 1)).toBeNull()
  })
})

describe('where a review’s ticket is', () => {
  it('passes an https address through', () => {
    expect(ticketUrl('https://linear.app/acme/issue/ABC-231'))
      .toBe('https://linear.app/acme/issue/ABC-231')
  })

  it('refuses anything else, and absence is not a refusal', () => {
    for (const refused of [
      'javascript:alert(1)',
      'http://linear.app/acme/issue/ABC-231',
      'linear.app/acme/issue/ABC-231',
      'HTTPS://linear.app/acme/issue/ABC-231',
      'data:text/html,<script>alert(1)</script>',
    ]) {
      expect(ticketUrl(refused), refused).toBeNull()
    }
    expect(ticketUrl(null)).toBeNull()
    expect(ticketUrl(undefined)).toBeNull()
    expect(ticketUrl('')).toBeNull()
  })
})

describe('the comment a finding was posted as', () => {
  it('is addressed by its numeric id, under the pull request', () => {
    expect(commentUrl('acme/portal', 398, '2314990002')).toBe(
      'https://github.com/acme/portal/pull/398#discussion_r2314990002',
    )
  })

  /*
   * **A node id is not an address.** `IC_kwDOA1…` appended to `#discussion_r` is a link
   * whose text says "the comment" and whose destination is the top of the pull request,
   * which is the defect `prUrl` refuses to build. The row draws no link instead, and the
   * page's header already reaches the pull request.
   */
  it('is not built from an id that is not GitHub’s numeric comment id', () => {
    expect(commentUrl('acme/portal', 398, 'IC_kwDOA1')).toBeNull()
    expect(commentUrl('acme/portal', 398, '2314990002x')).toBeNull()
    expect(commentUrl('acme/portal', 398, ' 2314990002')).toBeNull()
  })

  it('is nothing for a finding that was never posted as a comment', () => {
    expect(commentUrl('acme/portal', 398, null)).toBeNull()
    expect(commentUrl('acme/portal', 398, undefined)).toBeNull()
    expect(commentUrl('acme/portal', 398, '')).toBeNull()
  })

  /* And it refuses whatever `prUrl` refuses, because it is built on it. */
  it('is nothing where the repository is not a repository name', () => {
    expect(commentUrl('portal', 398, '2314990002')).toBeNull()
    expect(commentUrl('../..', 398, '2314990002')).toBeNull()
  })
})
