import { beforeEach, describe, expect, it } from 'vitest'
import { computed, effectScope, ref } from 'vue'

import {
  ALL_REPOSITORIES,
  resetDesk,
  useReviewDesk,
  type DeskResource,
} from '../app/composables/useReviewDesk'
import { loadSource } from '../app/data/sample'
import { composeBody, composeThreads } from '../app/utils/houseStyle'

/*
 * The data layer, which is where every write on the five screens ends up.
 *
 * It runs here over sample records — DEV-862 keeps them for exactly this, so that the
 * behaviour of a screen can be checked without a binary and a seeded database. Sample mode
 * raises the same refusals, with the same contract `code`s, that the server raises, so what
 * is asserted here is also the behaviour expected of the Rust server.
 *
 * Two things are *not* checked here, and both have their own file. `test/api.spec.ts`
 * covers what the dashboard makes of each error code and of a server that is not answering,
 * because sample mode is always answering. `web/e2e/` covers the whole path through a real
 * binary in a real browser.
 *
 * Reads are asynchronous now, so every one is awaited through `settled`. That is the one
 * shape change from the fixture era: a read has an in-flight state, and a test that read
 * `.data.value` without waiting would be asserting on `null`.
 */

const PORTAL = 'acme/portal'
const WEBHOOKS = 17 /* #412, the brief that is waiting */
const INVOICES = 15 /* #407, a draft that is waiting */
const CACHE = 16 /* #88, round 2 */
const EXPORT = 12 /* #398, posted, with author replies */
const DRAFTING = 18 /* a review a session is still writing */

/** Wait for a resource's first read, and hand back what it answered. */
async function settled<T>(resource: DeskResource<T>): Promise<T> {
  await resource.refresh()
  if (resource.error.value) {
    throw new Error(`the read failed: ${resource.error.value.message}`)
  }
  return resource.data.value as T
}

/**
 * A resource's refusal rather than its answer, for a read that is meant to fail.
 *
 * `settled` throws on a failed read, which is right everywhere else: a test that meant to
 * read something and got an error should say so rather than assert on `null`.
 */
async function refused<T>(resource: DeskResource<T>) {
  await resource.refresh()
  if (!resource.error.value) throw new Error('that read was expected to fail and did not')
  return resource.error.value
}

/*
 * Resources register themselves for refresh-after-write while their scope is alive, and a
 * test has no component scope. One scope per file keeps them from accumulating across
 * hundreds of tests, which would turn every write into hundreds of reads.
 */
const scope = effectScope()
function desk() {
  return scope.run(() => useReviewDesk())!
}

beforeEach(async () => {
  await resetDesk()
})

describe('the inbox', () => {
  it('groups twelve waiting items across three repositories', async () => {
    const inbox = await settled(desk().inbox())
    /*
     * **`items` counts items, not rows, and that has not changed.** One review can be two
     * of them: #412 has a brief to confirm *and* a draft to approve, because its session
     * wrote the draft without waiting for the confirmation. Both are waiting on the
     * developer, so both are items — which is what the server counts, and what this
     * counted differently until DEV-862's end-to-end test caught it. The queue's *heading*
     * counts pull requests and reads the merged rows instead; see
     * `screens.nuxt.spec.ts`.
     *
     * It was eight until DEV-878 gave the real-length review a draft, and nine until
     * DEV-957 added the fixtures in the shape of Patrick's own queue: `globex/portal#9541`
     * round 2's draft, `#9585` round 2's draft, and `#9585` round 1's author reply. Three
     * more items, on a third repository. The four rounds of merged `#9568` add none — a
     * finished review is in no group, which is that issue.
     */
    expect(inbox.totals).toEqual({ items: 12, repos: 3 })
  })

  it('leaves every round of a merged pull request out of every group', async () => {
    /*
     * **The record this is built from is Patrick's own**, from DEV-957: four rounds of
     * `globex/portal#9568`, one posted and three not, on a pull request that merged on 11
     * October. Every one of those three drafts was in "drafts to approve" and none of them
     * could ever be posted.
     *
     * Note which round the fixture harvested: **round 1 only**, because that is the round
     * that was posted and the harvest reads what it was posted on. Rounds 2, 3 and 4 hold
     * no `pr_state` of their own at all, so this is the derivation being asserted rather
     * than a column being read.
     */
    const inbox = await settled(desk().inbox())
    const merged = [64, 65, 66, 67]

    for (const group of [
      inbox.briefs_to_confirm,
      inbox.drafts_to_approve,
      inbox.author_replies,
      inbox.in_progress,
      inbox.posted_awaiting_author,
    ]) {
      const found = group
        .map(item => item.review_id)
        .filter(id => merged.includes(id))
      expect(found, 'a round of a merged pull request is in no group').toEqual([])
    }
  })

  it('leaves an unposted round a newer round supersedes out of the queue', async () => {
    /*
     * `globex/portal#9541` drew two rows on Patrick's screen, round 1 and round 2, both "Draft
     * ready". Round 1 can never be the review that posts — the author pushed past the
     * commit it was written against — so it is in no group, and round 2 is.
     */
    const inbox = await settled(desk().inbox())
    const drafts = inbox.drafts_to_approve.map(item => item.review_id)

    expect(drafts, 'round 1 of #9541 is superseded').not.toContain(60)
    expect(drafts, 'and round 2 is the one waiting').toContain(61)
  })

  it('has all five groups filled', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.briefs_to_confirm).toHaveLength(1)
    /* Four, plus round 2 of each of `globex/portal#9541` and `#9585` — see DEV-957's shape. */
    expect(inbox.drafts_to_approve).toHaveLength(6)
    /* The one on review 12, plus `#9585` round 1's, which its author answered. */
    expect(inbox.author_replies).toHaveLength(2)
    expect(inbox.own_prs_without_reviewer).toHaveLength(2)
    expect(inbox.proposed_rules).toHaveLength(1)
  })

  it('shows every session running, one of them with unknown liveness', async () => {
    const inbox = await settled(desk().inbox())
    /*
     * **Three, and the third is a posted review a session has gone back to.** This
     * asserted two until DEV-930: sample mode carried a `draft_status === 'drafting'`
     * filter that `Store::reviews_in_progress` has not got, so the one state the queue's
     * cross-group deduplication exists for — a review in `author_replies` and
     * `in_progress` at once — could not be expressed here at all. The queue draws review
     * 12 once; this is the array it has to do that from.
     *
     * **It was the harvest step that put review 12 here, and since DEV-933 it cannot
     * be.** A harvest sets step 7 `running` for as long as the pull request stays open,
     * so neither the server nor this mode counts that step as a session at work. What
     * puts review 12 here now is its code review step, which a session went back to when
     * the author replied — the next thing that happens in this loop, and a shape the
     * server can serve.
     */
    expect(inbox.in_progress.map(item => item.review_id)).toEqual([12, 18, 19])
    expect(inbox.in_progress.map(item => item.session_alive)).toEqual([null, true, null])
    const paginating = inbox.in_progress.find(item => item.review_id === 18)!
    expect(paginating).toMatchObject({ layer_number: 5, layers_total: 7 })
  })

  it('says a draft already written is not holding its session at layer 2', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.briefs_to_confirm[0]).toMatchObject({
      review_id: WEBHOOKS,
      blocks_later_layers: false,
    })
  })

  it('carries every kind of act the author and the harvester recorded', async () => {
    const inbox = await settled(desk().inbox())
    const kinds = inbox.author_replies[0]!.events.map(event => event.kind)
    /*
     * **Five, in the order the record holds them.** Two `replied` and a `thread_resolved`
     * were added with DEV-930, because the posted group's figures are counts of exactly
     * these and a fixture with none could not show a measurement. The `rejected` is the
     * developer accepting the author's reason, which is a different act and is why the
     * group counts by kind *and* actor.
     */
    expect(kinds).toEqual([
      'replied',
      'rejected',
      'replied',
      'thread_resolved',
      'code_changed_after',
    ])
  })

  /*
   * **The group that did not exist, and the two absences it has to tell apart.** DEV-930:
   * a posted review left every group this screen serves, so nothing linked to it.
   */
  it('serves a posted review whose pull request is open, saying what came back', async () => {
    const inbox = await settled(desk().inbox())
    const posted = inbox.posted_awaiting_author
    expect(posted.map(item => item.review_id).sort()).toEqual([12, 43])

    /* Harvested, with the author's own acts counted: two replies and one resolution. */
    const answered = posted.find(item => item.review_id === 12)!
    expect(answered.posted_as).toBe('request_changes')
    expect(answered.pr_state).toBe('open')
    expect(answered.replies).toBe(2)
    expect(answered.threads_resolved).toBe(1)
    expect(answered.code_changed_after).toBe(1)
    expect(answered.findings_linked).toBe(2)

    /*
     * And one nothing has harvested. **Every figure is null, not nought**: zero replies on
     * a review nobody has read back is the absence of a measurement rather than one.
     */
    const unlooked = posted.find(item => item.review_id === 43)!
    expect(unlooked.harvested_at).toBeNull()
    expect(unlooked.pr_state).toBeNull()
    expect(unlooked.replies).toBeNull()
    expect(unlooked.threads_resolved).toBeNull()
    expect(unlooked.code_changed_after).toBeNull()
  })

  /*
   * `totals.items` is the five waiting groups. Neither the reviews with the agent nor the
   * posted reviews are in it: nothing in either asks the developer for anything, and the
   * screen's headline counts what does.
   */
  it('leaves the posted reviews out of the count of what is waiting on you', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.posted_awaiting_author.length).toBeGreaterThan(0)
    expect(inbox.totals.items).toBe(
      inbox.briefs_to_confirm.length
      + inbox.drafts_to_approve.length
      + inbox.author_replies.length
      + inbox.own_prs_without_reviewer.length
      + inbox.proposed_rules.length,
    )
  })

  /* ---- a code change a later posted round has already reviewed (DEV-983) ---- */

  /*
   * **`globex/portal#9585`, with round 2 posted.** Patrick's queue on 14 October 2026 drew
   * a two-round pull request as "round 1 · The code under F1 changed · See what changed"
   * and opened round 1, though round 2 had been posted the day before — and the change it
   * reported was round 2's own head.
   *
   * Round 1 is review 62 at `c3d4e53` and round 2 is review 63 at `d4e5f64`, which the
   * fixtures already hold. What these set is what came back on round 1 and whether round 2
   * posted: round 63's draft is `ready` in the fixtures, so posting it is also what takes
   * it out of "drafts to approve" and leaves the pull request waiting on its author.
   */
  const ROUND_ONE = 62
  const ROUND_TWO = 63
  const ROUND_ONE_HEAD = 'c3d4e53'
  const ROUND_TWO_HEAD = 'd4e5f64'
  /** A commit after round 2's head, which no round of the fixture records. */
  const LATER_HEAD = 'e5f6a75'

  /** The detail the harvester writes for a code change, at this commit and comparison. */
  const changeDetail = (commit: string, from: string, to: string) => JSON.stringify({
    file: 'app/Providers/FortifyServiceProvider.php',
    line: 72,
    lines_changed: false,
    file_status: 'modified',
    commit,
    basis: `compare ${from}...${to}`,
  })

  const codeChanged = (review: number, id: number, detail: string) => ({
    id,
    review_id: review,
    finding_seq: 1,
    finding_key: 'F1' as const,
    kind: 'code_changed_after' as const,
    actor: 'harvester' as const,
    occurred_at: '2026-10-13T06:00:00Z',
    detail,
  })

  /**
   * The desk with round 2 posted, and these outcomes on each round.
   *
   * `keepRoundOneReply` is the third fixture: the author answering a round-1 thread after
   * round 2 posts, which is a thing a person did and is the developer's to read whatever
   * round it was left on.
   */
  async function twoRounds(options: {
    onRoundOne?: ReturnType<typeof codeChanged>[]
    onRoundTwo?: ReturnType<typeof codeChanged>[]
    keepRoundOneReply?: boolean
  }) {
    const source = loadSource()
    await resetDesk({
      reviews: source.reviews.map((detail) => {
        if (detail.review.id === ROUND_ONE) {
          return {
            ...detail,
            outcomes: [
              ...(options.keepRoundOneReply ? detail.outcomes : []),
              ...(options.onRoundOne ?? []),
            ],
          }
        }
        if (detail.review.id === ROUND_TWO) {
          return {
            ...detail,
            /* Posted, which is what makes it a round that reviewed its own head. */
            review: { ...detail.review, github_review_id: 'PRR_globex9585r2' },
            outcomes: options.onRoundTwo ?? [],
          }
        }
        return detail
      }) as typeof source.reviews,
      /* And posted as a draft too, so it leaves "drafts to approve". */
      drafts: source.drafts.map(detail =>
        detail.draft.review_id === ROUND_TWO
          ? {
              ...detail,
              draft: {
                ...detail.draft,
                status: 'posted' as const,
                event: 'comment' as const,
                posted_at: '2026-10-13T05:00:00Z',
              },
            }
          : detail) as typeof source.drafts,
    })
  }

  it('leaves out a code change the later posted round read, and waits on the author', async () => {
    await twoRounds({
      onRoundOne: [codeChanged(
        ROUND_ONE,
        9200,
        changeDetail(ROUND_TWO_HEAD, ROUND_ONE_HEAD, ROUND_TWO_HEAD),
      )],
    })

    const inbox = await settled(desk().inbox())
    expect(
      inbox.author_replies.map(item => item.review_id),
      'round 1 has nothing left that is waiting on anybody',
    ).not.toContain(ROUND_ONE)
    expect(inbox.posted_awaiting_author.map(item => item.review_id)).toContain(ROUND_TWO)
    expect(
      inbox.posted_awaiting_author.find(item => item.review_id === ROUND_TWO)!.round,
      'the item the row takes its number from is round 2',
    ).toBe(2)
  })

  it('counts a change after the posted round’s head, on the newest round', async () => {
    await twoRounds({
      onRoundOne: [codeChanged(
        ROUND_ONE,
        9200,
        changeDetail(LATER_HEAD, ROUND_ONE_HEAD, LATER_HEAD),
      )],
      onRoundTwo: [codeChanged(
        ROUND_TWO,
        9201,
        changeDetail(LATER_HEAD, ROUND_TWO_HEAD, LATER_HEAD),
      )],
    })

    const inbox = await settled(desk().inbox())
    const newest = inbox.author_replies.find(item => item.review_id === ROUND_TWO)
    expect(newest, 'the newest round carries the change nothing has reviewed').toBeDefined()
    expect(newest!.events.map(event => event.kind)).toEqual(['code_changed_after'])
    expect(newest!.round).toBe(2)
  })

  it('keeps an author’s reply on an earlier round after a later round posts', async () => {
    await twoRounds({
      keepRoundOneReply: true,
      onRoundOne: [codeChanged(
        ROUND_ONE,
        9200,
        changeDetail(ROUND_TWO_HEAD, ROUND_ONE_HEAD, ROUND_TWO_HEAD),
      )],
    })

    const inbox = await settled(desk().inbox())
    const reply = inbox.author_replies.find(item => item.review_id === ROUND_ONE)
    expect(reply, 'the reply is still the developer’s to read').toBeDefined()
    expect(
      reply!.events.map(event => event.kind),
      'the reply survives and the already-reviewed change does not',
    ).toEqual(['replied'])
    expect(reply!.round, 'and it says which round it was left on').toBe(1)
  })

  it('does not hide a change when no round of the record names that commit', async () => {
    await twoRounds({
      onRoundOne: [codeChanged(
        ROUND_ONE,
        9200,
        /* Neither commit is a round's head, and ancestry is in no record here. */
        changeDetail('9a8b7c6', ROUND_ONE_HEAD, '7f6e5d4'),
      )],
    })

    const inbox = await settled(desk().inbox())
    expect(
      inbox.author_replies.map(item => item.review_id),
      'an unrecognised commit is shown, not hidden',
    ).toContain(ROUND_ONE)
  })

  it('does not hide a change whose detail is not JSON', async () => {
    await twoRounds({
      onRoundOne: [codeChanged(ROUND_ONE, 9200, 'the author pushed a fix')],
    })

    const inbox = await settled(desk().inbox())
    expect(inbox.author_replies.map(item => item.review_id)).toContain(ROUND_ONE)
  })
})

describe('the reviews list', () => {
  it('names every review, newest first, with the step it reached', async () => {
    const answer = await settled(desk().reviews(ref(ALL_REPOSITORIES)))
    expect(answer.repo).toBeNull()
    /*
     * Every review in play, including the two that were posted — and **not** the four
     * rounds of merged `globex/portal#9568`, which are under Finished. This is the default
     * view, and DEV-957 is what the four absences are.
     */
    expect(answer.reviews.map(row => row.review.id))
      .toEqual([63, 62, 61, 60, 43, 41, 19, 18, 17, 16, 15, 12])

    /*
     * **A posted review nothing has harvested reports step 6, not step 7.** Posting
     * writes the harvest step as `pending`, so the furthest *started* step is the draft
     * until a harvest reads the review and writes step 7 — which is DEV-933, and review
     * 43 has not been read. The row carries the state and the note either way, because
     * "Draft" alone would read as a sign-off still owed.
     */
    const posted = answer.reviews.find(row => row.review.id === 43)!
    expect(posted.step_number).toBe(6)
    expect(posted.step_state).toBe('done')
    expect(posted.step_detail).toBe('posted by you')
    expect(posted.steps_total).toBe(7)
  })

  it('answers one repository when the picker names one', async () => {
    const answer = await settled(desk().reviews(ref('acme/billing-api')))
    expect(answer.repo).toBe('acme/billing-api')
    expect(answer.reviews.every(row => row.review.repo === 'acme/billing-api')).toBe(true)
    expect(answer.reviews.length).toBeGreaterThan(0)
  })
})

describe('confirming a brief', () => {
  it('takes the brief out of the inbox and leaves the draft', async () => {
    const api = desk()
    const inbox = api.inbox()
    const before = await settled(inbox)
    expect(before.briefs_to_confirm.map(item => item.review_id)).toContain(WEBHOOKS)
    /*
     * The draft is already there: its session wrote it without waiting. Confirming the
     * brief takes one item away and leaves the other, which is what "nothing posts until
     * you confirm" means — the draft can be looked at, and approving it is a second act.
     */
    expect(before.drafts_to_approve.map(item => item.review_id)).toContain(WEBHOOKS)
    const items = before.totals.items

    const result = await api.confirmBrief(WEBHOOKS)
    expect(result.ok).toBe(true)

    const after = await settled(inbox)
    expect(after.briefs_to_confirm).toHaveLength(0)
    expect(after.drafts_to_approve.map(item => item.review_id)).toContain(WEBHOOKS)
    /*
     * **One fewer, taken from whatever it was.** The figure itself is asserted by the
     * totals test above; what this one is about is that confirming a brief removes exactly
     * the brief's item, so it reads the before and the after rather than two literals that
     * both have to be edited whenever a fixture is added.
     */
    expect(after.totals.items).toBe(items - 1)
  })

  it('sets the confirmation and leaves the layer rows alone', async () => {
    const api = desk()
    const review = api.review(WEBHOOKS)
    const before = await settled(review)
    expect(before.layers[1]).toMatchObject({ state: 'waiting_on_you' })

    await api.confirmBrief(WEBHOOKS)

    const after = await settled(review)
    expect(after.brief!.confirmed).toBe(true)
    expect(after.brief!.confirmed_at).not.toBeNull()

    /*
     * **The layer row does not move, and this asserts that rather than wishing otherwise.**
     * A review's layers are written by the layer that ran, through `review-desk layer set`;
     * `confirmBrief` on the server writes the confirmation and nothing else. This used to
     * move layer 2 to `done`, which made sample mode behave better than the thing it stands
     * in for — so the screen looked right here and showed "2 Understand · waiting on you"
     * under a confirmed brief against a real database.
     *
     * Reported rather than fixed: what writes a layer row is beyond DEV-862.
     */
    expect(after.layers[1]).toMatchObject({ state: 'waiting_on_you' })
  })

  it('is idempotent, as the contract requires', async () => {
    const api = desk()
    await api.confirmBrief(WEBHOOKS)
    const at = (await settled(api.review(WEBHOOKS))).brief!.confirmed_at

    /* Confirming an already confirmed brief answers with the brief rather than failing. */
    const again = await api.confirmBrief(WEBHOOKS)
    expect(again.ok).toBe(true)
    expect((await settled(api.review(WEBHOOKS))).brief!.confirmed_at).toBe(at)
  })

  it('says so when there is no such review', async () => {
    const result = await desk().confirmBrief(999)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('not_found')
  })
})

describe('moving a finding', () => {
  it('refuses to hold one back without a reason, and says what it wanted', async () => {
    const api = desk()
    const result = await api.setFindingDisposition(WEBHOOKS, 1, 'held_back', '   ')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('invalid_value')
    expect(result.error.field).toBe('reason')
    expect(result.error.expected).toBe('a sentence saying why')
    expect((await settled(api.review(WEBHOOKS))).findings[0]!.disposition).toBe('in_draft')
  })

  it('records the reason and recounts the dispositions', async () => {
    const api = desk()
    const review = api.review(WEBHOOKS)
    expect((await settled(review)).finding_dispositions).toEqual({
      in_draft: 3,
      held_back: 3,
      suppressed: 1,
    })

    const result = await api.setFindingDisposition(WEBHOOKS, 2, 'suppressed', 'Covered by F1')
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toBe('F2 is now suppressed.')

    const after = await settled(review)
    expect(after.findings[1]).toMatchObject({
      disposition: 'suppressed',
      disposition_reason: 'Covered by F1',
    })
    expect(after.finding_dispositions).toEqual({ in_draft: 2, held_back: 3, suppressed: 2 })
  })

  it('clears the reason when a finding goes back in the draft', async () => {
    const api = desk()
    await api.setFindingDisposition(WEBHOOKS, 4, 'in_draft', '')
    expect((await settled(api.review(WEBHOOKS))).findings[3]).toMatchObject({
      disposition: 'in_draft',
      disposition_reason: null,
    })
  })

  it('updates the draft’s held-back list and its lint', async () => {
    const api = desk()
    const draft = api.draft(WEBHOOKS)
    const before = await settled(draft)
    expect(before.held_back.map(item => item.finding_key)).toEqual(['F4', 'F5', 'F7'])
    expect(before.lint!.findings_expected).toBe(3)

    await api.setFindingDisposition(WEBHOOKS, 4, 'in_draft', '')

    const after = await settled(draft)
    expect(after.held_back.map(item => item.finding_key)).toEqual(['F5', 'F7'])
    /* F4 is now expected in the body and is not there, so the lint fails loudly. */
    expect(after.lint!.findings_expected).toBe(4)
    expect(after.lint!.findings_in_body).toBe(3)
    expect(after.lint!.passes).toBe(false)
  })

  it('says so when there is no such finding', async () => {
    const result = await desk().setFindingDisposition(WEBHOOKS, 99, 'in_draft', '')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('not_found')
  })
})

describe('the draft', () => {
  it('serves a body with a mermaid fence and a folded section', async () => {
    const body = (await settled(desk().draft(WEBHOOKS))).draft.body
    expect(body).toContain('```mermaid')
    expect(body).toContain('<details>')
    expect(body).toContain('</details>')
  })

  it('counts the words actually read, against a budget', async () => {
    const lint = (await settled(desk().draft(WEBHOOKS))).lint!
    expect(lint.word_budget).toBe(150)
    expect(lint.visible_words!).toBeLessThanOrEqual(150)
    expect(lint.passes).toBe(true)
  })

  it('flags a finding raised before on the same pull request', async () => {
    expect((await settled(desk().draft(CACHE))).lint!.raised_before_on_pr).toBe(1)
  })

  it('records an edit against the finding it changed, and recounts', async () => {
    const api = desk()
    const draft = api.draft(WEBHOOKS)
    const before = (await settled(draft)).draft.body
    expect((await settled(draft)).edits).toHaveLength(3)

    const result = await api.saveDraft(
      WEBHOOKS,
      before,
      before.replace('I reverted the retry and', 'Reverting the retry and running it,'),
    )

    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('1 edit recorded')

    const after = await settled(draft)
    expect(after.edits.some(edit => edit.finding_key === 'F3' && edit.kind === 'rewritten'))
      .toBe(true)
    expect(after.edits).toHaveLength(4)
    expect(after.lint!.checked_at).not.toBe('')
  })

  it('suppresses a finding whose section the developer cut', async () => {
    const api = desk()
    const draft = api.draft(WEBHOOKS)
    const before = (await settled(draft)).draft.body
    const after = before.slice(0, before.indexOf('### Should · F3'))

    await api.saveDraft(WEBHOOKS, before, after)

    expect((await settled(api.review(WEBHOOKS))).findings[2]).toMatchObject({
      disposition: 'suppressed',
      disposition_reason: 'Cut from the draft before posting',
    })
    expect(
      (await settled(draft)).edits.some(
        edit => edit.kind === 'cut' && edit.finding_key === 'F3',
      ),
    ).toBe(true)
  })

  it('records nothing when the body is unchanged', async () => {
    const api = desk()
    const draft = api.draft(WEBHOOKS)
    const body = (await settled(draft)).draft.body

    const result = await api.saveDraft(WEBHOOKS, body, body)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('Nothing changed')
    expect((await settled(draft)).edits).toHaveLength(3)
  })

  it('approving changes the status and the event, and leaves the layers alone', async () => {
    const api = desk()
    const draft = api.draft(INVOICES)
    const inbox = api.inbox()
    expect((await settled(draft)).draft.status).toBe('ready')

    const result = await api.approveDraft(INVOICES, 'request_changes')
    expect(result.ok).toBe(true)

    const after = await settled(draft)
    expect(after.draft.status).toBe('approved')
    expect(after.draft.event).toBe('request_changes')
    expect(after.draft.approved_at).not.toBeNull()
    /* `draft_status` is derived from the draft, so it follows. The layer row does not. */
    expect((await settled(api.review(INVOICES))).draft_status).toBe('approved')
    expect((await settled(inbox)).drafts_to_approve.map(item => item.review_id))
      .not.toContain(INVOICES)
  })

  it('a write moves when the review was last written to, and a read does not', async () => {
    /*
     * **The contract's rule, which these records did not follow.** `last_activity_at` moves
     * on every write and on no read, and the stamp used to sit at whatever the fixture
     * held — so sample mode read "last written to 4 days ago" one second after a sign-off.
     * The draft page's `PostingWatch` shows that number while it waits for a session to
     * post, which makes it the one figure an invented record must not get wrong.
     */
    const api = desk()
    const review = api.review(INVOICES)
    const before = (await settled(review)).timings.last_activity_at
    expect(before).not.toBeNull()

    /* A read does not move it. */
    await review.refresh()
    expect((await settled(review)).timings.last_activity_at).toBe(before)

    const at = Date.parse(before!)
    expect((await api.approveDraft(INVOICES, 'request_changes')).ok).toBe(true)
    const after = (await settled(review)).timings.last_activity_at
    expect(Date.parse(after!)).toBeGreaterThan(at)
  })

  it('refuses to approve a draft that is not ready, as a conflict', async () => {
    const api = desk()
    await api.approveDraft(INVOICES, 'comment')

    /* Approving twice: the record is not in a state this act can be performed from. */
    const again = await api.approveDraft(INVOICES, 'comment')
    expect(again.ok).toBe(false)
    if (again.ok) return
    expect(again.error.code).toBe('conflict')
  })

  it('refuses to edit an approved draft, because that is what gets posted', async () => {
    const api = desk()
    const draft = api.draft(INVOICES)
    const body = (await settled(draft)).draft.body
    await api.approveDraft(INVOICES, 'comment')

    const result = await api.saveDraft(INVOICES, body, `${body}\n\nAn afterthought.`)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect((await settled(draft)).draft.body).toBe(body)
  })

  it('refuses to send a draft back with no note', async () => {
    const api = desk()
    const result = await api.sendBackDraft(INVOICES, '  ')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('invalid_value')
    expect((await settled(api.draft(INVOICES))).draft.status).toBe('ready')
  })

  it('sends a draft back with the note, and leaves the inbox', async () => {
    const api = desk()
    const inbox = api.inbox()
    const result = await api.sendBackDraft(INVOICES, 'Check the warehouse column first.')
    expect(result.ok).toBe(true)

    expect((await settled(api.draft(INVOICES))).draft).toMatchObject({
      status: 'sent_back',
      sent_back_note: 'Check the warehouse column first.',
      event: null,
    })
    expect((await settled(inbox)).drafts_to_approve.map(item => item.review_id))
      .not.toContain(INVOICES)
  })

  it('asks for one ticket covering the findings held back now', async () => {
    const api = desk()
    const review = api.review(WEBHOOKS)
    expect((await settled(review)).nit_ticket.status).toBe('none')

    const result = await api.requestNitTicket(WEBHOOKS)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('3 findings')

    expect((await settled(review)).nit_ticket).toMatchObject({
      status: 'requested',
      finding_seqs: [4, 5, 7],
    })
    expect((await settled(api.draft(WEBHOOKS))).nit_ticket.status).toBe('requested')
  })

  it('asking twice keeps the first time asked, as the store does', async () => {
    const api = desk()
    await api.requestNitTicket(WEBHOOKS)
    const first = (await settled(api.review(WEBHOOKS))).nit_ticket.requested_at

    const again = await api.requestNitTicket(WEBHOOKS)
    expect(again.ok).toBe(true)
    expect((await settled(api.review(WEBHOOKS))).nit_ticket.requested_at).toBe(first)
  })

  it('refuses a ticket for a review that holds nothing back', async () => {
    const result = await desk().requestNitTicket(EXPORT)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('holds back no nits')
  })

  it('has no draft for a review the session has not reached layer 6 on', async () => {
    const error = await refused(desk().draft(DRAFTING))
    expect(error.code).toBe('not_found')
  })

  it('shows a posted draft as posted', async () => {
    expect((await settled(desk().draft(EXPORT))).draft.status).toBe('posted')
  })
})

/*
 * The developer's decisions, which are the only writes on the code review page.
 *
 * **Every assertion here is also an assertion about the Rust server.** Sample mode
 * reproduces `crates/store/src/entities/decision.rs`'s three tables rather than patching a
 * record freely, and the failure this guards against is the one that would be worst: a
 * decision that works on fixtures and is refused by the binary. So what is checked is the
 * rules — which decision settles what, which needs a block that exists, which writes one
 * that does not — and not merely that a click changes something.
 */
describe('deciding on a finding', () => {
  /** #514, the real-length review: fifteen findings and a draft composed of blocks. */
  const REAL = 41

  async function findingOn(id: number, seq: number) {
    const detail = await settled(desk().review(id))
    return detail.findings.find(finding => finding.seq === seq)!
  }

  it('keeps a finding, and the draft still carries its block', async () => {
    const api = desk()
    const result = await api.decideFinding(REAL, 9, { decision: 'keep' })
    expect(result.ok).toBe(true)

    const finding = await findingOn(REAL, 9)
    expect(finding.disposition).toBe('in_draft')
    const draft = await settled(api.draft(REAL))
    expect(draft.draft.body).toContain('F9')
  })

  it('cuts a finding: its block stops posting and stays on the record', async () => {
    const api = desk()
    expect((await api.decideFinding(REAL, 10, { decision: 'cut' })).ok).toBe(true)

    const finding = await findingOn(REAL, 10)
    expect(finding.disposition).toBe('suppressed')

    const draft = await settled(api.draft(REAL))
    expect(draft.draft.body).not.toContain('F10')
    /*
     * **The block is still served.** A cut finding's block stays on the record — that is
     * the difference between a decision and a deletion — so the page can still say what
     * the session had written.
     */
    const block = draft.sections.find(section => section.finding_seq === 10)!
    expect(block).toBeDefined()
    expect(block.posted).toBe(false)
  })

  it('regrades a finding, and its row and its thread change with it', async () => {
    const api = desk()
    expect((await api.decideFinding(REAL, 1, {
      decision: 'regrade',
      severity: 'blocker',
    })).ok).toBe(true)

    expect((await findingOn(REAL, 1)).severity).toBe('blocker')
    const draft = await settled(api.draft(REAL))
    expect(draft.sections.find(section => section.finding_seq === 1)!.heading)
      .toMatch(/^### Blocker · F1 · /)

    /*
     * **F1 is anchored, so what a regrade changes is the row and the alert.** Since
     * DEV-929 a finding GitHub can anchor is a row in the body's index and a comment on
     * the code; there is no heading in the body to change, and the block's composed
     * heading above is what the code review page draws.
     */
    expect(draft.draft.body).toContain('| 🔴 Blocker · F1 |')
    expect(draft.draft.body).not.toContain('### Blocker · F1 ·')
    const thread = draft.threads.find(one => one.finding_seq === 1)!
    expect(thread.severity).toBe('blocker')
    expect(thread.body).toMatch(/^> \[!CAUTION\]\n> \*\*Blocker · F1\*\* — /)
  })

  it('refuses a regrade with no severity, and a severity on anything else', async () => {
    const api = desk()
    const noSeverity = await api.decideFinding(REAL, 1, { decision: 'regrade' })
    expect(noSeverity.ok).toBe(false)
    expect(noSeverity.ok === false && noSeverity.error.field).toBe('severity')

    const stray = await api.decideFinding(REAL, 1, { decision: 'cut', severity: 'nit' })
    expect(stray.ok).toBe(false)
    expect(stray.ok === false && stray.error.field).toBe('severity')
  })

  it('rewrites a block in the developer’s words and keeps the session’s beside them', async () => {
    const api = desk()
    const before = await settled(api.draft(REAL))
    const was = before.sections.find(section => section.finding_seq === 6)!

    expect((await api.decideFinding(REAL, 6, {
      decision: 'rewrite',
      title: 'The policy runs after the query, not in it',
      text: 'Please narrow the select rather than the result.',
    })).ok).toBe(true)

    const draft = await settled(api.draft(REAL))
    const block = draft.sections.find(section => section.finding_seq === 6)!
    expect(block.title).toBe('The policy runs after the query, not in it')
    expect(block.text).toBe('Please narrow the select rather than the result.')
    expect(block.rewritten).toBe(true)
    /* The agent's own words are still there, which is what "beside them" means. */
    expect(block.agent_title).toBe(was.agent_title)
    expect(block.agent_text).toBe(was.agent_text)
  })

  it('refuses a rewrite with neither a title nor a paragraph', async () => {
    const refusal = await desk().decideFinding(REAL, 6, { decision: 'rewrite' })
    expect(refusal.ok).toBe(false)
    expect(refusal.ok === false && refusal.error.field).toBe('text')
  })

  it('refuses words on a decision that takes the finding out of the post', async () => {
    const refusal = await desk().decideFinding(REAL, 6, {
      decision: 'to_ticket',
      title: 'something else',
    })
    expect(refusal.ok).toBe(false)
    expect(refusal.ok === false && refusal.error.field).toBe('text')
  })

  it('moves a finding to a ticket, and the ticket covers it', async () => {
    const api = desk()
    expect((await api.decideFinding(REAL, 3, {
      decision: 'to_ticket',
      note: 'worth doing, not worth this round',
    })).ok).toBe(true)

    const finding = await findingOn(REAL, 3)
    expect(finding.disposition).toBe('held_back')
    expect(finding.round_status).toBe('ticketed')
    expect(finding.round_status_by).toBe('developer')
    expect(finding.round_status_checked).toBe('worth doing, not worth this round')

    const draft = await settled(api.draft(REAL))
    expect(draft.draft.body).not.toContain('F3')
    expect(draft.held_back.map(item => item.finding_seq)).toContain(3)
  })

  it('requires the author’s reason before accepting it', async () => {
    const api = desk()
    const refusal = await api.decideFinding(REAL, 5, { decision: 'accept_reason' })
    expect(refusal.ok).toBe(false)
    expect(refusal.ok === false && refusal.error.field).toBe('note')

    expect((await api.decideFinding(REAL, 5, {
      decision: 'accept_reason',
      note: 'Only one schedule per tenant can be active; the validator enforces it.',
    })).ok).toBe(true)

    const finding = await findingOn(REAL, 5)
    expect(finding.disposition).toBe('suppressed')
    expect(finding.round_status).toBe('accepted')
    /*
     * **Recorded as `rejected`, with the reason as the detail.** That is what
     * "author disagreed once, with a reason you accepted" counts, so accepting has to be
     * that outcome rather than one of its own that nothing would read.
     */
    const detail = await settled(api.review(REAL))
    const outcome = detail.outcomes.find(one => one.finding_seq === 5)!
    expect(outcome.kind).toBe('rejected')
    expect(outcome.actor).toBe('developer')
    expect(outcome.detail).toContain('Only one schedule per tenant')
  })

  it('withdraws a finding, and the record says who withdrew it', async () => {
    const api = desk()
    expect((await api.decideFinding(REAL, 4, { decision: 'withdraw' })).ok).toBe(true)

    const finding = await findingOn(REAL, 4)
    expect(finding.round_status).toBe('withdrawn')
    const detail = await settled(api.review(REAL))
    expect(detail.outcomes.find(one => one.finding_seq === 4)!.kind).toBe('withdrawn')
  })

  it('posts a held-back finding by writing it a block it had none of', async () => {
    const api = desk()
    const before = await settled(api.draft(REAL))
    expect(before.sections.some(section => section.finding_seq === 2)).toBe(false)

    expect((await api.decideFinding(REAL, 2, { decision: 'keep' })).ok).toBe(true)

    const draft = await settled(api.draft(REAL))
    const block = draft.sections.find(section => section.finding_seq === 2)!
    expect(block.posted).toBe(true)
    expect(draft.draft.body).toContain('F2')
    /*
     * Written from the finding's own words, because the request carried none: its
     * consequence, or failing that its comment for the author. Never invented.
     */
    const finding = await findingOn(REAL, 2)
    expect(block.text).toBe(finding.why_it_matters ?? finding.inline_comment)

    /*
     * **And the heading comes from the finding's title, not its claim.**
     *
     * This asserted only the text, and sample mode was writing `agent_title` from the
     * claim while the store wrote it from the title — so pressing this button composed
     * `### Nit · F2 · <56 words of claim>` on fixtures and `### Nit · F2 · <the title>`
     * against the binary. The one failure mode this whole file exists to prevent, found by
     * the self-review rather than by any test here.
     */
    expect(block.title).toBe(finding.title)
    expect(block.agent_title).toBe(finding.title)
    expect(draft.draft.body).toContain(`### Nit · F2 · ${finding.title}`)
  })

  it('refuses a cut when the draft holds no block for the finding', async () => {
    /*
     * #412's draft was written as one block of markdown, which is every draft written
     * before the composition existed. Cutting changes words that have to exist first, so
     * there is nothing to cut — and the page says that, in words, once.
     */
    const refusal = await desk().decideFinding(WEBHOOKS, 1, { decision: 'cut' })
    expect(refusal.ok).toBe(false)
    expect(refusal.ok === false && refusal.error.code).toBe('conflict')
    expect(refusal.ok === false && refusal.error.message).toContain('one block of markdown')
  })

  it('refuses every decision once the draft is approved', async () => {
    const api = desk()
    expect((await api.approveDraft(REAL, 'request_changes')).ok).toBe(true)

    const refusal = await api.decideFinding(REAL, 9, { decision: 'cut' })
    expect(refusal.ok).toBe(false)
    expect(refusal.ok === false && refusal.error.code).toBe('conflict')
    expect(refusal.ok === false && refusal.error.message).toContain('approved')
  })

  it('leaves the lint not checked, never passing, after a decision', async () => {
    const api = desk()
    expect((await settled(api.draft(REAL))).lint).not.toBeNull()

    expect((await api.decideFinding(REAL, 10, { decision: 'cut' })).ok).toBe(true)

    /*
     * **Not a failing lint — no lint.** The review lint was run by the agent over the body
     * it wrote; the moment a decision changes that body the result is about a different
     * document, and a lint still saying "passes" would be the record vouching for words
     * nobody checked.
     */
    expect((await settled(api.draft(REAL))).lint).toBeNull()
  })

  it('composes the body in the house style’s shape, and the threads with it', async () => {
    const api = desk()
    const draft = await settled(api.draft(REAL))
    const review = await settled(api.review(REAL))

    expect(draft.draft.body)
      .toBe(composeBody(review.review, draft.sections, review.findings))
    expect(draft.threads).toEqual(composeThreads(draft.sections, review.findings))

    /*
     * And the three things that shape is, said once here so a reader of this file does not
     * have to run the composition in their head: an index of every posted finding, no
     * section for a finding that is a thread, and a section for one that cannot be.
     */
    const rows = draft.draft.body
      .split('\n')
      .filter(line => line.startsWith('| ') && !line.startsWith('| -- |') && !line.includes('| Where |'))
    const posting = review.findings.filter(
      finding => finding.disposition === 'in_draft'
        && (finding.round_status ?? 'new') !== 'ticketed',
    )
    expect(rows.length).toBe(posting.length)
    for (const thread of draft.threads) {
      expect(draft.draft.body).not.toContain(`· F${thread.finding_seq} ·`)
    }
    expect(draft.draft.body).toContain('### Should · F14 · ')
  })
})

/*
 * "Ticket what is left, and approve" — a sign-off, and it neither posts nor files anything.
 */
describe('ticketing the rest and approving', () => {
  const REAL = 41

  it('moves every open finding into the ticket and approves as approve', async () => {
    const api = desk()
    const before = await settled(api.review(REAL))
    const open = before.findings
      .filter(finding => finding.disposition === 'in_draft')
      .map(finding => finding.seq)
    expect(open.length).toBeGreaterThan(0)

    const result = await api.ticketRestAndApprove(REAL, 'all of it goes in one ticket')
    expect(result.ok).toBe(true)
    expect(result.ok === true && result.message).toContain('An agent files it')

    const after = await settled(api.review(REAL))
    for (const seq of open) {
      const finding = after.findings.find(one => one.seq === seq)!
      expect(finding.disposition).toBe('held_back')
      expect(finding.round_status).toBe('ticketed')
      expect(finding.round_status_by).toBe('developer')
    }
    expect(after.nit_ticket.status).toBe('requested')
    expect(after.nit_ticket.finding_seqs).toEqual(
      expect.arrayContaining([...open, 2]),
    )

    const draft = await settled(api.draft(REAL))
    /*
     * **The event is not the caller's to choose.** The button says approve, and a request
     * for changes that ticketed everything it asked for would contradict itself.
     */
    expect(draft.draft.status).toBe('approved')
    expect(draft.draft.event).toBe('approve')
    /* And the body is recomposed without them: no finding block posts. */
    expect(draft.sections.filter(section => section.kind === 'finding' && section.posted))
      .toHaveLength(0)
  })

  it('refuses when the review has no open finding to file', async () => {
    const api = desk()
    expect((await api.ticketRestAndApprove(REAL, '')).ok).toBe(true)

    const again = await api.ticketRestAndApprove(REAL, '')
    expect(again.ok).toBe(false)
    expect(again.ok === false && again.error.code).toBe('conflict')
  })
})

describe('the rules', () => {
  it('filters by repository, and keeps global rules in every repository', async () => {
    const repo = ref(PORTAL)
    const page = desk().rules(repo)
    expect((await settled(page)).repo).toBe(PORTAL)
    expect((await settled(page)).active.map(rule => rule.key))
      .toEqual(['R-09', 'R-03', 'R-14', 'R-02'])

    repo.value = 'acme/billing-api'
    expect((await settled(page)).active.map(rule => rule.key)).toEqual(['R-11', 'R-02'])
    /* Nothing is proposed there, which the screen shows by leaving the group out. */
    expect((await settled(page)).proposed).toEqual([])

    repo.value = ALL_REPOSITORIES
    expect((await settled(page)).repo).toBeNull()
    expect((await settled(page)).active).toHaveLength(5)
  })

  it('counts the active rules on screen, and leaves the replay figure absent', async () => {
    const page = await settled(desk().rules(computed(() => PORTAL)))
    expect(page.summary.active_rules).toBe(4)
    expect(page.summary.active_rules_cap).toBe(20)
    expect(page.summary.replay_matched).toBeNull()
    expect(page.summary.replay_total).toBeNull()
  })

  it('separates a rule going stale from the ones in force', async () => {
    const page = await settled(desk().rules(computed(() => PORTAL)))
    expect(page.stale.map(rule => rule.key)).toEqual(['R-05'])
    expect(page.active.map(rule => rule.key)).not.toContain('R-05')
  })

  it('promoting moves a proposal into the active list and raises the count', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))
    const inbox = api.inbox()
    expect((await settled(page)).summary.active_rules).toBe(4)

    const result = await api.promoteRule(22)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('R-22')

    const after = await settled(page)
    expect(after.proposed).toHaveLength(0)
    expect(after.active.map(rule => rule.key)).toContain('R-22')
    expect(after.summary.active_rules).toBe(5)
    expect((await settled(inbox)).proposed_rules.map(rule => rule.rule_key))
      .not.toContain('R-22')
  })

  it('refuses a promotion that would skip a stage', async () => {
    const api = desk()
    /*
     * R-22 is a proposal, and the stage after `proposed` is `active`. Naming `ci` is what
     * an accidental double click would amount to, and the contract has the caller state
     * the stage precisely so that it cannot happen silently.
     */
    const result = await api.promoteRule(22, 'ci')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('the stage that follows is active')
  })

  it('refuses to dismiss a rule without a reason', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))
    const result = await api.dismissRule(22, '')
    expect(result.ok).toBe(false)
    if (!result.ok) expect(result.error.code).toBe('invalid_value')
    expect((await settled(page)).proposed).toHaveLength(1)
  })

  it('dismissing with a reason removes the proposal', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))
    const inbox = api.inbox()

    const result = await api.dismissRule(22, 'Renames are a formatter’s job.')
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('R-22')

    expect((await settled(page)).proposed).toHaveLength(0)
    expect((await settled(inbox)).proposed_rules).toHaveLength(0)
  })

  it('refuses to retire a rule without a reason, then retires it', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))
    expect((await api.retireRule(5, ' ')).ok).toBe(false)
    expect((await settled(page)).stale).toHaveLength(1)

    expect((await api.retireRule(5, 'The importer is gone in #415.')).ok).toBe(true)
    const after = await settled(page)
    expect(after.stale).toHaveLength(0)
    expect(after.active.map(rule => rule.key)).not.toContain('R-05')
  })

  it('keeping a stale rule puts it back in force with a date to ask again', async () => {
    const api = desk()
    const page = api.rules(computed(() => PORTAL))

    const result = await api.deferRule(5, 60)
    expect(result.ok).toBe(true)
    if (result.ok) expect(result.message).toContain('60 days')

    const after = await settled(page)
    expect(after.stale).toHaveLength(0)
    const kept = after.active.find(rule => rule.key === 'R-05')
    expect(kept).toBeDefined()
    expect(kept!.review_again_at).toBeTruthy()
  })

  it('refuses a deferral outside the contract’s range', async () => {
    const result = await desk().deferRule(5, 400)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('invalid_value')
    expect(result.error.expected).toContain('1 to 365')
  })

  it('says so when there is no such rule', async () => {
    const result = await desk().promoteRule(999)
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('not_found')
  })
})

describe('starting a review', () => {
  it('refuses anything that is not a pull request link, and says what it wanted', async () => {
    const api = desk()
    for (const input of ['not a link', 'https://github.com/acme/portal']) {
      const result = await api.startReview(input)
      expect(result.ok).toBe(false)
      if (result.ok) continue
      expect(result.error.code).toBe('invalid_value')
      expect(result.error.field).toBe('pr_url')
      expect(result.error.expected).toContain('pull request link')
    }
  })

  it('refuses an empty field before it sends anything', async () => {
    const result = await desk().startReview('   ')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('bad_request')
    expect(result.error.field).toBe('pr_url')
  })

  it('refuses a pull request already on the desk, as a conflict', async () => {
    const result = await desk().startReview('https://github.com/acme/portal/pull/412')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('already on the desk')
  })

  /*
   * The head commit, the title and the changed-line count are read from GitHub when the
   * caller has none, so a machine with no `gh` fails the request outright and creates
   * nothing. That is the developer's environment rather than their request, and the screen
   * has to say so differently — which is what `refusalTone` decides on.
   */
  it('reports a GitHub it could not read, and creates nothing', async () => {
    await resetDesk({ github_available: false })
    const api = desk()
    const inbox = api.inbox()
    const before = (await settled(inbox)).totals.items

    const result = await api.startReview('https://github.com/acme/portal/pull/501')

    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('github_unavailable')
    expect(result.error.message).toContain('gh auth login')
    expect((await settled(inbox)).totals.items).toBe(before)
  })

  it('refuses a bad link before it ever needs GitHub', async () => {
    await resetDesk({ github_available: false })
    const result = await desk().startReview('not a link')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('invalid_value')
  })

  it('says plainly that invented records cannot hold a real review', async () => {
    /*
     * The one place sample mode cannot imitate the server, and it says so rather than
     * reporting a success that recorded nothing. A review invented here would have no
     * brief, no findings and no draft, so it would sit on the Inbox with every screen
     * behind it empty — which reads as a broken app rather than as a sample.
     */
    const result = await desk().startReview('https://github.com/acme/portal/pull/501')
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error.code).toBe('conflict')
    expect(result.error.message).toContain('review-desk serve')
  })
})

describe('a database with nothing in it', () => {
  /*
   * A fresh database, which is what the first run of `review-desk serve` serves. Every
   * screen has to read as "nothing here yet" rather than as broken, and the six reads have
   * to answer rather than fail — an empty desk is an answer.
   */
  beforeEach(async () => {
    await resetDesk({
      reviews: [],
      drafts: [],
      rules: [],
      repos: [],
      own_prs: [],
      handled_without_you: {
        window_days: 7,
        description_problems_caught: null,
        nits_filed: null,
        nit_tickets: null,
      },
      session_liveness: {},
    })
  })

  it('answers the inbox with every group empty', async () => {
    const inbox = await settled(desk().inbox())
    expect(inbox.totals).toEqual({ items: 0, repos: 0 })
    expect(inbox.briefs_to_confirm).toEqual([])
    expect(inbox.drafts_to_approve).toEqual([])
    expect(inbox.author_replies).toEqual([])
    expect(inbox.proposed_rules).toEqual([])
    expect(inbox.in_progress).toEqual([])
  })

  it('answers the rules page with no rule at any stage', async () => {
    const page = await settled(desk().rules(computed(() => ALL_REPOSITORIES)))
    expect(page.proposed).toEqual([])
    expect(page.active).toEqual([])
    expect(page.stale).toEqual([])
    expect(page.summary.active_rules).toBe(0)
  })

  it('answers the repository list with nothing in it', async () => {
    expect(await settled(desk().repos())).toEqual([])
  })

  it('refuses a review that is not there, rather than serving an empty one', async () => {
    /*
     * A `not_found` and an empty review are different answers, and the screen says
     * different things about them: one is "the desk holds no such review", the other
     * would be a review with every section blank.
     */
    expect((await refused(desk().review(WEBHOOKS))).code).toBe('not_found')
    expect((await refused(desk().draft(WEBHOOKS))).code).toBe('not_found')
  })
})

describe('the sample-data marker', () => {
  it('is on while the records are invented', () => {
    expect(desk().isSampleData.value).toBe(true)
  })
})

describe('the fixtures stay inside the contract’s closed sets', () => {
  const SEVERITIES = ['blocker', 'should', 'nit', 'question']
  const DISPOSITIONS = ['in_draft', 'held_back', 'suppressed']
  const LAYER_NAMES = [
    'lint',
    'understand',
    'alternatives',
    'solution_map',
    'code_review',
    'draft',
    'harvest',
  ]
  const LAYER_STATES = ['pending', 'running', 'done', 'waiting_on_you', 'failed', 'skipped']
  const STAGES = ['proposed', 'active', 'lint', 'ci', 'retired']
  const STATUSES = ['drafting', 'ready', 'approved', 'sent_back', 'posted']

  it('every finding, layer and rule uses a value the API defines', async () => {
    const api = desk()
    for (const id of [17, 15, 16, 12, 18, 19]) {
      const detail = await settled(api.review(id))
      expect(detail.layers).toHaveLength(7)
      for (const layer of detail.layers) {
        expect(LAYER_NAMES).toContain(layer.name)
        expect(LAYER_STATES).toContain(layer.state)
      }
      for (const finding of detail.findings) {
        expect(SEVERITIES).toContain(finding.severity)
        expect(DISPOSITIONS).toContain(finding.disposition)
        expect(finding.key).toBe(`F${finding.seq}`)
      }
      if (detail.draft_status) expect(STATUSES).toContain(detail.draft_status)
    }

    const page = await settled(api.rules(computed(() => ALL_REPOSITORIES)))
    for (const rule of [...page.proposed, ...page.active, ...page.stale]) {
      expect(STAGES).toContain(rule.stage)
      expect(rule.key).toMatch(/^R-\d{2,}$/)
      expect(rule.prevents.length).toBeGreaterThan(0)
    }
  })

  it('every timestamp is an instant the API would send', async () => {
    const api = desk()
    const review = await settled(api.review(17))
    const draft = await settled(api.draft(17))
    const stamps = [
      ...review.findings.map(finding => finding.created_at),
      ...review.layers.map(layer => layer.updated_at).filter(Boolean),
      draft.draft.updated_at,
      draft.lint!.checked_at,
    ] as string[]
    for (const stamp of stamps) {
      expect(stamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/)
      expect(Number.isNaN(Date.parse(stamp))).toBe(false)
    }
  })

  it('covers all four severities and all three dispositions', async () => {
    const api = desk()
    const all = []
    for (const id of [17, 15, 16, 12]) all.push(...(await settled(api.review(id))).findings)

    expect(new Set(all.map(finding => finding.severity))).toEqual(
      new Set(['blocker', 'should', 'nit', 'question']),
    )
    expect(new Set(all.map(finding => finding.disposition))).toEqual(
      new Set(['in_draft', 'held_back', 'suppressed']),
    )
  })
})
