import { describe, expect, it } from 'vitest'

import type { Finding, Outcome } from '../app/types/desk'
import { sinceRound, sinceRoundLines } from '../app/utils/outcomes'

/*
 * **S16's Since round 1, from what a later round holds.** DEV-1329.
 *
 * A round's outcomes are its own, and opening round 2 copies none of round 1's — so what the
 * later round can say about the earlier one is each carried finding's `round_status`, plus
 * the one act it records on one, `raised_again`. These cases are S16's own: F9 fixed in a
 * commit, F5 answered, F3 raised again, F1 asked again, F10 to a ticket.
 */

function finding(seq: number, fields: Partial<Finding>): Finding {
  return {
    review_id: 2,
    seq,
    key: `F${seq}`,
    lens: 'correctness',
    severity: 'should',
    line_sha: null,
    claim: `Claim ${seq}`,
    disposition: 'in_draft',
    github_comment_id: null,
    created_at: '2026-11-20T00:00:00Z',
    round_status: 'new',
    continues_review_id: null,
    continues_round: null,
    ...fields,
  } as Finding
}

const carried = (seq: number, fields: Partial<Finding>) =>
  finding(seq, { continues_review_id: 1, continues_round: 1, ...fields })

const raisedAgain = (seq: number) =>
  ({ finding_seq: seq, kind: 'raised_again' }) as Pick<Outcome, 'finding_seq' | 'kind'>

describe('since the round before', () => {
  it('is nothing for a round that carried nothing', () => {
    expect(sinceRound([finding(1, {}), finding(2, {})], [])).toBeNull()
  })

  it('groups each carried finding by what the record says became of it', () => {
    const since = sinceRound([
      carried(9, { round_status: 'fixed', round_status_commit: '3c1e2a9ffe' }),
      carried(5, { round_status: 'accepted' }),
      carried(3, { round_status: 'still_open' }),
      carried(1, { round_status: 'still_open' }),
      carried(10, { round_status: 'ticketed', disposition: 'to_ticket' }),
      finding(12, {}),
    ], [raisedAgain(3)])!

    expect(since.from).toBe(1)
    expect(since.fixed).toEqual([{ seq: 9, key: 'F9', commit: '3c1e2a9' }])
    expect(since.answered.map(one => one.key)).toEqual(['F5'])
    expect(since.raisedAgain.map(one => one.key)).toEqual(['F3'])
    expect(since.askedAgain.map(one => one.key)).toEqual(['F1'])
    expect(since.ticketed.map(one => one.key)).toEqual(['F10'])
  })

  /*
   * Asked again means posting again: a carried finding still open but set aside this round
   * is not being asked, and its disposition already says where it went.
   */
  it('counts a still-open finding as asked again only when it posts', () => {
    const since = sinceRound([carried(4, { round_status: 'still_open', disposition: 'cut' })], [])!
    expect(since.askedAgain).toEqual([])
  })

  it('says it in S16’s sentences, a group with nothing in it saying nothing', () => {
    const since = sinceRound([
      carried(9, { round_status: 'fixed', round_status_commit: '3c1e2a9' }),
      carried(5, { round_status: 'accepted' }),
      carried(3, { round_status: 'still_open' }),
      carried(1, { round_status: 'still_open' }),
      carried(10, { round_status: 'ticketed', disposition: 'to_ticket' }),
    ], [raisedAgain(3)])!

    expect(sinceRoundLines(since)).toEqual([
      'F9 fixed in 3c1e2a9, F5 answered.',
      'F3 raised again, F1 asked again.',
      'F10 still goes to a ticket.',
    ])
    expect(sinceRoundLines(sinceRound([carried(2, { round_status: 'still_open' })], [])!))
      .toEqual(['F2 asked again.'])
  })

  /* A carry from round 2 into round 3 is "since round 2". */
  it('names the round the findings came from', () => {
    expect(sinceRound([carried(2, { continues_round: 2, round_status: 'fixed' })], [])!.from).toBe(2)
  })
})
