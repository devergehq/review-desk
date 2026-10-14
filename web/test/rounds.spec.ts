/*
 * **A review's rounds, as rules rather than as a render — the two questions `utils/rounds.ts`
 * answers, tested in the order that file states them.**
 *
 * The sentences a page says about *this* round's head, and the one decision in them (what to
 * say when the record cannot count the commits); then where the pull request's *other* rounds
 * are, which is DEV-976. One file because one module, and because a reader who finds this one
 * should not have to know there is a second named almost the same thing.
 */
import { describe, expect, it } from 'vitest'

import {
  baseOnlyLine,
  commitLabel,
  foldedLine,
  movesSince,
  updatedLine,
  withdrawnLine,
  briefWrittenEarlier,
  roundLinks,
  roundsLabel,
} from '~/utils/rounds'
import { loadSource } from '../app/data/sample'

import type {
  Draft,
  HeadUpdate,
  HeadUpdates,
  Review,
  ReviewDetail,
  RoundSummary,
} from '~/types/desk'

function updates(over: Partial<HeadUpdates> = {}): HeadUpdates {
  return {
    since_sha: null,
    moved: 0,
    base_only: 0,
    folded: 0,
    commits: null,
    last_at: null,
    updates: [],
    ...over,
  }
}

function move(from: string, to: string, at: string): HeadUpdate {
  return { kind: 'moved', from_sha: from, to_sha: to, at }
}

describe('what a page says about a head that moved', () => {
  it('says nothing at all when the head has never moved', () => {
    expect(updatedLine(updates())).toBeNull()
    expect(baseOnlyLine(updates())).toBeNull()
    expect(foldedLine(updates())).toBeNull()
  })

  it('counts the commits when the record knows them', () => {
    expect(updatedLine(updates({ moved: 1, since_sha: '4d9464f', commits: 3 })))
      .toBe('Updated for 3 commits since 4d9464f')
  })

  it('says one commit in the singular', () => {
    expect(updatedLine(updates({ moved: 1, since_sha: '4d9464f', commits: 1 })))
      .toBe('Updated for 1 commit since 4d9464f')
  })

  it('drops the count rather than printing a nought', () => {
    // A null count is a round whose commits nobody recorded. "Updated for 0 commits" would
    // read as "nothing changed", which is the one thing a moved head is not.
    expect(updatedLine(updates({ moved: 2, since_sha: '4d9464f', commits: null })))
      .toBe('Updated since 4d9464f')
    expect(updatedLine(updates({ moved: 2, since_sha: '4d9464f', commits: 0 })))
      .toBe('Updated since 4d9464f')
  })

  it('abbreviates a full commit to the seven characters a reader quotes', () => {
    expect(commitLabel('4d9464f1b7c4e0aa41c9e2c7730bd9d02e713f9c')).toBe('4d9464f')
    expect(commitLabel('4d9464f')).toBe('4d9464f')
  })

  it('says a base-branch merge changed nothing', () => {
    expect(baseOnlyLine(updates({ base_only: 1 })))
      .toBe('1 push brought the base branch in and changed nothing')
    expect(baseOnlyLine(updates({ base_only: 2 })))
      .toBe('2 pushes brought the base branch in and changed nothing')
  })

  it('says how many earlier rounds the tidy folded in', () => {
    expect(foldedLine(updates({ folded: 1 }))).toBe('An earlier round was folded into this one')
    expect(foldedLine(updates({ folded: 3 }))).toBe('3 earlier rounds were folded into this one')
  })
})

describe('what changed since the draft was written', () => {
  const held = updates({
    moved: 2,
    since_sha: '4d9464f',
    commits: 3,
    updates: [
      move('4d9464f', 'a41c9e2', '2026-10-10T09:00:00Z'),
      { kind: 'base_only', from_sha: 'a41c9e2', to_sha: '7e2a9d3', at: '2026-10-10T10:00:00Z' },
      move('a41c9e2', '1b7c4e0', '2026-10-11T08:00:00Z'),
    ],
  })

  it('takes the moves after the draft was last written', () => {
    const since = movesSince(held, '2026-10-10T12:00:00Z')
    expect(since.map(update => update.to_sha)).toEqual(['1b7c4e0'])
  })

  it('leaves out the pushes that only brought the base branch in', () => {
    // Neither changed the pull request's own diff, so neither is something the draft is
    // out of date about.
    expect(movesSince(held, null).every(update => update.kind === 'moved')).toBe(true)
    expect(movesSince(held, null)).toHaveLength(2)
  })

  it('takes every move when there is no draft to date from', () => {
    expect(movesSince(held, null)).toHaveLength(2)
  })

  it('keeps the move that withdrew the sign-off, which is written in the same second', () => {
    /*
     * **The case the section exists for.** Withdrawing a sign-off writes the draft in the
     * same transaction as the move, and the record keeps whole seconds — so the draft's
     * `updated_at` equals the move's time, and a strict comparison left the page silent
     * about the one push that took a developer's sign-off away.
     */
    const withdrawn = updates({
      moved: 1,
      since_sha: '4d9464f',
      commits: 2,
      updates: [move('4d9464f', 'a41c9e2', '2026-10-11T08:00:00Z')],
    })
    expect(movesSince(withdrawn, '2026-10-11T08:00:00Z')).toHaveLength(1)
  })
})

describe('why a sign-off went', () => {
  const review = { head_sha: 'a41c9e2' } as Review

  it('says which commit was approved and where the pull request is now', () => {
    const draft = {
      signoff_withdrawn_at: '2026-10-11T08:00:00Z',
      signoff_withdrawn_head: '4d9464f',
    } as Draft
    const line = withdrawnLine(draft, review)
    expect(line).toContain('4d9464f')
    expect(line).toContain('a41c9e2')
    expect(line).toContain('withdrawn')
  })

  it('says nothing about a draft nobody signed off', () => {
    const draft = { signoff_withdrawn_at: null, signoff_withdrawn_head: null } as Draft
    expect(withdrawnLine(draft, review)).toBeNull()
  })
})

/* ================== Where the pull request's other rounds are ================= */

/** A review out of the fixtures, with the fields a case wants replaced. */
function detail(id: number, over: Partial<ReviewDetail> = {}): ReviewDetail {
  const found = loadSource().reviews.find(one => one.review.id === id)
  if (!found) throw new Error(`the fixtures hold no review ${id}`)
  return { ...found, ...over }
}

/** One round of a pull request, with only what a case cares about spelled out. */
function round(over: Partial<RoundSummary> & { review_id: number, round: number }): RoundSummary {
  return {
    head_sha: 'aaaaaaa',
    created_at: '2026-10-08T10:31:00Z',
    posted_at: null,
    event: null,
    findings: 0,
    commits: 0,
    replies: 0,
    threads_resolved: 0,
    statuses: { new: 0, fixed: 0, still_open: 0, accepted: 0, withdrawn: 0, ticketed: 0 },
    dismissed: false,
    brief_written: false,
    ...over,
  }
}

/** Three rounds of one pull request: round 1 posted, round 2 pushed past, round 3 open. */
const THREE: RoundSummary[] = [
  round({ review_id: 701, round: 1, head_sha: '1111111aaaa', posted_at: '2026-10-09T08:40:00Z', event: 'request_changes', brief_written: true }),
  round({ review_id: 702, round: 2, head_sha: '2222222bbbb' }),
  round({ review_id: 703, round: 3, head_sha: '3333333cccc' }),
]

/**
 * What the server answers for round 2 of these three, and so what a fixture of it must say.
 *
 * `Store::not_for_signing` returns `Superseded` for an **unposted** round a newer round
 * exists beside, so a round 2 carrying `null` is a response the API cannot produce. It
 * changes no assertion below — `inPlay` is true for `superseded` as well — and it keeps one
 * standard in the file: a fixture the server could not serve is a measurement against a
 * record that cannot arrive, whichever way it is wrong. Both halves found by reviews of this
 * branch.
 */
const SUPERSEDED = { reason: 'superseded', by_round: 3 } as const

describe('which round this is of how many', () => {
  it('counts the rounds of the pull request, not the reviews on the desk', () => {
    expect(roundsLabel(detail(41, { review: { ...detail(41).review, round: 2 }, rounds: THREE })))
      .toBe('Round 2 of 3')
  })

  /*
   * **The whole switcher turns on this.** Every review is a round, so a timeline of one
   * restates the page's own header; the acceptance criterion is that a single-round review
   * shows none.
   */
  it('says nothing where the pull request has one round', () => {
    expect(roundsLabel(detail(41, { rounds: [THREE[0]!] }))).toBeNull()
  })

  /**
   * `rounds` is not in `ReviewDetail`'s required list, so a response may carry none — and
   * the fixtures in `app/data/reviews.json` carry none. "Nothing served" and "one round"
   * are the same answer here, which is what lets the component ask one question.
   */
  it('says nothing where no rounds were served at all', () => {
    expect(roundsLabel(detail(41))).toBeNull()
    expect(roundsLabel(detail(41, { rounds: [] }))).toBeNull()
  })
})

describe('the links to the other rounds', () => {
  const here = (step: Parameters<typeof roundLinks>[1] = 'alternatives') =>
    roundLinks(detail(41, {
      review: { ...detail(41).review, id: 702, round: 2 },
      rounds: THREE,
      not_for_signing: SUPERSEDED,
    }), step)

  it('is oldest first, whatever order the rounds arrived in', () => {
    const backwards = [...THREE].reverse()
    const links = roundLinks(
      detail(41, {
        review: { ...detail(41).review, id: 702, round: 2 },
        rounds: backwards,
        not_for_signing: SUPERSEDED,
      }),
      'understand',
    )
    expect(links.map(one => one.round)).toEqual([1, 2, 3])
  })

  /**
   * **The acceptance criterion: each link opens the same step of the linked round.** A
   * reader on Alternatives in round 3 wants Alternatives in round 1, not the top of round 1.
   */
  it('links each other round to the same step', () => {
    expect(here('alternatives').map(one => one.to))
      .toEqual(['/reviews/701/alternatives', null, '/reviews/703/alternatives'])
    expect(here('solution_map').map(one => one.to))
      .toEqual(['/reviews/701/solution', null, '/reviews/703/solution'])
    expect(here('code_review').map(one => one.to))
      .toEqual(['/reviews/701/code-review', null, '/reviews/703/code-review'])
    expect(here('draft').map(one => one.to))
      .toEqual(['/reviews/701/draft', null, '/reviews/703/draft'])
    expect(here('harvest').map(one => one.to))
      .toEqual(['/reviews/701/what-happened', null, '/reviews/703/what-happened'])
  })

  /** The round being read is not a link: a link to the page you are on does nothing. */
  it('leaves the round being read without a link', () => {
    const [one, two, three] = here()
    expect(two!.here).toBe(true)
    expect(two!.to).toBeNull()
    expect([one!.here, three!.here]).toEqual([false, false])
  })

  /**
   * Step 1 has no page — the trail links six of the seven — so it has nowhere to send
   * anybody. Links to somewhere else would be worse than none.
   */
  it('answers no links for the one step that has no page', () => {
    expect(here('lint')).toEqual([])
  })

  it('shortens every head commit to seven characters', () => {
    expect(here().map(one => one.head)).toEqual(['1111111', '2222222', '3333333'])
  })

  describe('what it says about where each round stands', () => {
    it('gives a posted round the day it was posted', () => {
      expect(here()[0]!.standing).toBe('posted 9 October 2026')
    })

    /**
     * **A posted round the day of which cannot be read still says it was posted.** `dayLabel`
     * answers null for an unparseable instant exactly as it does for an absent one, so asking
     * it first let a posted round fall through to "being worked on" — a posted round described
     * as in hand. The server cannot send it; a hand-written fixture can, and `RoundTimeline`
     * guards the same case this way.
     */
    it('still says a round was posted when the day cannot be read', () => {
      const unreadable = [{ ...THREE[0]!, posted_at: 'the ninth of October' }, THREE[1]!, THREE[2]!]
      const links = roundLinks(
        detail(41, {
          review: { ...detail(41).review, id: 702, round: 2 },
          rounds: unreadable,
          not_for_signing: SUPERSEDED,
        }),
        'understand',
      )
      expect(links[0]!.standing).toBe('posted')
    })

    /**
     * **The newest unposted round is the one being worked on, and only that one.** An older
     * unposted round is one the author pushed past: nobody can post it, which is DEV-957's
     * whole subject, so "being worked on" would promise something never coming.
     */
    it('calls the newest unposted round the one being worked on', () => {
      expect(here()[2]!.standing).toBe('being worked on')
    })

    it('calls an unposted round a newer round supersedes not posted', () => {
      expect(here()[1]!.standing).toBe('not posted')
    })

    /**
     * **Nothing is being worked on once the pull request is over**, which is the same
     * correction `StepTrail` makes to a step's tense. DEV-957's record is exactly this
     * shape: a pull request merged with unposted rounds behind it, and three of them sat
     * in "drafts to approve" for a week.
     */
    it('says nothing is being worked on once the pull request is over', () => {
      const over = detail(41, {
        review: { ...detail(41).review, id: 702, round: 2 },
        rounds: THREE,
        not_for_signing: { reason: 'finished', how: 'merged', at: '2026-10-11T22:41:00Z' },
      })
      expect(roundLinks(over, 'understand').map(one => one.standing))
        .toEqual(['posted 9 October 2026', 'not posted', 'not posted'])
    })

    /**
     * **A superseded round still says it, because `superseded` is positive evidence.** The
     * three reasons are asked in a fixed order — dismissed, then finished, then superseded —
     * so reaching the third means the first two were asked and refused: the pull request has
     * not merged, closed or been dismissed. This is the common case the whole feature is
     * for, a reader on an earlier round of a live pull request.
     */
    it('says so from an earlier round of a pull request still in play', () => {
      /*
       * **Round 2, not round 1, and that is a reachable record rather than a tidier one.**
       * `Store::not_for_signing` answers `Superseded` only when `by_round > review.round`
       * *and the round was never posted*, and round 1 of this fixture is posted — so a
       * posted round carrying that reason is a response the API cannot produce. The same
       * defect as a fixture short of a required field, in the other direction; named by a
       * review of this branch.
       */
      const earlier = detail(41, {
        review: { ...detail(41).review, id: 702, round: 2 },
        rounds: THREE,
        not_for_signing: SUPERSEDED,
      })
      expect(roundLinks(earlier, 'understand')[2]!.standing).toBe('being worked on')
    })

    /**
     * **And a dismissed round is not in hand either.** This is the half of the problem that
     * per-round dismissal solves, and the reader it protects is the one on *another* round,
     * whose page would otherwise promise that a round nobody will touch again is in hand.
     */
    it('promises nothing about a round the developer dismissed', () => {
      const dismissed = { ...THREE[2]!, dismissed: true }
      const live = detail(41, {
        review: { ...detail(41).review, id: 702, round: 2 },
        rounds: [THREE[0]!, THREE[1]!, dismissed],
        not_for_signing: SUPERSEDED,
      })
      expect(roundLinks(live, 'understand')[2]!.standing).toBe('not posted')
    })

    /**
     * **And a dismissed round promises nothing, because `dismissed` is asked first.** A
     * dismissed round of a *merged* pull request answers `dismissed` and never reaches the
     * state check, so reading `finished` alone had this page say the newest unposted round of
     * a merged pull request was in hand — the exact false promise the rule exists to stop.
     * Found by a review of this branch. Unknown reads as not in play: a fact lost rather than
     * a promise broken.
     */
    it('promises nothing from a dismissed round, which may be of a merged one', () => {
      const dismissed = detail(41, {
        review: { ...detail(41).review, id: 701, round: 1 },
        rounds: THREE,
        not_for_signing: { reason: 'dismissed' },
      })
      expect(roundLinks(dismissed, 'understand').map(one => one.standing))
        .toEqual(['posted 9 October 2026', 'not posted', 'not posted'])
    })
  })

  /*
   * **`RoundSummary.dismissed`, read straight.** It was reached through a seam while DEV-962
   * was in flight and was three-valued, because the absence of a flag is not the absence of
   * a dismissal. The field is required now, so there is no third answer left to hold open
   * and no cast between the wire and the chip.
   */
  describe('a dismissed round', () => {
    it('is marked, and keeps its link', () => {
      const dismissed = { ...THREE[0]!, dismissed: true }
      const links = roundLinks(
        detail(41, {
          review: { ...detail(41).review, id: 702, round: 2 },
          rounds: [dismissed, THREE[1]!, THREE[2]!],
          not_for_signing: SUPERSEDED,
        }),
        'understand',
      )
      expect(links[0]!.dismissed).toBe(true)
      /*
       * **Still a link.** A dismissed round's pages are all still there and everything on it
       * was kept, so what the word changes is what a reader expects of the round, not where
       * the chip goes.
       */
      expect(links[0]!.to).toBe('/reviews/701/understand')
    })

    it('is not marked where the round was not dismissed', () => {
      expect(here().map(one => one.dismissed)).toEqual([false, false, false])
    })
  })
})

describe('where a step was written, when it was not written here', () => {
  /** This round holds a brief, so it wrote its own and there is nothing to point at. */
  it('says nothing where this round wrote a brief of its own', () => {
    const own = detail(41, { review: { ...detail(41).review, id: 702, round: 2 }, rounds: THREE })
    expect(own.brief, 'review 41 is the real-length brief').not.toBeNull()
    expect(briefWrittenEarlier(own, 'understand')).toBeNull()
  })

  /**
   * **The acceptance criterion: a step carried unchanged says so and links to the round that
   * wrote it.** Nothing carries a brief between rounds — `briefs` is keyed by the review id
   * and opening a round copies none of it — so the state the record actually has is "this
   * round wrote none and an earlier round did".
   */
  it('names the earlier round that wrote one, and the same step of it', () => {
    const later = detail(43, { review: { ...detail(43).review, id: 703, round: 3 }, rounds: THREE })
    expect(later.brief, 'review 43 has no brief').toBeNull()
    expect(briefWrittenEarlier(later, 'understand')).toMatchObject(
      { round: 1, to: '/reviews/701/understand' },
    )
    expect(briefWrittenEarlier(later, 'alternatives')).toMatchObject(
      { round: 1, to: '/reviews/701/alternatives' },
    )
    expect(briefWrittenEarlier(later, 'solution_map')).toMatchObject(
      { round: 1, to: '/reviews/701/solution' },
    )
  })

  /**
   * **The latest earlier round that wrote one, which is why the field exists.** Guessing the
   * first round is wrong the moment a round after the first rewrites the brief — and it is
   * also what makes "and no round since has written another" true in the sentence the page
   * shows: a round in between that had written one would have been named instead.
   */
  it('names the latest earlier round, not the first', () => {
    const rewritten = [
      THREE[0]!,
      { ...THREE[1]!, brief_written: true },
      THREE[2]!,
    ]
    const later = detail(43, { review: { ...detail(43).review, id: 703, round: 3 }, rounds: rewritten })
    expect(briefWrittenEarlier(later, 'understand')).toMatchObject(
      { round: 2, to: '/reviews/702/understand' },
    )
  })

  it('says nothing where no earlier round wrote one either', () => {
    const none = THREE.map(one => ({ ...one, brief_written: false }))
    const later = detail(43, { review: { ...detail(43).review, id: 703, round: 3 }, rounds: none })
    expect(briefWrittenEarlier(later, 'understand')).toBeNull()
  })

  /**
   * **The page's own heading, which is the answer rather than an absence.** It replaces
   * "Nothing has been written about this problem yet" — false over a pull request whose
   * brief is one round back, and exactly the ambiguity DEV-976 was raised to remove. The
   * notice under it then carries only the link, because DEV-892's sixth defect was a page
   * saying one sentence twice.
   */
  it('words the heading and the line under it for the page to use', () => {
    const later = detail(43, { review: { ...detail(43).review, id: 703, round: 3 }, rounds: THREE })
    expect(briefWrittenEarlier(later, 'understand')?.headline)
      .toBe('The last brief before this round was written in round 1.')
    expect(briefWrittenEarlier(later, 'alternatives')?.sub)
      .toBe('Round 3 wrote none of its own, and nothing carries a brief from one round to '
        + 'the next.')
  })

  /**
   * **"Before this round", because only earlier rounds were searched.** A third review of
   * this branch found the first wording — "the last brief on this pull request" — claiming
   * more than the filter looked at: round 1 writes one, round 2's session writes none, round
   * 3 writes one, and a reader who opens round 2 was told the newest brief was behind them
   * when it was ahead.
   *
   * **And the line under it says nothing about the page.** It read "so this page draws
   * nothing", which is false on Understand: its no-brief branch draws `RemoveReview`, whose
   * card is unconditional.
   */
  it('claims only what it searched, and nothing about the page', () => {
    const rewritten = [THREE[0]!, THREE[1]!, { ...THREE[2]!, brief_written: true }]
    const middle = detail(43, {
      review: { ...detail(43).review, id: 702, round: 2 },
      rounds: rewritten,
      not_for_signing: SUPERSEDED,
    })
    const said = briefWrittenEarlier(middle, 'understand')!
    expect(said.round, 'the latest earlier round, with round 3 ahead of it').toBe(1)
    expect(said.headline).toBe('The last brief before this round was written in round 1.')
    expect(said.headline, 'and not a claim about the whole pull request')
      .not.toContain('on this pull request')
    expect(said.sub, 'nor about what the page draws').not.toContain('page')
  })

  /**
   * **And it claims a brief, never a step.** `brief_written` says a `briefs` row exists for
   * that round and nothing more. Worded as "Round 2 wrote this pull request's alternatives"
   * it was falsified by the fixtures themselves — review 65 is round 2 of
   * `globex/portal#9568` and its brief holds `options: []` — so round 3's Alternatives page
   * claimed options that the page it linked to reads "None was recorded" for. Found by a
   * review of this branch.
   */
  it('says the same thing on all three pages, because it is one record', () => {
    const later = detail(43, { review: { ...detail(43).review, id: 703, round: 3 }, rounds: THREE })
    const said = (['understand', 'alternatives', 'solution_map'] as const)
      .map(step => briefWrittenEarlier(later, step)!)
    expect(new Set(said.map(one => one.headline)).size, 'one heading, about the brief').toBe(1)
    expect(new Set(said.map(one => one.sub)).size, 'one line under it').toBe(1)
    for (const one of said) {
      expect(one.headline, 'no step is named as something a round wrote')
        .not.toMatch(/alternatives|solution|understand/i)
    }
  })

  it('says nothing on the first round, which has nothing before it', () => {
    const first = detail(43, { review: { ...detail(43).review, id: 701, round: 1 }, rounds: THREE })
    expect(briefWrittenEarlier(first, 'understand')).toBeNull()
  })

  /**
   * **Steps 5 to 7 are never said to have been written in another round, and that is the
   * point of the allowlist.** A round's findings, its draft and its harvest are its own,
   * written against its own head commit; "written in round 1" would be false about the one
   * thing rounds exist to separate.
   */
  it('never says a later step was written in another round', () => {
    const later = detail(43, { review: { ...detail(43).review, id: 703, round: 3 }, rounds: THREE })
    for (const step of ['code_review', 'draft', 'harvest', 'lint'] as const) {
      expect(briefWrittenEarlier(later, step), `step ${step}`).toBeNull()
    }
  })
})

describe('the rounds the fixtures already hold', () => {
  /*
   * **Read through the sample backend's own derivation**, not through a literal: the
   * fixtures hold every round of `globex/portal#9568` as a review of its own, and
   * `roundsOf` is what turns that into the array the wire carries. A test that built the
   * array here would be testing a copy.
   */
  const rounds = (pr: number): RoundSummary[] =>
    loadSource().reviews
      .filter(one => one.review.pr === pr)
      .sort((one, two) => one.review.round - two.review.round)
      .map(one => round({
        review_id: one.review.id,
        round: one.review.round,
        head_sha: one.review.head_sha,
        brief_written: one.brief != null,
      }))

  /**
   * The case that justifies the contract field, and it is in the fixtures already: round 1
   * of `#9568` wrote no brief and round 2 did, so round 3 points at round 2.
   */
  it('points round 3 of the four-round pull request at round 2', () => {
    const four = rounds(9568)
    expect(four.map(one => one.brief_written)).toEqual([false, true, false, false])
    const third = detail(66, { rounds: four })
    expect(briefWrittenEarlier(third, 'understand')?.round).toBe(2)
  })

  it('points round 2 of the two-round pull request at round 1', () => {
    const two = rounds(9541)
    expect(two.map(one => one.brief_written)).toEqual([true, false])
    expect(briefWrittenEarlier(detail(61, { rounds: two }), 'understand')?.round).toBe(1)
  })

  it('points nowhere where no round of the pull request wrote one', () => {
    const two = rounds(9585)
    expect(two.map(one => one.brief_written)).toEqual([false, false])
    expect(briefWrittenEarlier(detail(63, { rounds: two }), 'understand')).toBeNull()
  })
})
