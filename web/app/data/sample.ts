/*
 * Sample mode: invented records, in memory, behind the same twenty-two operations.
 *
 * **This is the only file in `web/` that names a fixture**, and nothing imports it unless
 * `source.ts` decides to — which a production build has already decided not to, so none of
 * this reaches a built dashboard. `test/architecture.spec.ts` and
 * `test/build-output.spec.ts` hold both halves of that.
 *
 * ## Why it survived the switch to the API
 *
 * Two reasons, and neither is nostalgia. The component tests mount five screens and assert
 * what they render; against a real server they would be a server test wearing a component
 * test's clothes, and they would need one running to say anything at all. And a developer
 * changing a screen can see it with records on it without starting a binary and seeding a
 * database.
 *
 * ## It refuses the way the server refuses
 *
 * A write here raises `DeskRequestError` with the contract's own `code` — `invalid_value`
 * for a reason the server would have required, `conflict` for a draft that cannot be
 * edited, `github_unavailable` for a machine where `gh` is not logged in. That is what
 * makes a refusal state tested here a refusal state that works in the binary: the screens
 * branch on `code`, and sample mode speaks the same six.
 *
 * The one thing it cannot be faithful about is a server that is not running, which is why
 * `test/desk.spec.ts` reaches for `api.ts` with a stubbed `fetch` for that state.
 */

import { reactive } from 'vue'

import type { DeskBackend, DraftEditToRecord } from './backend'

import { DeskRequestError } from './fault'
import desk from './desk.json'
import drafts from './drafts.json'
import reviews from './reviews.json'
import rules from './rules.json'
import { visibleWords } from '~/utils/draft'
import {
  composeBody,
  composeThreads,
  findingHeading,
  isPosted,
  SEVERITY_LABELS,
} from '~/utils/houseStyle'
import { count, reviewWorkInWords, reviewWorkIsOneThing } from '~/utils/format'
import type {
  Brief,
  DecideFindingRequest,
  DeskSource,
  DismissedReviews,
  Disposition,
  DraftDetail,
  Finding,
  FindingDecision,
  FindingDecisionMade,
  Inbox,
  InboxPr,
  NitTicket,
  OutcomeKind,
  RepoName,
  RepoSummary,
  Review,
  ReviewDetail,
  ReviewEvent,
  ReviewList,
  RoundStatus,
  Rule,
  RuleStage,
  RulesPage,
  SeverityCounts,
  TicketedAndApproved,
} from '~/types/desk'

/*
 * `7`, not `number`. The contract declares `layers_total` as a constant — the screen's
 * "Layer 5 of 7" needs no constant of its own — and the generated type says so, so this one
 * has to be the literal rather than widen.
 */
const LAYERS_TOTAL = 7 as const

/** The whole desk, cloned so a write can never reach the fixture files. */
export function loadSource(): DeskSource {
  return structuredClone({
    reviews: reviews as unknown as ReviewDetail[],
    drafts: drafts as unknown as DraftDetail[],
    rules: rules as unknown as Rule[],
    summary: desk.summary,
    repos: desk.repos,
    own_prs: desk.own_prs,
    handled_without_you: desk.handled_without_you,
    session_liveness: desk.session_liveness as Record<string, boolean | null>,
    github_available: desk.github_available,
  })
}

const source = reactive<DeskSource>(loadSource())

/**
 * Put the desk back as it loaded, optionally with some of it replaced.
 *
 * Tests use this to reach a state no fixture of the record could express — a machine where
 * `gh` cannot be read, for instance, or a database with nothing in it. No screen calls it.
 */
export function resetSource(overrides: Partial<DeskSource> = {}) {
  Object.assign(source, loadSource(), overrides)
}

/** The desk as it stands, for a test that wants to read it without a screen. */
export function currentSource(): DeskSource {
  return source
}

function nowIso(): string {
  return new Date().toISOString()
}

/* ------------------------------- The refusals -------------------------------- */

/** The server's `not_found`, for a review, finding, draft or rule that is not there. */
function notFound(what: string): DeskRequestError {
  return new DeskRequestError({
    code: 'not_found',
    message: `Review Desk holds no ${what}.`,
    field: null,
    expected: null,
    status: 404,
  })
}

/** The server's `invalid_value`, which has to say what it would have accepted. */
function invalidValue(field: string, message: string, expected: string): DeskRequestError {
  return new DeskRequestError({
    code: 'invalid_value',
    message,
    field,
    expected,
    status: 400,
  })
}

/** The server's `conflict`: a statement about the record, not about the request. */
function conflict(message: string): DeskRequestError {
  return new DeskRequestError({
    code: 'conflict',
    message,
    field: null,
    expected: null,
    status: 409,
  })
}

/* ------------------------------- Reading it ---------------------------------- */

function reviewById(id: number): ReviewDetail | undefined {
  return source.reviews.find(item => item.review.id === id)
}

/**
 * The reviews the developer has not taken out of play.
 *
 * **Every group of the queue is built from this and not from `source.reviews`**, which is
 * what the server's own queries do — one clause, pasted into each of them, so that a group
 * added later cannot quietly put a dismissed review back in front of the developer.
 *
 * The round arithmetic and `reviewById` deliberately read every review, dismissed or not:
 * a dismissed round is still a round the pull request reached, and a dismissed review is
 * still a page the developer can open to restore it.
 */
function inPlay(): ReviewDetail[] {
  return source.reviews.filter(detail => detail.review.dismissed_at == null)
}

function draftById(id: number): DraftDetail | undefined {
  return source.drafts.find(item => item.draft.review_id === id)
}

/**
 * The ticket and its address, for an inbox item that makes a queue row.
 *
 * **On every array that is about a review**, which is what the contract requires and what
 * the queue needs: it draws one row per review, so a review reaches its row through
 * `briefs_to_confirm`, `drafts_to_approve` or `author_replies` depending on what it is
 * waiting for — and a pair served on only one of those means a review whose brief is
 * confirmed and whose draft is waiting has a ticket on the record and no way to reach it.
 *
 * `in_progress` and `posted_awaiting_author` carry it too, because a review can be in one
 * of those and in none of the other three: a session is working, or an author has not
 * answered, and nothing is waiting on the developer. Those rows are drawn from those arrays
 * directly. `own_prs_without_reviewer` is the one array with no ticket, and it is the one
 * with no review.
 */
function ticketOf(detail: ReviewDetail): { ticket_id: string | null, ticket_url: string | null } {
  return {
    ticket_id: detail.review.ticket_id ?? null,
    ticket_url: detail.review.ticket_url ?? null,
  }
}

function prOf(detail: ReviewDetail): InboxPr {
  return {
    repo: detail.review.repo,
    pr: detail.review.pr,
    title: detail.review.title ?? null,
  }
}

/**
 * The severities of the findings that are **in the draft**, which is what the inbox's
 * "Draft ready" row counts.
 *
 * Not `severityTally`, which `utils/format` exports as the inverse — counts in, a
 * sentence out — and which this file already imports a sibling of. One name for two
 * functions with opposite signatures, one import statement apart, is a redeclaration
 * waiting for the first person who reaches for the other one.
 *
 * Separate from `retally` below, which counts the whole review for the One review
 * screen's own figures. Two questions, two counts, one place each.
 */
function draftSeverities(findings: Finding[]): SeverityCounts {
  const counts: SeverityCounts = { blocker: 0, should: 0, nit: 0, question: 0 }
  for (const finding of findings) {
    if (finding.disposition === 'in_draft') counts[finding.severity] += 1
  }
  return counts
}

/** Recount the tallies the API serves precomputed, after a disposition has changed. */
function retally(detail: ReviewDetail) {
  const severities = { blocker: 0, should: 0, nit: 0, question: 0 }
  const dispositions = { in_draft: 0, held_back: 0, suppressed: 0 }
  for (const finding of detail.findings) {
    severities[finding.severity] += 1
    dispositions[finding.disposition] += 1
  }
  detail.finding_severities = severities
  detail.finding_dispositions = dispositions
}

/**
 * Put the draft's held-back list and its ticket back in step with the findings.
 *
 * `DraftDetail.held_back` is a projection of the findings, not a second record of them, so
 * anything that moves a disposition has to rebuild it. Shared by the three operations that
 * do — a disposition change, a decision, and ticketing the rest — because the first of them
 * carried this inline and the next two would each have been a chance to forget a line of it.
 */
function syncHeldBack(owner: ReviewDetail, drafted: DraftDetail | undefined): void {
  if (!drafted) return
  drafted.held_back = owner.findings
    .filter(item => item.disposition === 'held_back')
    .map(item => ({
      finding_seq: item.seq,
      finding_key: item.key,
      claim: item.claim,
      severity: item.severity,
    }))
  drafted.nit_ticket = owner.nit_ticket
}

/**
 * The next outcome id, across the whole desk.
 *
 * The id is `int64` and unique per outcome in the contract, and outcomes live on their
 * review — so counting within one review would hand two reviews the same id. Read from the
 * records each time rather than held in a module variable, because `resetSource` puts the
 * desk back and a counter would keep climbing past what the fixtures hold.
 */
function nextOutcomeId(): number {
  const highest = source.reviews
    .flatMap(detail => detail.outcomes)
    .reduce((top, outcome) => Math.max(top, outcome.id), 0)
  return highest + 1
}

/** Re-run the review lint over a draft body, the way the server does on every save. */
function relint(detail: DraftDetail) {
  const owner = reviewById(detail.draft.review_id)
  const expected = (owner?.findings ?? []).filter(f => f.disposition === 'in_draft')
  const present = expected.filter(f => new RegExp(`\\b${f.key}\\b`).test(detail.draft.body))
  const words = visibleWords(detail.draft.body)
  const budget = detail.lint?.word_budget ?? null
  detail.lint = {
    ...detail.lint,
    visible_words: words,
    findings_in_body: present.length,
    findings_expected: expected.length,
    passes: (budget == null || words <= budget) && present.length === expected.length,
    checked_at: nowIso(),
  }
}

function isActive(rule: Rule): boolean {
  return rule.stage === 'active' || rule.stage === 'lint' || rule.stage === 'ci'
}

function inScope(rule: Rule, repo: string | null): boolean {
  if (repo == null) return true
  return rule.scope.kind === 'global' || rule.scope.repo === repo
}

/**
 * The highest round each pull request has reached, keyed `owner/repo#number`.
 *
 * Read by both the posted group and the reviews list, and built once for each rather than
 * twice: the two had the same four lines, the same key format and the same fold, which is
 * two places for one rule about what supersedes what.
 */
function newestRoundByPr(): Map<string, number> {
  const rounds = new Map<string, number>()
  for (const detail of source.reviews) {
    const key = `${detail.review.repo}#${detail.review.pr}`
    rounds.set(key, Math.max(rounds.get(key) ?? 0, detail.review.round))
  }
  return rounds
}

/** The whole Inbox, recomputed from the desk — the shape `GET /inbox` serves. */
function inbox(): Inbox {
  /*
   * **A brief recorded as needing no confirmation is not in this group**, which is what
   * the server does: the group exists to ask the developer for the one act that field
   * says was unnecessary, so listing it would be asking for it anyway. The review is
   * still on the One review screen, where the developer can read it and still send the
   * draft back.
   */
  const briefs_to_confirm = inPlay()
    .filter(detail => detail.brief && !detail.brief.confirmed && detail.brief.confirmation_required)
    .map(detail => ({
      review_id: detail.review.id,
      pull_request: prOf(detail),
      ...ticketOf(detail),
      brief_ready_at: detail.brief!.updated_at,
      /* The session held at layer 2 only if nothing after it has run. */
      blocks_later_layers: detail.layers
        .filter(item => item.number > 2)
        .every(item => item.state === 'pending'),
    }))

  /*
   * **Every ready draft, confirmed brief or not**, which is what the server does and what
   * this filtered out until DEV-862's end-to-end test caught the two disagreeing.
   *
   * The Inbox is what is waiting on the developer, and a draft at `ready` is waiting on
   * them whatever layer 2 says. A session may run ahead of the confirmation — the contract
   * gives the brief item `blocks_later_layers` precisely to say whether it did — so hiding
   * the draft would mean a review whose session ran ahead showed nothing to do while a
   * finished draft sat behind it.
   */
  const drafts_to_approve = source.drafts
    .filter(detail => detail.draft.status === 'ready')
    .map(detail => ({ detail, owner: reviewById(detail.draft.review_id) }))
    /*
     * **And the owner is a review in play**, which the other four groups get from
     * `inPlay()` and this one has to say for itself: it is built from the drafts rather
     * than from the reviews, so the filter those four share never reaches it. The server's
     * `reviews_with_draft_status` carries `AND {IN_PLAY}` for this same group, and a
     * dismissed review left under "Drafts to approve" here would be sample mode
     * disagreeing with the binary about a screen this file exists to draw.
     */
    .filter(pair => pair.owner != null && pair.owner.review.dismissed_at == null)
    .map(({ owner }) => ({
      review_id: owner!.review.id,
      pull_request: prOf(owner!),
      ...ticketOf(owner!),
      round: owner!.review.round,
      /*
       * **The findings in the draft, not the review's**, which is what the binary sends
       * and what the row is captioned as: "Draft ready · 1 blocker, 2 should". This sent
       * `finding_severities` — every finding on the review — so the tally counted nits
       * that had been held back for a ticket and findings that had been set aside, beside
       * a cell saying how many of them were held back. Found during DEV-919 and named in
       * DEV-930's note; the contract now says which findings the figure counts, because
       * saying nothing is how three implementations came to answer it three ways.
       */
      severities: draftSeverities(owner!.findings),
      held_back: owner!.finding_dispositions.held_back,
      is_delta: owner!.review.round > 1,
      /*
       * When the draft became ready: step 6 entering `waiting_on_you`, which the server
       * reads out of the same record. The queue's age cell said "no time recorded" over
       * this row until DEV-890, and a review whose step 6 has not got there yet still
       * says so.
       */
      draft_ready_at: owner!.timings.draft_ready_at,
    }))

  const author_replies = inPlay()
    .map(detail => ({
      review_id: detail.review.id,
      pull_request: prOf(detail),
      ...ticketOf(detail),
      events: detail.outcomes
        .filter(outcome => outcome.actor !== 'developer')
        .map(outcome => ({
          finding_seq: outcome.finding_seq,
          finding_key: outcome.finding_key,
          kind: outcome.kind,
          detail: outcome.detail ?? null,
        })),
      /* The newest of those outcomes' own times, which the record has always held. */
      replied_at: detail.outcomes
        .filter(outcome => outcome.actor !== 'developer')
        .map(outcome => outcome.occurred_at)
        .sort()
        .at(-1) ?? null,
    }))
    .filter(item => item.events.length > 0)

  const proposed_rules = source.rules
    .filter(rule => rule.stage === 'proposed')
    .map(rule => ({
      rule_id: rule.id,
      rule_key: rule.key,
      statement: rule.statement,
      why_proposed: rule.why_proposed ?? null,
      scope: rule.scope,
    }))

  /*
   * **Any review with a running layer but the harvest, which is what the server asks.**
   * `Store::reviews_in_progress` has no draft-status condition — a layer `running` is the
   * whole of it — and this carried `draft_status === 'drafting'`, which hid the shape the
   * queue's cross-group deduplication exists for.
   *
   * **Step 7 is excluded, as the server excludes it** since DEV-933: a harvest sets that
   * step `running` for as long as the pull request stays open, which is a quarter-hourly
   * read of GitHub rather than a session at work, so every posted review would otherwise
   * sit under "with the agent" for weeks. The contract says the same — an
   * `InboxInProgressItem` may not carry layer 7 — so sample mode without this line serves
   * a shape the real server cannot and the contract refuses.
   *
   * Review 12 is still in both groups, and by the step that can put it there: its
   * harvest step no longer counts, and a session has gone back to its code review to work
   * the author's replies — which is the next thing that happens in this loop, and the
   * case the deduplication is for. Its step 7 carries what `record_harvest_step` would
   * write for its outcomes.
   *
   * **The highest-numbered of them, not the first.** `reviews_in_progress` reports
   * `MAX(number)` — a session that left an earlier step running is further on than that
   * step suggests — and this read the first match until the review of this change. No
   * fixture has two non-harvest steps running, so nothing here tells the two apart; the
   * server's rule is written out anyway rather than left to the order a list arrives in,
   * which nothing fixes.
   */
  const in_progress = inPlay()
    .map(detail => ({
      detail,
      running: detail.layers
        .filter(item => item.state === 'running' && item.name !== 'harvest')
        .reduce<typeof detail.layers[number] | undefined>(
          (furthest, item) => (furthest == null || item.number > furthest.number ? item : furthest),
          undefined,
        ),
    }))
    .filter(pair => pair.running != null)
    .map(({ detail, running }) => ({
      review_id: detail.review.id,
      pull_request: prOf(detail),
      ...ticketOf(detail),
      layer_number: running!.number,
      layer_name: running!.name,
      layers_total: LAYERS_TOTAL,
      session_id: detail.review.session_id ?? null,
      /*
       * Allele owns session liveness, not Review Desk. A review nothing has reported on
       * is null, and the screen says so rather than assuming the session is alive.
       */
      session_alive: source.session_liveness[String(detail.review.id)] ?? null,
      /*
       * What the record can say where Allele's answer is missing: when this step's current
       * pass began, and when anything last wrote to the review. The queue shows the second
       * one where it showed "Session state unknown".
       */
      in_state_since: running!.runs?.find(run => run.finished_at == null)?.started_at ?? null,
      last_activity_at: detail.timings.last_activity_at,
    }))

  /*
   * **Posted, and with the author** — derived the way the server derives it, out of the
   * same fields, so the two cannot come to disagree. Posted is a posted draft *or* a
   * recorded GitHub review id; open is open or never looked at; and only a pull request's
   * newest round, because nothing ever moves a superseded round's state again to retract
   * it. The three counts are nulled on their own source, which is the server's rule and
   * is copied rather than re-invented: null when a figure would be zero and nothing could
   * have found one.
   */
  const newestRound = newestRoundByPr()

  const posted_awaiting_author = inPlay()
    .filter((detail) => {
      const draft = draftById(detail.review.id)
      const isPosted
        = draft?.draft.posted_at != null || detail.review.github_review_id != null
      const open = detail.review.pr_state == null || detail.review.pr_state === 'open'
      const newest
        = newestRound.get(`${detail.review.repo}#${detail.review.pr}`) === detail.review.round
      return isPosted && open && newest
    })
    .map((detail) => {
      const draft = draftById(detail.review.id)
      const harvested = detail.review.harvested_at != null
      const linked = detail.findings.filter(one => one.github_comment_id != null).length
      /* A thread is reached through a finding's comment id; with none, none is read. */
      const readable = harvested && linked > 0
      const byAuthor = (kind: OutcomeKind) =>
        detail.outcomes.filter(one => one.kind === kind && one.actor === 'author').length
      const counted = (count: number, couldLook: boolean) =>
        (count > 0 || couldLook ? count : null)

      return {
        review_id: detail.review.id,
        pull_request: prOf(detail),
        ...ticketOf(detail),
        round: detail.review.round,
        /* The group holds only the newest round, so these are one number. */
        rounds_on_pr: detail.review.round,
        posted_at: draft?.draft.posted_at ?? null,
        posted_as: draft?.draft.event ?? null,
        pr_state: detail.review.pr_state ?? null,
        harvested_at: detail.review.harvested_at ?? null,
        findings: detail.findings.length,
        findings_linked: linked,
        replies: counted(byAuthor('replied'), readable),
        threads_resolved: counted(byAuthor('thread_resolved'), readable),
        /*
         * The harvester's own observation about the code, attributed to nobody — and
         * **distinct findings**, because it records one of these per commit it saw the
         * change in, so a finding touched by three pushes holds three.
         */
        code_changed_after: counted(
          new Set(
            detail.outcomes
              .filter(one => one.kind === 'code_changed_after')
              .map(one => one.finding_seq),
          ).size,
          harvested,
        ),
      }
    })
    .sort((a, b) => (b.posted_at ?? '').localeCompare(a.posted_at ?? ''))

  /*
   * **The five waiting groups, and deliberately not the other two.** `in_progress` is
   * with the agent and `posted_awaiting_author` is with the author; neither asks the
   * developer for anything, and `totals.items` is a count of what does.
   */
  const groups = [
    briefs_to_confirm,
    drafts_to_approve,
    author_replies,
    source.own_prs,
    proposed_rules,
  ]
  const repos = new Set<string>()
  for (const item of [
    ...briefs_to_confirm,
    ...drafts_to_approve,
    ...author_replies,
    ...source.own_prs,
  ]) {
    repos.add(item.pull_request.repo)
  }
  for (const rule of proposed_rules) {
    if (rule.scope.repo) repos.add(rule.scope.repo)
  }

  return {
    totals: {
      items: groups.reduce((total, group) => total + group.length, 0),
      repos: repos.size,
    },
    briefs_to_confirm,
    drafts_to_approve,
    author_replies,
    own_prs_without_reviewer: source.own_prs,
    proposed_rules,
    in_progress,
    posted_awaiting_author,
    handled_without_you: source.handled_without_you,
  }
}

/**
 * `GET /reviews` — every review, newest first, or one repository's.
 *
 * The step is the furthest that is **not** `pending`, which is the server's rule: not the
 * last step written, because a posted review's step 7 is written as `pending` and that
 * rule would report a harvest that has not begun.
 */
function reviewList(repo: string | null, dismissed: DismissedReviews): ReviewList {
  const rounds = newestRoundByPr()

  const wanted = (detail: ReviewDetail): boolean => {
    switch (dismissed) {
      case 'only': return detail.review.dismissed_at != null
      case 'included': return true
      /* The default, and the only safe one: a review taken out of play is out of it. */
      case 'excluded': return detail.review.dismissed_at == null
    }
  }

  const reviews = source.reviews
    .filter(detail => (repo == null || detail.review.repo === repo) && wanted(detail))
    .slice()
    .sort((a, b) => b.review.id - a.review.id)
    .map((detail) => {
      const reached = detail.layers.filter(layer => layer.state !== 'pending').at(-1)
      return {
        review: detail.review,
        rounds_on_pr: rounds.get(`${detail.review.repo}#${detail.review.pr}`) ?? detail.review.round,
        /* Absent together: "nothing has started" is not "the lint is pending". */
        ...(reached
          ? {
              step_number: reached.number,
              step_name: reached.name,
              step_state: reached.state,
              step_detail: reached.detail ?? null,
            }
          : {}),
        steps_total: LAYERS_TOTAL,
        last_activity_at: detail.timings.last_activity_at,
      }
    })

  return { repo: (repo as RepoName | null) ?? null, dismissed, reviews }
}

/** The Rules screen for one repository, or for all of them. */
function rulesPage(repo: string | null): RulesPage {
  const scoped = source.rules.filter(rule => inScope(rule, repo))
  const active = scoped.filter(rule => isActive(rule) && !rule.stale)
  return {
    repo: (repo as RepoName | null) ?? null,
    summary: { ...source.summary, active_rules: active.length },
    proposed: scoped.filter(rule => rule.stage === 'proposed'),
    active,
    stale: scoped.filter(rule => rule.stale && rule.stage !== 'retired'),
  }
}

/* ------------------------------- The writes ---------------------------------- */

/**
 * `POST /reviews` — the developer pastes a link and nothing else.
 *
 * The link is checked the way `crates/server/src/wire.rs` checks it, and in the same order:
 * a malformed link is refused before anything reaches for GitHub, so a machine with no
 * `gh` is told its link is wrong rather than told `gh` is missing.
 */
function startReview(prUrl: string): Review {
  const match = prUrl
    .trim()
    .match(/^(?:https?:\/\/github\.com\/)?([\w.-]+\/[\w.-]+)(?:\/pull\/|#)(\d+)\/?$/)
  if (!match) {
    throw invalidValue(
      'pr_url',
      'Paste a link like github.com/acme/portal/pull/412.',
      'a pull request link such as https://github.com/acme/portal/pull/412',
    )
  }

  const repo = match[1]!
  const pr = Number(match[2])

  if (source.reviews.some(item => item.review.repo === repo && item.review.pr === pr)) {
    throw conflict(`${repo}#${pr} is already on the desk.`)
  }

  /* No head commit was sent, so the server reads the pull request to resolve one. */
  if (!source.github_available) {
    throw new DeskRequestError({
      code: 'github_unavailable',
      message:
        `Could not read ${repo}#${pr}: gh is not logged in. Run \`gh auth login\`, then `
        + 'try again.',
      field: null,
      expected: null,
      status: 502,
    })
  }

  /*
   * Sample mode records nothing: there is no store behind it and a review invented here
   * would have no brief, no findings and no draft, which would put a row on the Inbox that
   * every screen then showed as empty. The refusal says so, in the one place the real API
   * would have succeeded.
   */
  throw conflict(
    `${repo}#${pr} is a real pull request and these records are invented. Start `
    + 'Review Desk with `review-desk serve` to open a review on it.',
  )
}

/**
 * `POST /reviews/{id}/brief/confirm` — layer 2, and the gate on everything after it.
 *
 * **It sets the confirmation and touches nothing else**, which is what the server does.
 * This used to move layer 2 to `done` and layer 6 to `waiting_on_you` as well, and the
 * server never has: a review's layer rows are written by the layer that ran, through
 * `review-desk layer set`, and the contract says they are stored rather than derived
 * precisely so that a skipped layer can be told from one that has not run.
 *
 * The visible consequence is that the layer trail still reads "2 Understand · waiting on
 * you" after the developer has confirmed. That is what the record says, and the screen is
 * right to show it — the record is what is wrong. Left as it is and reported rather than
 * fixed here, because fixing it is a change to what `confirmBrief` writes, which is beyond
 * DEV-862.
 */
/**
 * Refuse an act that needs an agent afterwards, on a review the developer dismissed.
 *
 * **The binary's own rule, in the binary's own words**, which is what makes a refusal
 * state tested on fixtures one that works against the record: a dismissed review is in no
 * `work list`, so confirming its brief, approving or sending back its draft, or asking for
 * its held-back findings to be ticketed would be the developer signing off something
 * nothing will ever act on.
 */
function refuseIfDismissed(id: number, act: string): void {
  const detail = reviewById(id)
  if (detail?.review.dismissed_at != null) {
    throw conflict(
      `Review ${id} is dismissed, so no session will be started for it and nothing would `
      + `come of ${act}; bring it back first.`,
    )
  }
}

function confirmBrief(id: number): Brief {
  const detail = reviewById(id)
  if (!detail) throw notFound(`review ${id}`)
  refuseIfDismissed(id, 'confirming its brief')
  if (!detail.brief) throw conflict(`Review ${id} has no brief to confirm.`)
  if (detail.brief.confirmed) return detail.brief

  detail.brief.confirmed = true
  detail.brief.confirmed_at = nowIso()
  return detail.brief
}

/**
 * `PATCH /reviews/{id}/findings/{seq}` — move a finding between the draft, held back and
 * suppressed. A finding is never deleted; the reason is what a later rule cites.
 */
function setFindingDisposition(
  reviewId: number,
  seq: number,
  disposition: Disposition,
  reason: string | null,
): Finding {
  const detail = reviewById(reviewId)
  const finding = detail?.findings.find(item => item.seq === seq)
  if (!detail || !finding) throw notFound(`finding ${seq} on review ${reviewId}`)

  const trimmed = (reason ?? '').trim()
  if (disposition !== 'in_draft' && trimmed.length === 0) {
    throw invalidValue(
      'reason',
      'Say why it is not going in the draft. A finding removed without a reason teaches '
      + 'the harvester nothing.',
      'a sentence saying why',
    )
  }

  finding.disposition = disposition
  finding.disposition_reason = disposition === 'in_draft' ? null : trimmed
  retally(detail)

  /* The ticket covers whatever is held back now, not whatever was held back before. */
  if (detail.nit_ticket.status === 'none') {
    detail.nit_ticket.finding_seqs = detail.findings
      .filter(item => item.disposition === 'held_back')
      .map(item => item.seq)
  }

  const drafted = draftById(reviewId)
  if (drafted) {
    syncHeldBack(detail, drafted)
    relint(drafted)
  }

  return finding
}

/** `PUT /reviews/{id}/draft` — the body, plus the edits this save represents. */
function saveDraft(id: number, body: string, edits: DraftEditToRecord[]): DraftDetail {
  const detail = draftById(id)
  if (!detail) throw notFound(`draft on review ${id}`)

  if (detail.draft.status === 'approved' || detail.draft.status === 'posted') {
    throw conflict(
      `This draft is ${detail.draft.status} and cannot be edited: an approved draft is `
      + 'what the review session reads and posts.',
    )
  }

  const owner = reviewById(id)
  const at = nowIso()

  detail.draft.body = body
  detail.draft.updated_at = at

  for (const edit of edits) {
    const finding = owner?.findings.find(item => item.seq === edit.finding_seq)
    if (!finding) throw notFound(`finding ${edit.finding_seq} on review ${id}`)
    if (edit.summary.trim().length === 0) {
      throw invalidValue(
        'edits[].summary',
        `The edit to ${finding.key} has an empty summary.`,
        'what changed, in a few words',
      )
    }

    const existing = detail.edits.find(
      item => item.finding_seq === edit.finding_seq && item.kind === edit.kind,
    )
    if (existing) {
      existing.summary = edit.summary
      existing.occurred_at = at
    }
    else {
      detail.edits.push({ ...edit, finding_key: finding.key, occurred_at: at })
    }

    if (edit.kind === 'cut' && finding.disposition === 'in_draft' && owner) {
      finding.disposition = 'suppressed'
      finding.disposition_reason = 'Cut from the draft before posting'
      retally(owner)
    }
  }

  relint(detail)
  return detail
}

/** `POST /reviews/{id}/draft/approve` — approved here, posted by the review session. */
function approveDraft(id: number, event: ReviewEvent): DraftDetail {
  const detail = draftById(id)
  if (!detail) throw notFound(`draft on review ${id}`)
  refuseIfDismissed(id, 'approving its draft')
  if (detail.draft.status !== 'ready') {
    throw conflict(
      `This draft is ${detail.draft.status} and only a draft that is ready can be `
      + 'approved.',
    )
  }

  detail.draft.status = 'approved'
  detail.draft.event = event
  detail.draft.approved_at = nowIso()
  detail.draft.sent_back_note = null
  detail.draft.sent_back_at = null

  /* `draft_status` is derived from the draft, so it follows; the layer rows do not. */
  const owner = reviewById(id)
  if (owner) owner.draft_status = 'approved'

  return detail
}

/** `POST /reviews/{id}/draft/send-back` — refused without a note, as the API refuses it. */
function sendBackDraft(id: number, note: string): DraftDetail {
  const trimmed = note.trim()
  if (trimmed.length === 0) {
    throw invalidValue(
      'note',
      'Say what the session is to change. A draft sent back with no note teaches it '
      + 'nothing.',
      'what the session is to change',
    )
  }

  refuseIfDismissed(id, 'sending its draft back')

  const detail = draftById(id)
  if (!detail) throw notFound(`draft on review ${id}`)
  /*
   * **Ready, or approved and not yet posted.** Taking a sign-off back is the one transition
   * out of `approved`, and it is the developer's: without it a draft the hand-over refuses
   * cannot be posted, rewritten or sent back at all.
   */
  if (detail.draft.status !== 'ready' && detail.draft.status !== 'approved') {
    throw conflict(
      `This draft is ${detail.draft.status} and only a draft that is ready, or one approved `
      + 'and not yet posted, can be sent back.',
    )
  }

  detail.draft.status = 'sent_back'
  detail.draft.sent_back_note = trimmed
  detail.draft.sent_back_at = nowIso()
  /* The sign-off goes with it: the record should not hold one that was withdrawn. */
  detail.draft.event = null
  detail.draft.approved_at = null

  const owner = reviewById(id)
  if (owner) owner.draft_status = 'sent_back'

  return detail
}

/** `POST /reviews/{id}/held-back/ticket` — Review Desk records it; an agent files it. */
function requestNitTicket(id: number, ticketId: string | null = null): NitTicket {
  const detail = reviewById(id)
  if (!detail) throw notFound(`review ${id}`)
  /*
   * **The ask, not the report** — the split the store makes, for its reason: with no
   * ticket id this is the developer asking for one, which needs an agent a dismissed
   * review will never be given; with one it is a session saying which ticket it already
   * filed, and refusing that would lose an id that exists in the tracker. Guarding both
   * here made this backend refuse where the binary accepts, which is the one thing it
   * must not do.
   */
  if (ticketId == null) {
    refuseIfDismissed(id, 'asking for its held-back findings to be ticketed')
  }

  const seqs = detail.findings
    .filter(finding => finding.disposition === 'held_back')
    .map(finding => finding.seq)
  if (seqs.length === 0) {
    throw conflict(`Review ${id} holds back no nits, so there is nothing to file.`)
  }

  detail.nit_ticket = {
    ...detail.nit_ticket,
    status: ticketId == null ? 'requested' : 'filed',
    ticket_id: ticketId ?? detail.nit_ticket.ticket_id ?? null,
    finding_seqs: seqs,
    requested_at: detail.nit_ticket.requested_at ?? nowIso(),
    filed_at: ticketId == null ? detail.nit_ticket.filed_at ?? null : nowIso(),
  }

  const drafted = draftById(id)
  if (drafted) drafted.nit_ticket = detail.nit_ticket
  return detail.nit_ticket
}

/* ------------------------- The developer's decisions -------------------------- */

/*
 * **`crates/store/src/entities/decision.rs`'s own three tables, restated once here and
 * nowhere else.**
 *
 * This is the one place in `web/` where sample mode has to reproduce a *rule* rather than
 * patch a record. The eight decisions are not eight branches: each settles a disposition, a
 * round status and exactly one outcome, and separately each either needs a block of the
 * draft that is already there, writes one that is not, or neither. Written as branches in a
 * handler, "every decision leaves exactly one outcome" would be a sentence nobody could
 * check; written as tables, a decision added to the contract is a type error here.
 *
 * The reason it matters that these agree with the store and not merely look like it: a
 * decision that works on fixtures and is refused by the binary is the worst possible shape
 * of this file. `test/desk.spec.ts` asserts the three tables against the contract's own
 * `FindingDecision` description, which carries the same table in prose.
 */

/** What one decision settles, with no reference to the record. */
interface Settled {
  /** Where the finding stands in this round's draft afterwards, or null to leave it. */
  disposition: Disposition | null
  /** What became of the finding this round, or null to leave it. */
  round_status: RoundStatus | null
  /** The outcome kind it is recorded as. Exactly one, always. */
  outcome: OutcomeKind
}

const SETTLES: Record<FindingDecision, Settled> = {
  keep: { disposition: 'in_draft', round_status: null, outcome: 'kept' },
  cut: { disposition: 'suppressed', round_status: null, outcome: 'cut' },
  regrade: { disposition: null, round_status: null, outcome: 'regraded' },
  rewrite: { disposition: null, round_status: null, outcome: 'rewritten' },
  to_ticket: { disposition: 'held_back', round_status: 'ticketed', outcome: 'ticketed' },
  raise_again: { disposition: 'in_draft', round_status: 'still_open', outcome: 'raised_again' },
  /*
   * `rejected`, and the reason is the detail. That is exactly what
   * `RuleCounts.rejected_with_reason` counts — "author disagreed once, with a reason you
   * accepted" — so it has to be that kind rather than one of its own nothing would read.
   */
  accept_reason: { disposition: 'suppressed', round_status: 'accepted', outcome: 'rejected' },
  withdraw: { disposition: 'suppressed', round_status: 'withdrawn', outcome: 'withdrawn' },
}

/** The decisions that write a block for a finding the draft has none for. */
const WRITES_A_BLOCK: FindingDecision[] = ['keep', 'raise_again']

/** The decisions that need a block that is already there — they change words. */
const NEEDS_A_BLOCK: FindingDecision[] = ['cut', 'regrade', 'rewrite']

/** The round statuses that mean the finding is still open. */
const STILL_OPEN: RoundStatus[] = ['new', 'still_open']

/**
 * Recompose the body from the blocks, and clear the lint.
 *
 * **The lint is cleared on every change to the body, and that is the rule rather than a
 * convenience.** The review lint was run by the agent over the body it wrote; the moment a
 * decision changes that body the result is about a different document, and a lint still
 * saying "passes" would be the record vouching for words nobody checked.
 *
 * Blocks are joined by a blank line and the body ends in one newline. A composition with
 * nothing to post is the empty string rather than a lone newline.
 */
function recompose(drafted: DraftDetail, owner: ReviewDetail): void {
  for (const section of drafted.sections) {
    if (section.kind === 'prose') {
      section.posted = true
      continue
    }
    const finding = owner.findings.find(item => item.seq === section.finding_seq)
    /* Derived, never stored: regrading the finding changes the heading it composes. */
    section.severity = finding?.severity ?? section.severity
    section.posted = finding != null && isPosted(finding)
    section.rewritten = section.text !== section.agent_text
      || (section.title ?? null) !== (section.agent_title ?? null)
    section.heading = section.title == null || section.severity == null
      ? null
      : findingHeading(section.severity, section.finding_seq!, section.title)
  }

  /*
   * **The body and the threads together, because a decision changes both.** An anchored
   * finding is a row in the body's index and a comment on the code, so cutting one has to
   * take its row and its thread in the same step — `houseStyle.ts` is the shape, and the
   * binary's `house_style.rs` is what it mirrors.
   */
  drafted.draft.body = composeBody(owner.review, drafted.sections, owner.findings)
  drafted.threads = composeThreads(drafted.sections, owner.findings)
  drafted.draft.updated_at = nowIso()
  /* Not relint(): a decision leaves the lint **not checked**, never passing. */
  drafted.lint = null
}

/**
 * The refusals a decision can earn from the request alone, before the record is touched.
 *
 * Every one of them is `decision.rs`'s `check`, in the same order, so that a request the
 * binary refuses is refused here too — and with the same `field`, because the page reads
 * `field` to decide which control to put the message beside.
 */
function checkDecision(request: DecideFindingRequest): void {
  const blank = (text: string | null | undefined) => (text ?? '').trim().length === 0

  if (request.decision === 'regrade' && request.severity == null) {
    throw invalidValue(
      'severity',
      'Choose the severity to post it at. Regrading is the decision that changes one.',
      'blocker, should, nit or question',
    )
  }

  if (request.decision === 'rewrite' && request.title == null && request.text == null) {
    throw invalidValue(
      'text',
      'A rewrite needs words: the heading, the paragraph, or both.',
      'the heading\'s title, the paragraph, or both — in your own words, which is what '
      + 'gets posted',
    )
  }

  if (request.decision === 'rewrite') {
    if (request.title != null && blank(request.title)) {
      throw invalidValue(
        'title',
        'The heading cannot be blank.',
        'the title the heading posts',
      )
    }
    if (
      request.text != null
      && (request.text.startsWith('\n') || request.text.endsWith('\n'))
    ) {
      throw invalidValue(
        'text',
        'The paragraph begins or ends with a newline.',
        'the paragraph itself; blocks are joined by one blank line, and a block that '
        + 'brought its own would compose a body nobody wrote',
      )
    }
  }

  if (request.decision === 'accept_reason' && blank(request.note)) {
    throw invalidValue(
      'note',
      'Say what the author’s reason was.',
      'the author’s reason, in their words. A rejection with no reason is the one '
      + 'outcome the feedback loop cannot learn from',
    )
  }

  /*
   * Words belong to a rewrite, and to the two decisions that may have to write a block the
   * draft has none of. **Refused rather than ignored** on the other five: an API that
   * accepts a field and does nothing with it is one a caller cannot tell they misused.
   */
  if (
    (request.title != null || request.text != null)
    && !['rewrite', 'keep', 'raise_again'].includes(request.decision)
  ) {
    throw invalidValue(
      'text',
      `Words on a ${request.decision} decision.`,
      'no title and no text on this decision; rewriting what gets posted is its own '
      + 'decision, and this one takes the finding out of the post',
    )
  }

  if (request.severity != null && request.decision !== 'regrade') {
    throw invalidValue(
      'severity',
      `A severity on a ${request.decision} decision.`,
      'no severity on this decision; changing one is a regrade, so that the record says '
      + 'the developer regraded it',
    )
  }
}

/** What the outcome says beyond its kind: `decision.rs`'s `detail_for`. */
function decisionDetail(
  request: DecideFindingRequest,
  finding: Finding,
): string | null {
  const note = (request.note ?? '').trim()
  const withNote = (change: string) => (note.length > 0 ? `${change}: ${note}` : change)

  if (request.decision === 'regrade' && request.severity != null) {
    return withNote(
      `${SEVERITY_LABELS[finding.severity]} to ${SEVERITY_LABELS[request.severity]}`,
    )
  }
  if (request.decision === 'rewrite') {
    const parts: string[] = []
    if (request.title != null) parts.push('the heading')
    if (request.text != null) parts.push('the paragraph')
    return withNote(`rewrote ${parts.join(' and ')}`)
  }
  if (request.title != null || request.text != null) {
    return withNote('posted in the developer\'s own words')
  }
  return note.length > 0 ? note : null
}

/**
 * `POST /reviews/{id}/findings/{seq}/decide` — the developer's decision, and the draft
 * recomposed.
 *
 * One step, like the store's one transaction: the severity, the disposition, the round
 * status, the block and the body are all written before anything is read back, and the
 * three things handed over are one instant of the record.
 */
function decideFinding(
  reviewId: number,
  seq: number,
  request: DecideFindingRequest,
): FindingDecisionMade {
  const owner = reviewById(reviewId)
  const finding = owner?.findings.find(item => item.seq === seq)
  if (!owner || !finding) throw notFound(`finding ${seq} on review ${reviewId}`)

  /* Computed from the request alone, before anything is written. */
  checkDecision(request)

  const drafted = draftById(reviewId)
  if (!drafted) throw notFound(`draft on review ${reviewId}`)

  if (drafted.draft.status === 'approved' || drafted.draft.status === 'posted') {
    throw conflict(
      `This draft is ${drafted.draft.status} and cannot be decided about: an approved `
      + 'draft is what the review session posts under your name.',
    )
  }

  const held = drafted.sections.find(
    section => section.kind === 'finding' && section.finding_seq === seq,
  )

  if (NEEDS_A_BLOCK.includes(request.decision) && !held) {
    throw conflict(
      `the draft of review ${reviewId} has no block for ${finding.key}, so there is `
      + 'nothing to decide about: it was written as one block of markdown rather than '
      + 'in parts',
    )
  }

  /*
   * **Keeping a finding this round has already settled is refused rather than quietly
   * reopened.** "Keep" means *post this as it stands*, and a finding carried as fixed,
   * accepted, withdrawn or ticketed is not posted as an open finding. Raising it again is
   * the decision that says it is still open, and it is one button away.
   */
  if (request.decision === 'keep' && !STILL_OPEN.includes(
    (finding.round_status ?? 'new') as RoundStatus,
  )) {
    throw invalidValue(
      'decision',
      `${finding.key} is ${finding.round_status} this round.`,
      'a finding whose round status is new or still open. Raise it again if this round '
      + 'is to say it is still there',
    )
  }

  /* The developer's own words belong to a rewrite once the block exists. */
  if (held && request.decision !== 'rewrite' && (request.title != null || request.text != null)) {
    throw invalidValue(
      'text',
      `${finding.key} already has a block in this draft.`,
      'no words on this decision; rewriting what gets posted is its own decision, so '
      + 'that the record says the developer rewrote it',
    )
  }

  const settled = SETTLES[request.decision]
  const detail = decisionDetail(request, finding)

  /* The severity first, because the heading is composed from it. */
  if (request.severity != null) finding.severity = request.severity

  if (settled.disposition != null) {
    finding.disposition = settled.disposition
    finding.disposition_reason = (request.note ?? '').trim() || null
  }

  if (settled.round_status != null) {
    finding.round_status = settled.round_status
    finding.round_status_by = 'developer'
    finding.round_status_checked = (request.note ?? '').trim() || null
    finding.round_status_at = nowIso()
  }

  if (request.decision === 'rewrite' && held) {
    if (request.title != null) held.title = request.title.trim()
    if (request.text != null) held.text = request.text
  }
  else if (WRITES_A_BLOCK.includes(request.decision) && !held) {
    /*
     * The words a new block is written with. The agent's own title and paragraph are kept
     * beside the developer's, so the record can say whose words were posted — and a finding
     * with neither a consequence nor a comment is refused rather than given an invented
     * paragraph.
     */
    /*
     * **The finding's own title, and its claim only when it has none** — which is
     * `decision.rs`'s `words_for`, and was the claim here while the store used the title.
     * The two composed different headings for the same press, which is this file's whole
     * failure mode: `### Nit · F2 · <56 words>` on fixtures and `### Nit · F2 · <title>`
     * against the binary.
     */
    const agentTitle = finding.title ?? finding.claim
    const agentText = [finding.why_it_matters, finding.inline_comment]
      .find(text => (text ?? '').trim().length > 0)
    if (agentText == null) {
      throw invalidValue(
        'text',
        `${finding.key} has no paragraph to post.`,
        'what the review is to say about it; the finding records no consequence and no '
        + 'comment for the author, and nothing may invent one',
      )
    }
    const title = (request.title ?? '').trim() || agentTitle
    const text = (request.text ?? '').trim().length > 0 ? request.text! : agentText
    drafted.sections.push({
      position: drafted.sections.length + 1,
      kind: 'finding',
      finding_seq: seq,
      finding_key: finding.key,
      severity: finding.severity,
      heading: findingHeading(finding.severity, seq, title),
      title,
      text,
      agent_title: agentTitle,
      agent_text: agentText,
      rewritten: title !== agentTitle || text !== agentText,
      posted: true,
    })
  }

  /* The body last, so it is composed from everything the decision changed. */
  retally(owner)
  syncHeldBack(owner, drafted)
  recompose(drafted, owner)

  const outcome = {
    id: nextOutcomeId(),
    review_id: reviewId,
    finding_seq: seq,
    finding_key: finding.key,
    kind: settled.outcome,
    actor: 'developer' as const,
    occurred_at: nowIso(),
    detail,
  }
  owner.outcomes.push(outcome)

  return { finding, outcome, draft: drafted }
}

/**
 * `POST /reviews/{id}/draft/ticket-and-approve` — file what is left, and approve.
 *
 * **A sign-off, and it neither posts nor files anything.** Every finding the draft would
 * have posted is held back and recorded as ticketed this round by the developer, the one
 * nit ticket is asked for, the body is recomposed without them, and the draft is approved
 * with the event `approve` — which is not the caller's to choose, because a request for
 * changes that ticketed everything it asked for would contradict itself.
 */
function ticketRestAndApprove(id: number, note: string | null): TicketedAndApproved {
  const drafted = draftById(id)
  const owner = reviewById(id)
  if (!drafted || !owner) throw notFound(`draft on review ${id}`)
  refuseIfDismissed(id, 'ticketing what is left and approving')

  if (drafted.draft.status !== 'ready') {
    throw conflict(
      `This draft is ${drafted.draft.status} and only a draft that is ready can be `
      + 'approved.',
    )
  }

  const open = owner.findings.filter(isPosted)
  if (open.length === 0) {
    throw invalidValue(
      'review_id',
      `Review ${id} has no open finding to file.`,
      'a review with an open finding; sign the draft off instead, and ask for a ticket '
      + 'for the nits held back on its own',
    )
  }

  const said = (note ?? '').trim() || null
  const at = nowIso()
  for (const finding of open) {
    finding.disposition = 'held_back'
    finding.disposition_reason = said
    finding.round_status = 'ticketed'
    finding.round_status_by = 'developer'
    finding.round_status_checked = said
    finding.round_status_at = at
    owner.outcomes.push({
      id: nextOutcomeId(),
      review_id: id,
      finding_seq: finding.seq,
      finding_key: finding.key,
      kind: 'ticketed',
      actor: 'developer',
      occurred_at: at,
      detail: said,
    })
  }

  retally(owner)
  owner.nit_ticket = {
    ...owner.nit_ticket,
    status: 'requested',
    finding_seqs: owner.findings
      .filter(finding => finding.disposition === 'held_back')
      .map(finding => finding.seq),
    requested_at: owner.nit_ticket.requested_at ?? at,
  }
  syncHeldBack(owner, drafted)
  recompose(drafted, owner)

  drafted.draft.status = 'approved'
  drafted.draft.event = 'approve'
  drafted.draft.approved_at = at
  drafted.draft.sent_back_note = null
  drafted.draft.sent_back_at = null
  owner.draft_status = 'approved'

  return {
    draft: drafted,
    nit_ticket: owner.nit_ticket,
    ticketed: open.map(finding => finding.seq),
    ticketed_keys: open.map(finding => finding.key),
  }
}

/** The stage that follows this one, or `null` at the end of the line. */
function nextStage(stage: RuleStage): RuleStage | null {
  if (stage === 'proposed') return 'active'
  if (stage === 'active') return 'lint'
  if (stage === 'lint') return 'ci'
  return null
}

/**
 * How the Rules screen words each stage.
 *
 * Keyed by `RuleStage` rather than written as a chain of ternaries, for the reason `VERBS`
 * in `utils/format.ts` gives: a stage added to the contract is then a type error here
 * instead of a rule the screen has no word for. `active` is the one that depends on more
 * than the stage, so it asks.
 */
const STAGE_WORDS: Record<RuleStage, (rule: Rule) => string> = {
  proposed: () => 'Proposed',
  active: rule => (rule.scope.kind === 'global' ? 'Global rule' : 'Repository rule'),
  lint: () => 'Lint check',
  ci: () => 'CI check',
  retired: () => 'Retired',
}

/** How the Rules screen words a stage, with the exceptions the server appends. */
function stageLabel(rule: Rule, stage: RuleStage): string {
  const base = STAGE_WORDS[stage](rule)
  const exceptions = rule.counts.exceptions ?? 0
  return exceptions > 0 ? `${base} · ${count(exceptions, 'exception')}` : base
}

function ruleById(id: number): Rule {
  const rule = source.rules.find(item => item.id === id)
  if (!rule) throw notFound(`rule ${id}`)
  return rule
}

/** `POST /rules/{id}/promote` — the developer promotes; the app never promotes itself. */
function promoteRule(id: number, toStage: RuleStage): Rule {
  const rule = ruleById(id)
  const next = nextStage(rule.stage)
  if (next !== toStage) {
    throw conflict(
      `A rule at ${rule.stage} cannot be promoted to ${toStage}: `
      + (next == null ? 'there is no stage after it.' : `the stage that follows is ${next}.`),
    )
  }

  rule.stage = toStage
  rule.why_proposed = null
  rule.stale = false
  rule.stage_label = stageLabel(rule, toStage)
  rule.note = 'Promoted by you. Nothing cites it yet.'
  rule.counts = { ...rule.counts, raised: rule.counts.raised ?? 0, kept: rule.counts.kept ?? 0 }
  return rule
}

/** The reason is required on both, so a rule is not proposed again next week. */
function retireWithReason(id: number, reason: string, what: string): Rule {
  const trimmed = reason.trim()
  if (trimmed.length === 0) {
    throw invalidValue(
      'reason',
      `A rule ${what} with no reason gets proposed again next week.`,
      'why',
    )
  }

  const rule = ruleById(id)
  rule.stage = 'retired'
  rule.stale = false
  rule.stage_label = 'Retired'
  rule.note = `${what === 'dismissed' ? 'Dismissed' : 'Retired'} by you: ${trimmed}`
  return rule
}

/**
 * `POST /reviews/{id}/dismiss` — take a review out of play, with a reason.
 *
 * Refused exactly where the binary refuses: a blank reason is `invalid_value` naming
 * `reason`, and a review already dismissed is `conflict`. A screen's refusal state tested
 * on fixtures is then one that works against the record.
 */
function dismissReview(id: number, reason: string): Review {
  const detail = reviewById(id)
  if (!detail) throw notFound(`review ${id}`)

  const trimmed = reason.trim()
  if (trimmed.length === 0) {
    throw invalidValue(
      'reason',
      'A review dismissed with no reason is one nobody can say why they hid.',
      'why this review is being dismissed',
    )
  }
  if (detail.review.dismissed_at != null) {
    throw conflict(
      `Review ${id} is already dismissed. Restore it first to dismiss it again with `
      + 'different words.',
    )
  }

  detail.review.dismissed_at = new Date().toISOString().replace(/\.\d+Z$/, 'Z')
  detail.review.dismissed_reason = trimmed
  return detail.review
}

/** `POST /reviews/{id}/restore` — bring a dismissed review back. */
function restoreReview(id: number): Review {
  const detail = reviewById(id)
  if (!detail) throw notFound(`review ${id}`)
  if (detail.review.dismissed_at == null) {
    throw conflict(`Review ${id} is not dismissed, so there is nothing to bring back.`)
  }

  /* Both together: a reason with no time reads as in play to every query. */
  detail.review.dismissed_at = null
  detail.review.dismissed_reason = null
  return detail.review
}

/**
 * `DELETE /reviews/{id}` — remove a review that never got past being requested.
 *
 * **The same three things the store asks about**, and in the same words, because the
 * refusal is what the developer reads: no brief, no finding, no draft. A fourth thing
 * checked here and not there — or a different sentence — is how a page comes to offer a
 * button the binary refuses.
 */
function deleteReview(id: number): Review {
  const detail = reviewById(id)
  if (!detail) throw notFound(`review ${id}`)

  const held = {
    brief: detail.brief != null,
    findings: detail.findings.length,
    /*
     * **`draft_status`, which is what the card reads and what the server derives this
     * from.** Asking `draftById` instead was a second way to answer one question, and
     * the fixtures already disagreed with it: reviews 18 and 19 carry a `draft_status`
     * with no entry in `drafts.json`, so a review like them holding no brief would have
     * had the card offer a delete that this function refused — the drift both files say
     * must not happen.
     */
    draft: detail.draft_status != null,
    posted: detail.review.github_review_id != null,
  }
  const holds = reviewWorkInWords(held)
  if (holds !== '') {
    /* "lose that" or "lose them", through the helper the binary's own message uses. */
    const lost = reviewWorkIsOneThing(held) ? 'that' : 'them'
    throw conflict(
      `Review ${id} holds ${holds}, and deleting it would lose ${lost}: dismiss it `
      + 'instead, which takes it out of the queue and keeps the record.',
    )
  }

  const went = detail.review
  source.reviews.splice(source.reviews.indexOf(detail), 1)
  return went
}

/** `POST /rules/{id}/defer` — keep it, and ask again later. */
function deferRule(id: number, days: number): Rule {
  if (!Number.isInteger(days) || days < 1 || days > 365) {
    throw invalidValue(
      'days',
      `${days} is not a period to keep a rule for.`,
      'a whole number of days from 1 to 365',
    )
  }

  const rule = ruleById(id)
  rule.stale = false
  rule.review_again_at = new Date(Date.now() + days * 86_400_000).toISOString()
  rule.note = `Kept for ${count(days, 'day')}. Asked about again after that.`
  return rule
}

/**
 * A read's answer, as a plain object nothing else holds a reference to.
 *
 * **The API hands back a fresh object every read, because it parses fresh JSON, and this
 * has to as well.** Without the clone a read returned the live reactive record the writes
 * mutate in place, which made sample mode behave differently from the server in a way no
 * test was looking at: a component watching `() => props.finding` to notice that its
 * finding had changed never fired, because the object was the same one. The two backends
 * are meant to be indistinguishable, and identity is part of that.
 *
 * It also closes a second hole. A screen handed the live record could have written to it —
 * reaching past the data layer into the fixtures, which is the one thing
 * `test/architecture.spec.ts` exists to prevent and could not have caught.
 *
 * **A JSON round trip rather than `structuredClone`**, and not only because the latter
 * throws on a Vue proxy: this is precisely what the real answer goes through, so sample
 * mode now differs from the API in no way a caller could detect — including on the fields
 * the wire cannot carry.
 */
function answered<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

/**
 * Note that a review has just been written to, and hand back what the write made.
 *
 * **`last_activity_at` moves on every write and on no read**, which the contract states and
 * these records did not do: the stamp sat at whatever the fixture held, so sample mode read
 * "last written to 4 days ago" one second after a sign-off. That is the number the draft
 * page's `PostingWatch` shows while it waits for a session to post, so an invented record
 * that never moved it would have shown the one thing the screen exists to report as a lie.
 *
 * Wrapped around the eight review-scoped writes **where they are listed**, rather than
 * added inside each of them. That does not make the rule structural — a ninth write can
 * still be added without it — and the claim that it did was wrong when it was written
 * here. What the list buys is that all eight are one screenful, so the odd one out is
 * visible; what it costs is that nothing fails when somebody adds the ninth. The Rust side
 * guards the same rule with a test that walks every write; this side has one test, over
 * `approveDraft`, in `test/desk.spec.ts`.
 */
function wrote<T>(reviewId: number, made: T): T {
  const owner = reviewById(reviewId)
  if (owner) owner.timings.last_activity_at = nowIso()
  return made
}

/**
 * The twenty-two operations over the records in memory.
 *
 * Every one is `async` so that the composable above cannot tell which backend it has. They
 * resolve immediately — there is nothing to wait for — which means a loading state is not
 * exercised here; `test/api.spec.ts` exercises that against `api.ts` with a stubbed
 * `fetch`, where it is real.
 */
export function sampleBackend(): DeskBackend {
  return {
    isSampleData: true,

    getInbox: async () => answered(inbox()),
    listReviews: async (repo, dismissed = 'excluded') =>
      answered(reviewList(repo, dismissed)),
    getReview: async (id) => {
      const detail = reviewById(id)
      if (!detail) throw notFound(`review ${id}`)
      return answered(detail)
    },
    getDraft: async (id) => {
      const detail = draftById(id)
      if (!detail) throw notFound(`draft on review ${id}`)
      return answered(detail)
    },
    listRules: async repo => answered(rulesPage(repo)),
    listRepos: async (): Promise<RepoSummary[]> =>
      answered(source.repos.map(repo => ({
        ...repo,
        /* `inScope`, not the rule written out again: one answer to "is this repository's". */
        active_rules: source.rules.filter(rule => isActive(rule) && inScope(rule, repo.repo))
          .length,
      }))),

    startReview: async link => answered(startReview(link)),
    confirmBrief: async id => answered(wrote(id, confirmBrief(id))),
    setFindingDisposition: async (reviewId, seq, disposition, reason) =>
      answered(wrote(reviewId, setFindingDisposition(reviewId, seq, disposition, reason))),
    decideFinding: async (reviewId, seq, request) =>
      answered(wrote(reviewId, decideFinding(reviewId, seq, request))),
    saveDraft: async (id, body, edits) => answered(wrote(id, saveDraft(id, body, edits))),
    approveDraft: async (id, event) => answered(wrote(id, approveDraft(id, event))),
    sendBackDraft: async (id, note) => answered(wrote(id, sendBackDraft(id, note))),
    ticketRestAndApprove: async (id, note) =>
      answered(wrote(id, ticketRestAndApprove(id, note))),
    requestNitTicket: async (id, ticketId = null) =>
      answered(wrote(id, requestNitTicket(id, ticketId))),
    /*
     * **Dismissing and restoring are writes to the review and move the stamp**, which is
     * what the store does: both call `mark_review_active`, and its own comment argues
     * for it against the reading that a developer's act is not a session's. Deleting is
     * not wrapped: the review is gone, so there is nothing left to stamp — `wrote` would
     * look up a row it has just removed and quietly do nothing.
     */
    dismissReview: async (id, reason) => answered(wrote(id, dismissReview(id, reason))),
    restoreReview: async id => answered(wrote(id, restoreReview(id))),
    deleteReview: async id => answered(deleteReview(id)),
    promoteRule: async (id, toStage) => answered(promoteRule(id, toStage)),
    dismissRule: async (id, reason) => answered(retireWithReason(id, reason, 'dismissed')),
    retireRule: async (id, reason) => answered(retireWithReason(id, reason, 'retired')),
    deferRule: async (id, days) => answered(deferRule(id, days)),

    /* Nothing to be unreachable. Sample mode is always answering. */
    health: async () => ({ status: 'ok', schema_version: 0 }),
  }
}
