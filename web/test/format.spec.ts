import { describe, expect, it } from 'vitest'
import type { InboxPostedItem } from '../app/types/desk'

import {
  activityLabel,
  authorResponseLabel,
  count,
  descriptionLintLabel,
  durationLabel,
  editVerb,
  figure,
  forLabel,
  layerDetail,
  layerLabel,
  layerTone,
  openForLabel,
  prStateLabel,
  sessionLabel,
  severityTally,
  sinceLabel,
  stepDurationLabel,
} from '../app/utils/format'

/*
 * The wording of an absence matters as much as the wording of a number: the contract says
 * a count the server cannot answer is null, and that null is not zero. These are the
 * functions that have to say so.
 */

const NOW = new Date('2026-10-08T11:30:00Z')

describe('saying when', () => {
  it.each([
    ['2026-10-08T11:29:40Z', 'just now'],
    ['2026-10-08T11:24:00Z', '6 min ago'],
    ['2026-10-08T10:30:00Z', '1 hour ago'],
    ['2026-10-08T07:30:00Z', '4 hours ago'],
    ['2026-10-07T11:30:00Z', '1 day ago'],
    ['2026-10-05T11:30:00Z', '3 days ago'],
  ])('%s reads as %s', (at, expected) => {
    expect(sinceLabel(at, NOW)).toBe(expected)
  })

  it('never predicts: a future instant reads as just now', () => {
    expect(sinceLabel('2026-10-09T11:30:00Z', NOW)).toBe('just now')
  })

  it('says so when nothing recorded a time', () => {
    expect(sinceLabel(null, NOW)).toBe('at an unrecorded time')
    expect(openForLabel(null, NOW)).toBe('open for an unrecorded time')
  })

  it('counts whole days a pull request has been open', () => {
    expect(openForLabel('2026-10-05T09:12:00Z', NOW)).toBe('open 3 days')
    expect(openForLabel('2026-10-07T08:40:00Z', NOW)).toBe('open 1 day')
    expect(openForLabel('2026-10-08T08:40:00Z', NOW)).toBe('open today')
  })
})

describe('an absent measurement is not a zero', () => {
  it('a null figure has no number to show', () => {
    expect(figure(null)).toBeNull()
    expect(figure(null, 10)).toBeNull()
    expect(figure(0)).toBe('0')
    expect(figure(9, 10)).toBe('9 of 10')
  })

  it('an unknown session says it is unknown', () => {
    expect(sessionLabel(true)).toBe('session alive')
    expect(sessionLabel(false)).toBe('session ended')
    expect(sessionLabel(null)).toBe('session state unknown')
    expect(sessionLabel(undefined)).toBe('session state unknown')
  })

  it('an unchecked description says it was not checked', () => {
    expect(descriptionLintLabel(null)).toBe('description not checked')
    expect(descriptionLintLabel({ passes: true, budget_ratio: 0.7 })).toBe(
      'description lint passes',
    )
    expect(descriptionLintLabel({ passes: false, budget_ratio: 2.3 })).toBe(
      'description is 2.3× its budget',
    )
    expect(descriptionLintLabel({ passes: false, budget_ratio: null })).toBe(
      'description lint fails',
    )
  })
})

describe('saying how long', () => {
  it.each([
    [0, '0 sec'],
    [42, '42 sec'],
    [60, '1 min'],
    [1020, '17 min'],
    [3600, '1 hr'],
    [3840, '1 hr 4 min'],
    [86_400, '1 day'],
    [90_000, '1 day 1 hr'],
    [180_000, '2 days 2 hr'],
  ])('%d seconds reads as %s', (seconds, expected) => {
    expect(durationLabel(seconds)).toBe(expected)
  })

  it('has nothing to say about a duration the record cannot answer', () => {
    // **The rule the whole of DEV-890 rests on.** A null duration is not a nought: a
    // review recorded before the change has no start for any step, and "0 sec" there
    // would say it took no time at all. Null out, and the screen says which absence it is.
    expect(durationLabel(null)).toBeNull()
    expect(durationLabel(undefined)).toBeNull()
    expect(durationLabel(0)).toBe('0 sec')
  })

  it('tells a step with no duration from one that has not finished', () => {
    expect(stepDurationLabel({ state: 'done', seconds: 1020 })).toBe('17 min')
    expect(stepDurationLabel({ state: 'done', seconds: null })).toBe('time not recorded')
    expect(stepDurationLabel({ state: 'skipped', seconds: null })).toBe('time not recorded')
    /*
     * **A step still going shows nothing, even when it has a total.** Measured on the
     * binary: step 2 waiting on the developer read "7 sec", which is the agent's pass
     * before the wait and reads like how long you have been keeping it. How long a step
     * still going has been where it is belongs to the queue, which says exactly that.
     */
    expect(stepDurationLabel({ state: 'running', seconds: 420 })).toBeNull()
    expect(stepDurationLabel({ state: 'waiting_on_you', seconds: 420 })).toBeNull()
    expect(stepDurationLabel({ state: 'pending', seconds: null })).toBeNull()
  })

  it('says how long something has been where it is', () => {
    expect(forLabel('2026-10-08T11:10:00Z', NOW)).toBe('20 min')
    expect(forLabel('2026-10-08T07:30:00Z', NOW)).toBe('4 hr')
    expect(forLabel(null, NOW)).toBeNull()
  })

  it('says how long the record has been silent, and when it holds nothing', () => {
    // Allele's answer first, because Allele owns the fact. Then the record's own.
    expect(activityLabel(true, null, NOW)).toBe('Session running')
    expect(activityLabel(false, '2026-10-08T11:10:00Z', NOW)).toBe('Session ended')
    expect(activityLabel(null, '2026-10-08T11:24:00Z', NOW)).toBe('Last activity 6 min ago')
    expect(activityLabel(null, null, NOW)).toBe('No activity recorded')
  })
})

describe('counting things', () => {
  it('names the singular', () => {
    expect(count(1, 'item')).toBe('1 item')
    expect(count(7, 'item')).toBe('7 items')
    expect(count(1, 'repository', 'repositories')).toBe('1 repository')
    expect(count(2, 'repository', 'repositories')).toBe('2 repositories')
  })

  /*
   * **The order, with all four present.** It is the order `SEVERITY_WORDS` declares its
   * keys in, and that is a real dependency: since DEV-928 the tally walks the shared table
   * rather than an array of its own, so the four would reorder silently if somebody tidied
   * the table alphabetically. The case above has `question` at nought and cannot see it.
   *
   * **It is not the code review page's order**, which puts a question above a nit — that is
   * `SEVERITY_ORDER`, and the two are deliberately different: this is a tally, that is a
   * ranking.
   */
  it('counts the four severities in the order the record declares them', () => {
    expect(severityTally({ blocker: 1, should: 2, nit: 3, question: 4 })).toBe(
      '1 blocker, 2 should, 3 nits, 4 questions',
    )
  })

  it('leaves out a severity nothing was found at', () => {
    expect(severityTally({ blocker: 1, should: 2, nit: 4, question: 0 })).toBe(
      '1 blocker, 2 should, 4 nits',
    )
    expect(severityTally({ blocker: 0, should: 1, nit: 0, question: 0 })).toBe('1 should')
    expect(severityTally({ blocker: 0, should: 0, nit: 0, question: 0 })).toBe('')
  })
})

describe('the seven layers', () => {
  it('writes a layer key as a person reads it', () => {
    expect(layerLabel('solution_map')).toBe('Solution map')
    expect(layerLabel('code_review')).toBe('Code review')
  })

  /*
   * DEV-870: the store writes layer 6's detail and calls it `approved by the developer`.
   * On screen "approve" is GitHub's word for one of three review kinds, so the developer's
   * act is said in the screen's own vocabulary — and said here, because the store, the API
   * and the command line keep theirs.
   */
  it('says the developer signed off, whatever the store calls it', () => {
    expect(layerDetail('approved by the developer')).toBe('signed off by the developer')
  })

  it('shows a detail it has no word of its own for exactly as the store wrote it', () => {
    expect(layerDetail('confirmed by the developer')).toBe('confirmed by the developer')
    expect(layerDetail('posted, waiting for the harvest')).toBe('posted, waiting for the harvest')
    expect(layerDetail(null)).toBe('')
  })

  it('treats waiting and running as the layer you are on', () => {
    expect(layerTone('waiting_on_you')).toBe('current')
    expect(layerTone('running')).toBe('current')
    expect(layerTone('done')).toBe('done')
    expect(layerTone('pending')).toBe('pending')
    expect(layerTone('skipped')).toBe('pending')
    expect(layerTone('failed')).toBe('failed')
  })

  it('names an outcome as a verb about the developer', () => {
    expect(editVerb('cut')).toBe('Cut')
    expect(editVerb('rewritten')).toBe('Rewrote')
    expect(editVerb('code_changed_after')).toBe('Code changed after')
  })
})

/*
 * What has come back on a posted review — **four states, and the rule that a nought and
 * an absence are different things**.
 *
 * The three counts are nulled on their own sources, so these are not variations of one
 * sentence: a figure may be measured while the two beside it are not, and a review may
 * hold a reply the harvester never wrote.
 */
describe('what came back on a posted review', () => {
  const posted = (over: Partial<InboxPostedItem> = {}): InboxPostedItem => ({
    review_id: 12,
    pull_request: { repo: 'acme/portal', pr: 398, title: 'Split the export job' },
    round: 1,
    rounds_on_pr: 1,
    posted_at: '2026-10-08T16:02:00Z',
    posted_as: 'request_changes',
    pr_state: 'open',
    harvested_at: '2026-10-09T08:15:00Z',
    findings: 3,
    findings_linked: 3,
    replies: 2,
    threads_resolved: 1,
    code_changed_after: 1,
    ...over,
  })

  it('says nothing has looked, rather than nought, on a review no harvest has read', () => {
    const said = authorResponseLabel(posted({
      harvested_at: null,
      pr_state: null,
      findings_linked: 0,
      replies: null,
      threads_resolved: null,
      code_changed_after: null,
    }))
    expect(said).toContain('Not harvested yet')
    expect(said).not.toMatch(/\b0\b/)
    /* And it does not date an absence. */
    expect(said).not.toContain('as at')
  })

  it('names the figures and when they were taken', () => {
    const said = authorResponseLabel(posted())
    expect(said).toContain('2 replies')
    expect(said).toContain('1 thread resolved')
    expect(said).toContain('1 finding with code changed since')
    /*
     * **The measurement's own time.** A bare nought over a figure taken two hours ago
     * reads as a statement about now, and the record only supports one about then.
     */
    expect(said).toContain('as at')
  })

  /*
   * **The state the harvester actually leaves most reviews in.** It reaches a thread
   * through a finding's `github_comment_id`, skips thread reading when no finding has
   * one, and marks the review harvested regardless — so "0 replies" there would be a
   * nought over something nobody could see.
   */
  it('says no reply could be read when no finding is linked to a comment', () => {
    const said = authorResponseLabel(posted({
      findings_linked: 0,
      replies: null,
      threads_resolved: null,
    }))
    expect(said).toContain('No finding is linked to a comment on GitHub')
    expect(said).toContain('3 findings')
  })

  /*
   * And that absence must not silence the figure beside it: `code_changed_after` is
   * measured on its own source and is a real count even when no thread could be read.
   */
  it('keeps a measured figure when the two beside it are unknown', () => {
    const said = authorResponseLabel(posted({
      findings_linked: 0,
      replies: null,
      threads_resolved: null,
      code_changed_after: 4,
    }))
    expect(said).toContain('No finding is linked')
    expect(said).toContain('4 findings with code changed since')
  })

  /*
   * The converse: the harvest writes its outcomes and its own timestamp in separate
   * statements, and `outcome add` writes outcomes with no harvest at all. A reply the
   * record holds is counted, and the row says it did not come from a harvest.
   */
  it('counts a reply the record holds even with no harvest behind it', () => {
    const said = authorResponseLabel(posted({ harvested_at: null, replies: 3 }))
    expect(said).toContain('3 replies')
    expect(said).toContain('recorded without a harvest')
    expect(said).not.toContain('Not harvested yet')
  })

  /*
   * **The mixed state, which `?? 0` printed a nought over.** An outcome recorded by hand
   * gives a review a reply count with no harvest behind it — and the resolution beside it
   * is still unmeasured, so it is left out rather than drawn as none.
   */
  it('draws no figure for the half of a pair that is still unmeasured', () => {
    const said = authorResponseLabel(posted({
      harvested_at: null,
      findings_linked: 0,
      replies: 3,
      threads_resolved: null,
      code_changed_after: null,
    }))
    expect(said).toContain('3 replies')
    expect(said).not.toContain('0 threads resolved')
    expect(said).not.toContain('thread')
  })

  it('does not claim a pull request is open when nothing has looked', () => {
    expect(prStateLabel(null)).toBe('pull request state not established')
    expect(prStateLabel('open')).toBe('pull request open at the last harvest')
  })
})
