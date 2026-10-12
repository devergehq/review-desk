/*
 * The data layer. Every screen reads and writes the desk through this composable and
 * through nothing else.
 *
 * The reads mirror the ones the API offers — one GET per screen, so no screen joins
 * anything:
 *
 *   inbox()          GET /inbox
 *   reviews(repo)    GET /reviews?repo=
 *   review(id)       GET /reviews/{id}
 *   draft(id)        GET /reviews/{id}/draft
 *   rules(repo)      GET /rules?repo=
 *   repos()          GET /repos
 *
 * and the writes mirror its sixteen writes, one function each. Which records they reach
 * — the Rust server, or invented ones in memory — is `app/data/source.ts`'s decision, and
 * nothing above this file can tell.
 *
 * ## Reads are resources, not values
 *
 * A read over HTTP has three states a fixture never had: it is in flight, it failed, or it
 * answered. So a read hands back a [`DeskResource`] rather than a value, and a screen
 * renders it through `<DeskState>` — which is what put loading, "nothing here yet", and
 * "the server is not answering, retry" on all five screens from one place rather than five.
 *
 * ## A write refreshes what is on screen, and the screen does not ask it to
 *
 * Every resource registers itself while it is alive, and a write that succeeds refreshes
 * all of them. That is one GET per open screen — the contract's own shape — and it is why
 * no page carries a line of "now reload the thing I just changed", which is the kind of
 * boilerplate that gets forgotten on the one path nobody clicked.
 *
 * ## Nothing is posted to GitHub from here
 *
 * The developer approves a draft and the review session posts it — that is the whole point
 * of the preview.
 */

import {
  computed,
  isRef,
  onScopeDispose,
  ref,
  shallowRef,
  watch,
  type ComputedRef,
  type Ref,
} from 'vue'

import type { DeskBackend, DraftEditToRecord } from '~/data/backend'

import { asDeskError } from '~/data/fault'
import { chooseSource, loadBackend, sampleWanted, type DeskSourceName } from '~/data/source'
import { detectEdits, findingsInBody } from '~/utils/draft'
import { count, startedReviewLine } from '~/utils/format'
import type {
  Brief,
  DecideFindingRequest,
  DismissedReviews,
  FindingDecision,
  DeskError,
  Disposition,
  DraftDetail,
  Finding,
  FindingDecisionMade,
  Inbox,
  NitTicket,
  RepoName,
  RepoSummary,
  Review,
  ReviewDetail,
  ReviewEvent,
  ReviewList,
  Rule,
  RuleStage,
  RulesPage,
  TicketedAndApproved,
} from '~/types/desk'

/** The repository filter's value that means every repository. */
export const ALL_REPOSITORIES = 'all'

/** A disposition in a sentence about what just happened to a finding. */
const DISPOSITIONS: Record<Disposition, string> = {
  in_draft: 'in the draft',
  held_back: 'held back for a ticket',
  suppressed: 'suppressed',
}

/**
 * What each decision came to, in the developer's own terms.
 *
 * Keyed by `FindingDecision` rather than written as a chain of ternaries, for the reason
 * `DISPOSITIONS` above and `VERBS` in `utils/format.ts` are: a ninth decision added to the
 * contract is then a type error here instead of a decision the screen has no word for.
 *
 * Each sentence says what the *record* now holds, not what the button said — "is in the
 * review again" rather than "raised again" — because the notice is read after the fact.
 */
const DECIDED: Record<FindingDecision, string> = {
  keep: 'stays in the review, and its comment posts on its line.',
  cut: 'will not be posted. It stays on the record, marked as cut by you.',
  regrade: 'will be posted at the severity you chose.',
  rewrite: 'will be posted in your words. The session’s own are kept beside them.',
  to_ticket: 'has left the review for the ticket.',
  raise_again: 'is in the review again, as still open this round.',
  accept_reason: 'is closed: you accepted the author’s reason, and the reason is recorded.',
  withdraw: 'is withdrawn. The record says you withdrew it, not that it was wrong.',
}

/* --------------------------- The records behind it --------------------------- */

/**
 * The backend, resolved once for the whole page.
 *
 * A promise rather than a value, because sample mode is reached by a dynamic import and so
 * cannot be had synchronously. Shared, so five screens do not each load it.
 */
let backend: Promise<DeskBackend> | null = null

function records(): Promise<DeskBackend> {
  backend ??= loadBackend()
  return backend
}

/* ------------------------------- Resources ---------------------------------- */

/** A read, with the three states a read has. */
export interface DeskResource<T> {
  /** What the server answered, or `null` while that is not known. */
  data: Ref<T | null>
  /** Whether a read is in flight. */
  pending: Ref<boolean>
  /** Why the last read failed, or `null`. */
  error: Ref<DeskError | null>
  /** Whether a read has ever finished, which is what tells a first load from a refresh. */
  loaded: Ref<boolean>
  /** Read it again. Behind the retry on an error, and run after every successful write. */
  refresh: () => Promise<void>
}

/**
 * Every live resource, so that a write can refresh what is on screen.
 *
 * A set of the refresh functions themselves rather than of the resources: nothing here
 * needs to read them, only to ask them to read again.
 */
const live = new Set<() => Promise<void>>()

/** Refresh every resource a screen is currently showing. */
async function refreshEverything(): Promise<void> {
  await Promise.all([...live].map(refresh => refresh()))
}

/**
 * Resources more than one component on a page wants, and the count of who still wants them.
 *
 * **One read, however many ask for it.** The layout used to read the inbox to know which
 * review to link "Reviews" at while the queue read it to draw itself, so the queue issued
 * `GET /inbox` twice on load and twice more after every write, for one document that is
 * the same both times. DEV-930 pointed that item at `/reviews` and deleted the layout's
 * read, so **the double read this was written for is gone** — recorded here rather than
 * left as a reason a reader would check and not find. What it still buys is the other
 * half of the argument, which has no second reader to depend on: two resources over one
 * document can answer differently for as long as one of them is in flight.
 *
 * Counted rather than kept forever: the last component to go takes the resource with it, so
 * a long-lived page does not hold a refresher nothing is watching.
 */
const shared = new Map<string, { resource: DeskResource<unknown>, wanted: number }>()

/**
 * One read, as a resource.
 *
 * It loads immediately. `keep` is what the resource shows while a *refresh* is in flight:
 * the previous answer, so a write does not blank the screen it just changed. A first load
 * has nothing to keep, which is the state `<DeskState>` draws as loading.
 */
function resource<T>(read: (from: DeskBackend) => Promise<T>): DeskResource<T> {
  const data = shallowRef<T | null>(null)
  const pending = ref(false)
  const error = ref<DeskError | null>(null)
  const loaded = ref(false)

  /*
   * Only the newest read may write the result. Without this, a slow first read answering
   * after a fast refresh would overwrite the newer answer with the older one — which on
   * this app would show a draft the developer had just edited as it was before the edit.
   */
  let generation = 0

  async function refresh(): Promise<void> {
    const mine = ++generation
    pending.value = true
    try {
      const answer = await read(await records())
      if (mine !== generation) return
      data.value = answer
      error.value = null
      loaded.value = true
    }
    catch (thrown) {
      if (mine !== generation) return
      error.value = asDeskError(thrown)
      /*
       * The last good answer is kept rather than cleared. A server that stops while a page
       * is open should show a message over what the developer was reading, not an empty
       * page — and `<DeskState>` does exactly that when there is data and an error.
       */
      loaded.value = true
    }
    finally {
      if (mine === generation) pending.value = false
    }
  }

  live.add(refresh)
  onScopeDispose(() => {
    live.delete(refresh)
  })

  void refresh()
  return { data, pending, error, loaded, refresh }
}

/** What a write answers: a sentence for the screen, or the refusal to show. */
export type WriteResult
  = | { ok: true, message: string }
    | { ok: false, error: DeskError }

/**
 * Run a write, then refresh what is on screen.
 *
 * The refresh is deliberately part of the write rather than of the screen. `PUT …/draft`
 * re-runs the review lint and `PATCH …/findings/{seq}` changes the held-back list the
 * draft screen shows, so the answer to "what does the record say now" is a read, not the
 * write's return value — and a screen that patched its own copy would drift from the
 * record it is a preview of.
 */
async function write<T>(
  act: (to: DeskBackend) => Promise<T>,
  said: (made: T) => string,
  /**
   * Whether to read every open screen again afterwards. True for every write but one.
   *
   * **The exception is the write whose subject is gone when it returns.** A refresh after
   * a delete re-reads `GET /reviews/{id}` for the review that has just been removed, and
   * the screen that asked for it is still mounted — so it draws "no review with id N. It
   * may have been opened on another database" over the page, for the half-second before
   * the navigation. The caller leaves for the Reviews list instead, which reads on mount,
   * so nothing is left stale by not refreshing here.
   */
  refresh = true,
): Promise<WriteResult> {
  let made: T
  try {
    made = await act(await records())
  }
  catch (thrown) {
    return { ok: false, error: asDeskError(thrown) }
  }

  if (refresh) await refreshEverything()
  return { ok: true, message: said(made) }
}

/**
 * One sentence, left for the next screen to say.
 *
 * **For an act whose own screen does not survive it.** Deleting a review takes the
 * developer to the Reviews list, and the sentence about what went — the only
 * acknowledgement an irreversible act gets — belongs where they land rather than on a
 * page that is unmounting. Taken once: a reload or a second visit says nothing, because
 * it is a receipt and not a state.
 */
let handedOver: string | null = null

/** Leave a sentence for the next screen. */
export function handOver(said: string): void {
  handedOver = said
}

/** Take the sentence the last screen left, if there is one. */
export function takeHandOver(): string | null {
  const said = handedOver
  handedOver = null
  return said
}

/**
 * Drop a sentence nobody came to read.
 *
 * **Because it is module state and the screen that consumes it may never be reached.** A
 * delete navigates to the Reviews list, which takes it — but a test that deletes and then
 * mounts something else, or a switch to the invented records, would leave it armed and
 * the next Reviews list would say a review from a database it is no longer reading had
 * been deleted. Called wherever the records change underneath it.
 */
function forgetHandOver(): void {
  handedOver = null
}

/* ------------------------------ The composable ------------------------------- */

export function useReviewDesk() {
  /*
   * Answered without awaiting the backend, because the "Sample data" marker is in the
   * header on every screen and a marker that appeared a frame late would be one nobody
   * could rely on.
   */
  const isSampleData = computed(() => sampleWanted())

  /**
   * Whether invented records are an option at all.
   *
   * A compile-time constant, so it is `false` in a built dashboard and the control that
   * reads it never renders — there would be nothing to switch to, because the fixtures are
   * not in the bundle. Exposed from here rather than read in a component, so that no
   * screen knows which records it is showing: a screen that branched on it would behave
   * differently on fixtures than on the record, which is the one thing sample mode must
   * not do.
   */
  const sampleAvailable = computed(() => import.meta.env.DEV)

  /**
   * Read the invented records instead, and reload so every screen does.
   *
   * Offered on the screen that says the server is not answering, which is the moment it is
   * worth anything: a developer who has not started the binary can still see what they
   * were working on.
   */
  function useSampleRecords(): void {
    chooseSource('sample')
    backend = null
    forgetHandOver()
    try {
      window.location.reload()
    }
    catch {
      /* No window. Nothing to reload, and the choice holds for whatever comes next. */
    }
  }

  /* ------------------------------- The reads ------------------------------- */

  /**
   * `GET /inbox` — the five waiting groups, the reviews in progress, the posted reviews
   * waiting on their author, and the week's figures.
   */
  function inbox(): DeskResource<Inbox> {
    return sharing('inbox', () => resource(from => from.getInbox()))
  }

  /**
   * `GET /reviews?repo=` — every review, newest first, or one repository's.
   *
   * The filter is reactive, the way `rules` is and for the same reason: changing the
   * picker is a new read rather than a filter over a list the dashboard already holds, so
   * the picker cannot come to disagree with the server about what "this repository" means.
   */
  function reviews(
    repo: Ref<string> | ComputedRef<string>,
    dismissed?: Ref<DismissedReviews> | ComputedRef<DismissedReviews>,
  ): DeskResource<ReviewList> {
    /*
     * Both filters are watched as one, because `following` takes one value: a page with
     * two pickers over one read has to re-read when either moves, and two resources over
     * the one document can answer differently while one of them is in flight — which is
     * the argument `shared` above makes for the Inbox.
     */
    const asked = computed(() => ({
      repo: repo.value,
      dismissed: dismissed?.value ?? ('excluded' as DismissedReviews),
    }))
    return following(asked, (from, value) =>
      from.listReviews(
        value.repo === ALL_REPOSITORIES ? null : (value.repo as RepoName),
        value.dismissed,
      ))
  }

  /** `GET /reviews/{id}` — one review whole, as the One review screen shows it. */
  function review(id: number | Ref<number> | ComputedRef<number>): DeskResource<ReviewDetail> {
    return following(id, (from, value) => from.getReview(value))
  }

  /** `GET /reviews/{id}/draft` — the draft body, its checks, the edits and the nits. */
  function draft(id: number | Ref<number> | ComputedRef<number>): DeskResource<DraftDetail> {
    return following(id, (from, value) => from.getDraft(value))
  }

  /** `GET /repos` — the repository picker's options. */
  function repos(): DeskResource<RepoSummary[]> {
    return resource(from => from.listRepos())
  }

  /**
   * `GET /rules?repo=` — the Rules screen, for one repository or all of them.
   *
   * The filter is reactive, so changing the picker is a new read rather than a filter over
   * a list the dashboard already holds. That is the contract's shape — the server decides
   * which rules are in scope, including every global one — and it is why the picker cannot
   * disagree with the server about what "this repository" means.
   */
  function rules(repo: Ref<string> | ComputedRef<string>): DeskResource<RulesPage> {
    return following(repo, (from, value) =>
      from.listRules(value === ALL_REPOSITORIES ? null : (value as RepoName)))
  }

  /* ------------------------------- The writes ------------------------------ */

  /**
   * `POST /reviews` — the developer pastes a link and nothing else.
   *
   * The request carries only `pr_url`: the screen has no head commit to give, and the
   * server resolves the head, the title and the changed-line count by reading the pull
   * request before it creates anything. That read is a read — nothing here writes to
   * GitHub — but it can fail on a machine where `gh` is missing or logged out, and the
   * screen has to tell that apart from a link it refused. One is the developer's
   * environment; the other is their typing.
   */
  async function startReview(link: string): Promise<WriteResult> {
    const trimmed = link.trim()
    if (trimmed.length === 0) {
      return {
        ok: false,
        error: {
          code: 'bad_request',
          message: 'Paste a pull request link first.',
          field: 'pr_url',
          expected: 'a GitHub pull request link',
          status: null,
        },
      }
    }

    return write(
      to => to.startReview(trimmed),
      // Both answers this can have are one function, in `utils/format.ts`, where the
      // dismissed one can be read in a test: `POST /reviews` hands back the review
      // already on the record, and that review may be one the developer dismissed.
      (review: Review) => startedReviewLine(review),
    )
  }

  /**
   * `POST /reviews/{id}/dismiss` — the developer takes a review out of play.
   *
   * **The reason is checked here as well as at the API**, and that is not a second
   * opinion: the API's refusal is what makes the rule real, and this is what spares the
   * developer a round trip to be told what the form could have told them. The wording is
   * the API's own, so the two cannot say different things about the same blank box.
   */
  async function dismissReview(id: number, reason: string): Promise<WriteResult> {
    const said = reason.trim()
    if (said.length === 0) {
      return {
        ok: false,
        error: {
          code: 'invalid_value',
          message: 'Say why first. A review dismissed with no reason is one nobody can '
            + 'say why they hid.',
          field: 'reason',
          expected: 'why this review is being dismissed',
          status: null,
        },
      }
    }

    return write(
      to => to.dismissReview(id, said),
      // Short, because the page it is read on says what a dismissal means twice over:
      // once under the trail and once in the card this notice appears in. What the
      // notice has to add is that it happened.
      (review: Review) => `${review.repo}#${review.pr} is dismissed.`,
    )
  }

  /** `POST /reviews/{id}/restore` — bring a dismissed review back. */
  async function restoreReview(id: number): Promise<WriteResult> {
    return write(
      to => to.restoreReview(id),
      (review: Review) =>
        `${review.repo}#${review.pr} is back in the queue, exactly as it was.`,
    )
  }

  /**
   * `DELETE /reviews/{id}` — remove a review that never got past being requested.
   *
   * **Nothing is checked here.** The store decides whether a review has been started and
   * says what it holds when it has, and a second judgement in the page is how a button
   * comes to be offered for a review the API refuses. What the page does with the answer
   * is hide the button when the review holds anything — which is a different thing from
   * deciding the refusal.
   */
  async function deleteReview(id: number): Promise<WriteResult> {
    return write(
      to => to.deleteReview(id),
      (review: Review) =>
        `Review ${review.id} — ${review.repo}#${review.pr} — is deleted. It held no `
        + 'brief, no findings and no draft, and nothing of it is left.',
      /*
       * **No refresh**: the review this page is reading is the one that has just gone, so
       * reading it again answers 404 and draws "not on the desk" over the page for as
       * long as it takes to leave it. The caller leaves for the Reviews list, which reads
       * on mount.
       */
      false,
    )
  }

  /** `POST /reviews/{id}/brief/confirm` — layer 2, and the gate on everything after it. */
  async function confirmBrief(id: number): Promise<WriteResult> {
    return write(
      to => to.confirmBrief(id),
      (brief: Brief) =>
        brief.confirmed
          ? 'Problem confirmed. The draft is yours to sign off.'
          : 'The brief was not confirmed.',
    )
  }

  /**
   * `PATCH /reviews/{id}/findings/{seq}` — move a finding between the draft, held back and
   * suppressed. A finding is never deleted; the reason is what a later rule cites.
   */
  async function setFindingDisposition(
    reviewId: number,
    seq: number,
    disposition: Disposition,
    reason: string,
  ): Promise<WriteResult> {
    return write(
      to =>
        to.setFindingDisposition(
          reviewId,
          seq,
          disposition,
          reason.trim().length === 0 ? null : reason.trim(),
        ),
      (finding: Finding) => `${finding.key} is now ${DISPOSITIONS[finding.disposition]}.`,
    )
  }

  /**
   * `POST /reviews/{id}/findings/{seq}/decide` — the developer's decision on one finding.
   *
   * **The whole request is the parameter, not a decision and five optional arguments.**
   * Four of the eight decisions carry words and the contract refuses the wrong combination
   * by name, so passing the generated request type means a severity on a cut is a type
   * error here rather than a round trip that comes back refused.
   *
   * The sentence it answers with is the decision in the developer's own terms, because
   * what changed is spread over the finding, the draft and an outcome and none of those is
   * a thing to read back. `FindingDecision`'s table in the contract is where each of the
   * eight is defined; this is only the wording.
   */
  async function decideFinding(
    reviewId: number,
    seq: number,
    request: DecideFindingRequest,
  ): Promise<WriteResult> {
    return write(
      to => to.decideFinding(reviewId, seq, request),
      (made: FindingDecisionMade) => `${made.finding.key} ${DECIDED[request.decision]}`,
    )
  }

  /**
   * `POST /reviews/{id}/draft/ticket-and-approve` — file what is left, and approve.
   *
   * A sign-off. It neither posts nor files anything: the findings are recorded as ticketed,
   * the one ticket is asked for, and the review session does the rest.
   */
  async function ticketRestAndApprove(id: number, note: string): Promise<WriteResult> {
    const said = note.trim()
    return write(
      to => to.ticketRestAndApprove(id, said.length === 0 ? null : said),
      (made: TicketedAndApproved) =>
        `Signed off. ${count(made.ticketed.length, 'finding')} `
        + `(${made.ticketed_keys.join(', ')}) `
        + 'moved into the ticket. An agent files it; Review Desk does not.',
    )
  }

  /**
   * `PUT /reviews/{id}/draft` — the body, plus the edits this save represents.
   *
   * The edits are read out of the two bodies here rather than left to the server, because
   * "regraded F3 from Blocker to Should" is not recoverable from the characters that
   * changed — and they are the strongest signal in the feedback loop, so they are worth
   * getting from the text rather than asking the developer to label them.
   *
   * Which findings an edit could be about comes from the old body's own headings, not from
   * a second read of the review: the draft screen is one GET and that GET does not carry
   * the findings. Every finding a body can have been edited away from has a section in it.
   */
  async function saveDraft(id: number, before: string, body: string): Promise<WriteResult> {
    const edits: DraftEditToRecord[] = detectEdits(
      before,
      body,
      findingsInBody(before),
    ).map(edit => ({
      finding_seq: edit.finding_seq,
      kind: edit.kind,
      summary: edit.summary,
    }))

    return write(
      to => to.saveDraft(id, body, edits),
      () =>
        edits.length > 0
          ? `Saved. ${count(edits.length, 'edit')} recorded for the harvester.`
          : 'Saved. Nothing changed that the harvester records.',
    )
  }

  /** `POST /reviews/{id}/draft/approve` — approved here, posted by the review session. */
  async function approveDraft(id: number, event: ReviewEvent): Promise<WriteResult> {
    return write(
      to => to.approveDraft(id, event),
      () => 'Signed off. The review session posts it; Review Desk does not.',
    )
  }

  /** `POST /reviews/{id}/draft/send-back` — refused without a note, as the API refuses it. */
  async function sendBackDraft(id: number, note: string): Promise<WriteResult> {
    return write(
      to => to.sendBackDraft(id, note),
      () => 'Sent back. The session has the note.',
    )
  }

  /** `POST /reviews/{id}/held-back/ticket` — Review Desk records it; an agent files it. */
  async function requestNitTicket(id: number): Promise<WriteResult> {
    return write(
      to => to.requestNitTicket(id),
      (ticket: NitTicket) =>
        `${count(ticket.finding_seqs.length, 'finding')} queued as one ticket. `
        + 'An agent files it.',
    )
  }

  /** `POST /rules/{id}/promote` — the developer promotes; the app never promotes itself. */
  async function promoteRule(id: number, toStage: RuleStage = 'active'): Promise<WriteResult> {
    return write(
      to => to.promoteRule(id, toStage),
      (rule: Rule) => `${rule.key} promoted. It applies from the next review.`,
    )
  }

  /** `POST /rules/{id}/dismiss` — the reason is required, so it is not proposed again. */
  async function dismissRule(id: number, reason: string): Promise<WriteResult> {
    return write(
      to => to.dismissRule(id, reason),
      (rule: Rule) => `${rule.key} dismissed: ${reason.trim()}`,
    )
  }

  /** `POST /rules/{id}/retire` — a rule that has stopped earning its place. */
  async function retireRule(id: number, reason: string): Promise<WriteResult> {
    return write(to => to.retireRule(id, reason), (rule: Rule) => `${rule.key} retired.`)
  }

  /** `POST /rules/{id}/defer` — keep it, and ask again later. */
  async function deferRule(id: number, days: number): Promise<WriteResult> {
    return write(
      to => to.deferRule(id, days),
      (rule: Rule) => `${rule.key} kept for ${count(days, 'day')}.`,
    )
  }

  return {
    isSampleData,
    sampleAvailable,
    useSampleRecords,
    inbox,
    reviews,
    review,
    draft,
    repos,
    rules,
    startReview,
    dismissReview,
    restoreReview,
    deleteReview,
    confirmBrief,
    setFindingDisposition,
    decideFinding,
    ticketRestAndApprove,
    saveDraft,
    approveDraft,
    sendBackDraft,
    requestNitTicket,
    promoteRule,
    dismissRule,
    retireRule,
    deferRule,
  }
}

/**
 * One resource for every component that asks for it, made once.
 *
 * The caller's scope decides when it is done with it; the resource goes when the last one
 * does. `resource()` registers its own refresher and disposes with the scope that made it,
 * so the shared one is made outside any scope — which is why the clean-up is counted here
 * rather than left to Vue.
 */
function sharing<T>(key: string, make: () => DeskResource<T>): DeskResource<T> {
  let held = shared.get(key)
  if (!held) {
    held = { resource: make() as DeskResource<unknown>, wanted: 0 }
    shared.set(key, held)
  }

  held.wanted += 1
  const mine = held
  onScopeDispose(() => {
    mine.wanted -= 1
    if (mine.wanted <= 0) {
      live.delete(mine.resource.refresh)
      shared.delete(key)
    }
  })

  return held.resource as DeskResource<T>
}

/**
 * A resource over a value that can change — a review id from the route, the repository
 * picker — which re-reads when it does.
 */
function following<V, T>(
  value: V | Ref<V> | ComputedRef<V>,
  read: (from: DeskBackend, now: V) => Promise<T>,
): DeskResource<T> {
  const current = (): V => (isRef(value) ? value.value : value)
  const made = resource(from => read(from, current()))
  if (isRef(value)) watch(value, () => void made.refresh())
  return made
}

/**
 * Read the records again from scratch, with some of them replaced.
 *
 * Tests use this to reach a state no fixture of the record could express — a machine where
 * `gh` cannot be read, a database with nothing in it. It also selects sample mode, because
 * a test that wants invented records wants them whatever the URL says. No screen calls it.
 *
 * `overrides` is `Partial<DeskSource>`, typed loosely here so that this file does not
 * import the fixtures to name their shape.
 */
export async function resetDesk(overrides: Record<string, unknown> = {}): Promise<void> {
  chooseSource('sample')
  backend = null
  forgetShared()
  forgetHandOver()
  const { resetSource } = await import('~/data/sample')
  resetSource(overrides)
  await records()
}

/** Which records to read. Used by the development toggle and by the tests. */
export function useDeskSource(source: DeskSourceName | null): void {
  chooseSource(source)
  backend = null
  forgetShared()
}

/** Drop every shared resource, so the next page makes fresh ones against the new records. */
function forgetShared(): void {
  for (const held of shared.values()) live.delete(held.resource.refresh)
  shared.clear()
}
