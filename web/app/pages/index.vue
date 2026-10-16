<script setup lang="ts">
import type {
  Inbox,
  InboxAwaitingSessionItem,
  InboxBriefItem,
  InboxDraftItem,
  InboxInProgressItem,
  InboxOwnPrItem,
  InboxPostedItem,
  InboxReplyItem,
  PrState,
} from '~/types/desk'
import type { DeskNotice } from '~/utils/format'

/**
 * The queue: every review, grouped by who has to act, **one row each**.
 *
 * ## One row per pull request
 *
 * `GET /inbox` serves five arrays, and the screen this replaces drew one group per array.
 * A review can be in three of them at once — the session wrote a draft before the developer
 * confirmed the brief, and the author has replied to the round before — so the seeded
 * database showed one pull request three times. DEV-876: "A review that needs two things
 * from the developer is one row showing its next action."
 *
 * **DEV-957 moved the key from the review to the pull request**, which is the same rule
 * applied one level up. A review is one *round*, and Patrick's queue drew #9541 twice —
 * round 1 and round 2, both "Draft ready" — and #9585 twice, its posted round 1's reply
 * and its round 2 draft. One pull request is one thing to deal with. The row is about the
 * **newest round**: an unposted older round can never be the review that posts, and the
 * store no longer serves one. The heading counts these rows, so it counts pull requests.
 *
 * So the three needs-you arrays are merged by repository and number, and the row shows the
 * **earliest unfinished layer**: the brief at layer 2, then the draft at layer 6, then the
 * author's reply after it. What else is waiting is said on the row's second line rather
 * than given a row of its own — including which round an earlier round's reply was on.
 *
 * ## What the API cannot answer, and what the screen does about it
 *
 * | The board draws | Source |
 * |---|---|
 * | Waiting on the author | `posted_awaiting_author` — **DEV-930** |
 * | Requested, not started | `awaiting_session` — **DEV-994**, with the reviews GitHub asked for that no round exists of since **DEV-996** |
 * | The age of a draft, a reply or a running session | `draft_ready_at`, `replied_at`, `in_state_since` — **DEV-890** |
 * | "Session not seen for 20 min" | `last_activity_at`, which is when anything last wrote to the review |
 *
 * **All three were "None" once**, and the rows said so where the figure goes. What changed
 * each time is the record, not the screen's rule: each step keeps every state it entered,
 * so there is a time to show for a draft waiting, a reply and a session's current step; and
 * a posted review now has a group of its own, so the one group the board drew that nothing
 * could fill is filled. `session_alive` is still Allele's to answer and still null, and the
 * rows still say so rather than assuming a session is alive.
 *
 * A row whose figure is still absent says so where the figure goes. Inventing one would be
 * the one thing the brief forbids — "a figure with no data source is shown as unknown,
 * never as a number" — and the posted group is where that rule earns its keep twice over:
 * a review nothing has harvested and a review a harvest could read nothing of are two
 * absences, and the row says which.
 *
 * ## One row per pull request, across groups as well as within them
 *
 * A posted review whose author has answered is **not** drawn under "Waiting on the author".
 * It is already under "Needs you" — the author has replied and that is the developer's turn
 * — and drawing it twice is the defect DEV-876 exists to prevent. The group says how many
 * it left out and where they went, so it cannot read as "no review is waiting on an
 * author", and **the figures about the author's response are drawn on the row that is about
 * the author's response**: the needs-you row, which says "2 replies · 1 thread resolved"
 * where it used to say only which finding was replied to.
 *
 * Two facts the boards do not draw and the API does serve are kept as one line each at the
 * foot: what was handled without the developer, and a rule proposal waiting on them.
 */
const desk = useReviewDesk()
const inbox = desk.inbox()

useHead({ title: 'Queue · Review Desk' })

const link = ref('')
const starting = ref(false)

/*
 * What the desk said back. `blocked` is for something Review Desk needed and could not
 * get — the developer's `gh` is missing or logged out, or its own database is unreadable:
 * nothing was created, and the fix is a command rather than a different link.
 */
const notice = ref<DeskNotice | null>(null)

async function start() {
  starting.value = true
  const result = await desk.startReview(link.value)
  starting.value = false

  notice.value = noticeFor(result)
  /* The link stays in the field when it is refused: retyping what was right is waste. */
  if (result.ok) link.value = ''
}

/** Which repository the queue is showing. `ALL_REPOSITORIES` for every one. */
const repo = ref(ALL_REPOSITORIES)

/* ------------------------------- The rows ------------------------------- */

/** What one review is waiting for, in the order the layers run. */
interface Waiting {
  review_id: number
  repo: string
  pr: number
  title: string
  /**
   * Which round this row is about: the newest round of the pull request in the answer.
   *
   * **Never null since every item that makes a row carries a round.** It was nullable while
   * only `drafts_to_approve` had one, and that nullability is what let an older round's
   * reply decide the row's ticket — the guard against it could not fire on a row whose own
   * round was unknown.
   */
  round: number
  ticket: string | null
  /**
   * Where that ticket is, when the record holds an address for it.
   *
   * Read from the same inbox item the id is read from — any of the three that make a row —
   * and `null` for a row whose review has an id and no address, which the row then names
   * on its own line and does not link.
   */
  ticketUrl: string | null
  brief: InboxBriefItem | null
  draft: InboxDraftItem | null
  reply: InboxReplyItem | null
  /**
   * The round the author's reply was left on, when it is not the round this row is about.
   *
   * `null` when they are the same round, which is the ordinary case, and the row then says
   * nothing about it. A reply is always on the round that was *posted*, and once a row is
   * one per pull request that can be an earlier round than the one the row is numbered —
   * which is Patrick's #9585: round 1 posted and replied to, round 2 still a draft.
   */
  replyRound: number | null
  /**
   * The in-progress item for this review, when a session is also working on it.
   *
   * **Joined in for the reason `posted` below is**: the row is drawn here and not under
   * "With the agent", so what that group would have said about the session has to be said
   * on this row or not at all.
   */
  agent: InboxInProgressItem | null
  /**
   * The posted-review item for this review, when there is one.
   *
   * **Joined in so that the author's figures are drawn on the row they describe.** A
   * review whose author has replied is here rather than under "Waiting on the author",
   * and the counts of what came back belong on the row saying what came back — not in a
   * group this review is deliberately not in.
   */
  posted: InboxPostedItem | null
}

/**
 * Every pull request that is waiting on the developer, merged into one row each.
 *
 * Called through `waiting` below rather than directly, because every reader of it wants
 * the same answer about the same inbox.
 */
function needingYou(data: Inbox): Waiting[] {
  const byPullRequest = new Map<string, Waiting>()
  const keyOf = (pull: { repo: string, pr: number }) => `${pull.repo}#${pull.pr}`

  /**
   * Hand this row the identity of a newer round, when that is what it has been shown.
   *
   * **The identity moves up, the joined facts do not.** A row first made by round 1's
   * reply and then reached by round 2's draft becomes round 2's row — its link and its
   * number — while keeping the reply, which is a real thing the author did and is still the
   * developer's to read. `alsoWaiting` says which round it was on, because a reply shown on
   * a row numbered differently from the round it was left on is the kind of detail a reader
   * would otherwise have to guess at.
   *
   * **The title is not raised here, and it was.** It was taken from whichever item did the
   * raising — and a reply item raising a row to the newest round is an item about the
   * *older* one, so a pull request renamed between rounds drew round 1's title under round
   * 2's number, with the equal-round posted item unable to correct it because this returns
   * early. The title is chosen by round below, beside the ticket and for the same reason.
   * Found by a review of the second fix batch.
   *
   * **Split out of `slot` because the arrays that can raise a row are not the arrays that
   * can make one, and that was DEV-983's second fault.** Only the three needs-you arrays
   * make a row. But `author_replies` carries the round that was *replied to*, which can be
   * older than the round the pull request has reached — and the two arrays that do know the
   * newest round, `posted_awaiting_author` and `in_progress`, could only ever read a row.
   * So a pull request whose round 1 was replied to and whose round 2 was posted drew one row
   * saying "round 1" and opening round 1, with round 2's figures dropped because they were
   * about a round the row was not numbered for. The row is always the newest round in the
   * answer now, whichever array carries it.
   */
  const raise = (row: Waiting, id: number, round: number) => {
    if (round <= row.round) return
    row.review_id = id
    row.round = round
  }

  /**
   * The row for this pull request, taking the newest round's identity.
   *
   * **Keyed on the pull request, and that is DEV-957's first correction.** It was keyed on
   * the review, which is one *round*, so #9585 drew two rows — its posted round 1, where
   * the author had replied, and its round 2 draft — and the heading counted both. One pull
   * request is one thing to deal with, and the row is about the newest round of it: an
   * unposted older round can never be the review that posts, and the store no longer
   * offers one at all.
   */
  const slot = (
    id: number,
    round: number,
    pull: { repo: string, pr: number, title?: string | null },
  ): Waiting => {
    const key = keyOf(pull)
    const held = byPullRequest.get(key)
    if (held) {
      raise(held, id, round)
      return held
    }
    const made: Waiting = {
      review_id: id,
      repo: pull.repo,
      pr: pull.pr,
      title: pull.title ?? 'no title recorded',
      round,
      ticket: null,
      ticketUrl: null,
      brief: null,
      draft: null,
      reply: null,
      replyRound: null,
      agent: null,
      posted: null,
    }
    byPullRequest.set(key, made)
    return made
  }

  /*
   * **The ticket comes off whichever of the three put the row here** — all three carry it,
   * the contract requires them to, and reading it from `briefs_to_confirm` alone was a
   * defect with a name: a review whose brief had been confirmed and whose draft was
   * waiting had a row, a ticket on the record, and no way to reach it, while the Reviews
   * list showed the link for that same review.
   *
   * **But only from an item about the round this row *is*.** Once a row is one pull
   * request, two of its rounds can both be in the answer — `#9585`'s posted round 1 and its
   * round 2 draft — and the loops run in a fixed order, so the last one to touch the row
   * was deciding its ticket whatever round it was about. The two rounds of a pull request
   * usually carry the same ticket and the fault was invisible for that reason alone; where
   * they differ, the row drew round 1's ticket under round 2's number and linked a reader
   * to the wrong one.
   */
  const fromItsOwnRound = (
    row: Waiting,
    round: number,
    item: { ticket_id?: string | null, ticket_url?: string | null, title?: string | null },
  ) => {
    /*
     * **Only from an item about the round this row *is*.** The row is one pull request and
     * two of its rounds can both be in the answer, so without this the last loop to touch
     * the row decided its ticket whatever round it was about: `#9585`'s row is round 2's
     * and took round 1's ticket. The two rounds of a pull request usually carry the same
     * one, which is why it was invisible.
     *
     * **This asks a round of every item, and a round was added to two schemas to make that
     * possible.** The first version compared against `row.round`, which is set only from a
     * draft — so a row made by a brief had none, the comparison was skipped, and an older
     * round's reply went straight through. Found by a review of the commit that added the
     * guard; `briefs_to_confirm` and `author_replies` now carry a round, which closes the
     * set and leaves nothing to special-case.
     */
    if (round !== row.round) return
    row.ticket = item.ticket_id ?? null
    row.ticketUrl = ticketUrl(item.ticket_url)
    /*
     * **And the title, which is chosen the same way for a weaker version of the same
     * reason.** A title is a fact about the pull request rather than about a round, so
     * every round of it usually carries the same one and an older round's is stale rather
     * than wrong — unlike a ticket, where the row linked a reader to a different ticket.
     * But a pull request renamed between rounds drew the older title under the newer
     * number, and an item about the round the row *is* always has the newer one.
     *
     * **A row with no item about its own round keeps the title it was made with**, which
     * is some round of the same pull request. `slot` sets it, and that is the fallback: a
     * row numbered for a round nothing else mentions still has a name, which "no title
     * recorded" over a pull request with a title would not be. `InboxNewestRound` carries a
     * title for exactly that case, so the fallback is reached only by a record that holds
     * no newest round for its own pull request — found by a review of the third fix batch,
     * which pointed out that an object carrying the ticket for the row's round and not the
     * title is an object a reader has to remember an exception about.
     *
     * **A round recorded with no title says so**, rather than keeping an earlier round's.
     * This skipped a null, which made "the title of the round the row is" true except when
     * that round had none — and the contract says the row then says it has none. Written the
     * way `slot` writes it, so one spelling of "no title" reaches the page. Found by a
     * review of the fourth fix batch.
     */
    row.title = item.title ?? 'no title recorded'
  }

  /*
   * **The three arrays that make a row.** Every one carries its round, so the order of
   * these loops decides nothing. It used to: the draft was the only item with a round, so
   * it had to go first or the row had none to take — and the ticket, which is chosen by
   * round, was then decided by whichever loop ran last. Two schemas gained a `round` to
   * close that.
   *
   * The slot takes the newest round it is shown, so the row's identity is the newest round
   * whichever loop reaches it first — and the two arrays below raise it further when they
   * know of a newer one.
   */
  for (const item of data.drafts_to_approve) {
    const row = slot(item.review_id, item.round, item.pull_request)
    row.draft = item
  }
  for (const item of data.briefs_to_confirm) {
    const row = slot(item.review_id, item.round, item.pull_request)
    row.brief = item
  }
  /*
   * **The newest round's reply wins, and the plain assignment this replaced did not.**
   * `reviews_with_author_activity` is the one inbox query with no newest-round restriction
   * — deliberately, because a reply on a posted round is a real thing the developer has to
   * read — so a pull request whose round 1 was replied to *and* whose round 2 a harvest has
   * written an outcome on yields two items. The store serves them newest id first, so
   * `row.reply = item` left the row holding round 1's: its events, and through
   * `queueRow`'s `at` its time, which also put the row in the wrong place in an
   * oldest-first list. Nothing said the newer one had been dropped.
   */
  for (const item of data.author_replies) {
    /*
     * **Slotted with its own round like the other two.** A reply is always on a round that
     * *posted*, which can be older than the row's — so this can take the row's identity only
     * when no newer round is in the answer at all, which is a pull request whose only item
     * is the reply.
     */
    const row = slot(item.review_id, item.round, item.pull_request)
    if (row.reply == null || item.round > row.reply.round) row.reply = item
    /*
     * **And the row is the newest round, which only this item can say.** DEV-983, in the
     * shape the two raises below do not reach: a pull request whose round 1 was replied to
     * and whose round 2 has been opened and not yet written to is in the answer through
     * this item alone — round 2 has no brief, no ready draft, no running layer and nothing
     * posted, so no other array mentions it. The page cannot deduce a round nothing told
     * it about, and the row said "round 1" and opened round 1.
     *
     * So the item carries `rounds_on_pr` and the review that round is, and the row takes
     * both. Found by a review of this branch, which is also why the raise below is kept:
     * see its own comment.
     */
    raise(row, item.newest_round.review_id, item.newest_round.round)
  }

  /*
   * **The row's number, link and title are the newest round in the answer, and that takes
   * all five arrays rather than the three that make a row.** DEV-983's second fault: the
   * two arrays that always carry a pull request's newest round — `posted_awaiting_author`,
   * which serves only that round, and `in_progress` — could join onto a row and never
   * renumber it. So a pull request with round 1 replied to and round 2 posted drew one row
   * saying "round 1", opening round 1, and dropping round 2's figures as being about
   * another round.
   *
   * **Kept although the reply item now carries `rounds_on_pr`, and that is a decision.**
   * With that field the reply loop above already raises a row to the highest round the
   * record holds, so these two can only agree with it. They stay because the invariant is
   * "the newest round *in the answer*", asked of every source the page has, rather than
   * "the newest round one array says": which of the five arrays are served for the newest
   * round only is a property of the store's queries, and a page that trusted one field
   * from one array would be trusting a promise the contract does not make.
   *
   * **Raising, never slotting.** Neither of these makes a row: a posted review, and a
   * review a session is working on, are drawn here only because one of the three groups
   * above put them here. What they add is what the group they are *not* drawn in would have
   * said — and, now, which round the row is.
   *
   * **Found by pull request, as the three above are keyed.** By review id a posted round 1
   * joined onto its own row and nothing else, and the row for a pull request is its newest
   * round's.
   */
  for (const item of data.posted_awaiting_author) {
    const row = byPullRequest.get(keyOf(item.pull_request))
    if (row) raise(row, item.review_id, item.round)
  }
  for (const item of data.in_progress) {
    const row = byPullRequest.get(keyOf(item.pull_request))
    if (row) raise(row, item.review_id, item.round)
  }

  /*
   * **Then the joins, once every row's number is final.** Each asks whether the item is
   * about the round the row *is*, so both have to run after every raise above: a posted
   * item attached while the row was numbered 2 and then renumbered 3 by a session on a
   * newer round would be a measurement drawn under the wrong number, which is the failure
   * this guard exists to prevent rather than a case it may make.
   */
  for (const item of data.posted_awaiting_author) {
    const row = byPullRequest.get(keyOf(item.pull_request))
    /*
     * **Only when it is about the round this row is.** `posted_awaiting_author` serves a
     * pull request's newest round only, so it cannot attach an *older* round's posting to a
     * newer row; the reverse was reachable and is what the raise above now answers, by
     * making the row that round rather than dropping the figures. What is left for this
     * guard is a row a session has raised past the posted round — and there the figures are
     * a measurement about a round the row is not numbered for, which is worse than none.
     * The posted group's own note already says how many rows it left to the groups above.
     */
    if (row && item.round === row.round) row.posted = item
  }
  for (const item of data.in_progress) {
    const row = byPullRequest.get(keyOf(item.pull_request))
    /*
     * **Only the session on the round this row is**, which is the same guard the posted
     * join applies. `reviews_in_progress` asks only whether a layer is `running` and says
     * nothing about which round, so two rounds of one pull request can both be here — and
     * the row then said "a session is on step 4 of 7" about a round other than the one it
     * is numbered for.
     *
     * The first fix for this took the highest review id instead, which is a guess at the
     * newest round rather than an answer. `InboxInProgressItem` carries a round, so this
     * asks the question directly.
     */
    if (row && item.round === row.round) row.agent = item
  }

  /*
   * **The ticket last, once every row's identity has settled.** It is chosen by round, and
   * the row's round is only final after every raise above — so choosing it inside the
   * slotting loops made the answer depend on the order they ran in, which is the fault a
   * review of DEV-957's first commit found. Each row takes the ticket from whichever item is
   * about the round it is.
   *
   * **All five arrays, since DEV-983 lets two of them decide the round.** The three that
   * make a row were enough while they also decided its number. They are not now: a pull
   * request whose round 1 was replied to and whose round 2 is posted is numbered round 2,
   * and no item from the three carries round 2 at all — so the row had a ticket on the
   * record and nothing on the page to reach it with. `posted_awaiting_author` and
   * `in_progress` both serve the pair, for exactly this.
   */
  for (const item of [
    ...data.drafts_to_approve,
    ...data.briefs_to_confirm,
    ...data.author_replies,
    ...data.posted_awaiting_author,
    ...data.in_progress,
  ]) {
    const row = byPullRequest.get(keyOf(item.pull_request))
    if (row) fromItsOwnRound(row, item.round, { ...item, ...item.pull_request })
  }
  /*
   * **And a reply item's newest round, which is the one source for a row no other item is
   * about.** The loop above offers each item's own round, and a reply's is the round that
   * was *replied to* — so a pull request whose round 2 has only been opened draws a row
   * numbered round 2 that no item above is about, and the row lost a ticket it had been
   * showing. The rule is unchanged and is DEV-957's: a row takes a ticket only from
   * something about the round it is numbered for. What changed is that the reply item now
   * carries that something. Found by a review of the first fix batch.
   */
  for (const item of data.author_replies) {
    const row = byPullRequest.get(keyOf(item.pull_request))
    if (row) fromItsOwnRound(row, item.newest_round.round, item.newest_round)
  }

  /*
   * **Which round the reply was left on, when it is not the one this row is numbered for.**
   * Worked out after every group has been read, because the row's own round is only settled
   * then: a reply on round 1 reaches a row that round 2's draft later takes over.
   *
   * `posted_awaiting_author` cannot answer this — it serves a pull request's newest round
   * only, and the replied-to round is by definition one that posted, which on `#9585` is
   * round 1 while round 2 is the newest. So the reply item carries its own round, which is
   * what DEV-957 added it for.
   */
  for (const row of byPullRequest.values()) {
    /*
     * **Which round the reply was on, when it is not the one this row is numbered for.**
     * Every item carries a round now, so every row has one — the guard against an unknown
     * round that used to be here is gone with the schemas that made it necessary.
     */
    if (row.reply && row.reply.round !== row.round) row.replyRound = row.reply.round
  }

  return [...byPullRequest.values()]
}

/**
 * The merge, done once per answer.
 *
 * Four things read it — the rows, the posted group's subtraction, the note beside that
 * group's heading, and the repository chips — and it was a plain function call at each,
 * so it rebuilt its `Map` and walked three arrays five times per render, including on
 * every press of a repository chip. A `computed` over the one thing it depends on is both
 * smaller than the calls it replaces and the shape the rest of this file already uses.
 */
const waiting = computed(() => (inbox.data.value ? needingYou(inbox.data.value) : []))

/**
 * Where a row goes: **Understand, whatever the row is waiting for.**
 *
 * Patrick, on a review: "when we click a review, we want to go to the review, not to
 * the draft." In a page-per-step review the review *starts* at Understand — his order is
 * understand, alternatives, solution, then the code, then the draft — so every row lands
 * there and the reading happens in the order it was designed to happen in.
 *
 * **The trail is what says which step is waiting**, and it is a better place for that than
 * the destination of a click: it says so about all seven steps at once, where a
 * destination can only say it about one. From Understand the draft is one click along the
 * trail, which is also the click that walks you past the alternatives and the solution on
 * the way — the reading that precedes a sign-off.
 */
function stepFor(row: Waiting): string {
  return `/reviews/${row.review_id}/understand`
}

/**
 * One of the developer's own pull requests, as its row reads it.
 *
 * **A row whose only purpose is a link has to say when there is no link.** `prUrl` returns
 * null for a `repo` that is not a repository name — a released Review Desk could store
 * `acme/portal#x`, and nothing can repair that column, because `repo` is identity: half of
 * a review's unique key and the join to a repository's rules, so rewriting it would merge
 * or split reviews rather than mend them. `QueueRow` then draws the row as a `div` rather
 * than an anchor, and it went on reading "Open on GitHub" in the accent colour with the
 * "opens in a new tab" hint gone: advertising an action it could not perform.
 *
 * `warn` rather than `go`, because an unlinkable repository name is something to look at.
 *
 * **Called once per row, from `rows`, not from the template.** `:href`, `:action` and
 * `:action-tone` each need the answer; three calls in the markup are three evaluations per
 * render — on every press of a repository chip — and three chances for the expressions to
 * drift apart. An earlier version of this comment claimed the single evaluation while the
 * template still made three calls, which fixed the drift and not the count. `rows` maps it
 * onto each row now, which is where the other groups build theirs.
 */
function ownPr(item: InboxOwnPrItem) {
  const href = prUrl(item.pull_request.repo, item.pull_request.pr)
  return {
    href,
    action: href ? 'Open on GitHub' : 'no link: this is not a repository name',
    tone: href ? ('go' as const) : ('warn' as const),
  }
}

/** The next thing the developer does about this review, and what to say about it. */
function nextAction(row: Waiting) {
  if (row.brief) {
    return {
      what: 'Problem brief ready',
      detail: row.brief.blocks_later_layers
        ? 'nothing else runs until you confirm'
        : 'nothing posts until you confirm',
      action: 'Read the brief',
      to: stepFor(row),
      at: row.brief.brief_ready_at ?? null,
      atUnknown: 'the brief carries no ready time',
    }
  }
  if (row.draft) {
    const tally = severityTally(row.draft.severities)
    const held
      = row.draft.held_back > 0 ? `${count(row.draft.held_back, 'nit')} to a ticket` : ''
    return {
      what: 'Draft ready',
      detail: [tally, held].filter(Boolean).join(' · ') || 'nothing in the draft yet',
      action: 'Read and sign off',
      to: stepFor(row),
      at: row.draft.draft_ready_at ?? null,
      atUnknown: 'this review was recorded before Review Desk kept when a draft became ready',
    }
  }
  const first = row.reply!.events[0]!
  return {
    /*
     * **Who did what, which is not "the author replied" for every kind.** This row is
     * drawn for a resolution and for the harvester's observation about the code as well as
     * for a reply — see `cameBackHeadline`, which is where the kinds are told apart.
     */
    what: cameBackHeadline(first),
    /*
     * **What came back, in words, then in figures where the record has them.**
     *
     * The words are `outcomeLine`, and this is the defect DEV-958 is named after: the
     * expression here was `first.detail ?? replyLabel(first)`, and a harvested detail is
     * JSON — so the row printed
     * `{"basis":"compare 633e04a…","commit":…,"lines_changed":true}` and ran off the page.
     * Preferring the stored detail was not a careless reach for a raw field; it was the
     * only way to say anything about *this* outcome, because the function beside it worded
     * the kind and dropped the detail. `outcomeLine` does both, so there is nothing left to
     * prefer. No finding key, because `what` above has already named it.
     *
     * The figures are the counts the posted-review group carries, and this is the row they
     * are about — the review is here rather than in that group precisely because something
     * has come back on it, so drawing them there would have meant drawing them nowhere.
     */
    /*
     * **And only when the figures are about the round the reply is on.** The guard that
     * used to stop a posted item attaching to a row of another round was the thing keeping
     * these apart — "round 1's reply beside round 2's counts", which it names — and
     * renumbering the row to the newest round makes that guard pass, so the two met on one
     * line: "The author replied to F1 · Not harvested yet", which is a reply beside a
     * measurement saying nothing has come back. `replyRound` is null exactly when the reply
     * and the posting are the same round, and `alsoWaiting` says which round it was on
     * otherwise. Found by a review of the fourth fix batch.
     */
    detail: row.posted && row.replyRound == null
      ? `${outcomeLine({ kind: first.kind, detail: first.detail })} · ${
        authorResponseLabel(row.posted)}`
      : outcomeLine({ kind: first.kind, detail: first.detail }),
    /*
     * **Worded from the kind, like the heading above it.** It said "Read the reply" over a
     * row whose outcome was the harvester noticing the code had changed — an act that did
     * not happen, offered to a developer who would go looking for it.
     */
    action: cameBackAction(first.kind),
    to: stepFor(row),
    at: row.reply!.replied_at ?? null,
    atUnknown: 'no outcome on this review carries a time',
  }
}

/** What else is waiting behind the next action, so one row can say it needs two things. */
function alsoWaiting(row: Waiting): string[] {
  const also: string[] = []
  if (row.brief && row.draft) also.push('a draft is ready behind it')
  /*
   * **And which round the reply was on, when it was not this one.** A row is one pull
   * request and is numbered for its newest round, so the reply it carries can be the
   * author's answer to a round that was posted earlier — #9585, round 1 posted and replied
   * to while round 2 is still a draft. "The author has replied too" over a row numbered
   * round 2 would leave a reader to work out which review the reply was on.
   */
  const replied = row.replyRound == null
    ? 'the author has replied too'
    : `the author has replied to round ${row.replyRound}`
  if (row.brief && row.reply) also.push(replied)
  else if (row.draft && row.reply) also.push(replied)
  /*
   * **And when the reply *is* the next action but was left on an earlier round, which the
   * two lines above could not say.** They fire only beside a brief or a draft, because
   * their job is to say what is waiting *behind* the next action. DEV-983 made a third
   * shape reachable: a pull request whose round 2 is posted with nothing back yet and whose
   * round 1 has just been replied to is numbered round 2, has no brief and no draft, and
   * its headline is the reply itself — so the row read "The author replied to F1" under the
   * number of a round nobody had replied to, and said nothing about round 1 at all.
   *
   * Worded as DEV-957 words it, because it is the same sentence about the same fact: the
   * reply is on a round the row is not numbered for, and a reader should not have to work
   * out which review it was on.
   */
  else if (row.replyRound != null) also.push(`replied to round ${row.replyRound}`)
  /*
   * **And the session, which is why this row is not under "With the agent".** That group
   * draws the step, a bar and how long the step has been running; this review is drawn
   * here instead, so without this line those facts are not on the page at all — the row
   * would be the quieter for having been deduplicated, which is the opposite of the
   * point.
   */
  if (row.agent) {
    const running = layerLabel(row.agent.layer_name).toLowerCase()
    const forHow = row.agent.in_state_since ? ` for ${forLabel(row.agent.in_state_since)}` : ''
    also.push(
      `a session is on step ${row.agent.layer_number} of ${row.agent.layers_total}, `
      + `${running}${forHow}`,
    )
  }
  return also
}

/**
 * The line under the title of a posted row: the repository, and what the pull request was
 * last seen to be.
 *
 * **The state moved here from the status cell, and this is the line it belongs on.** It was
 * `prStateLabel` joined onto the end of `authorResponseLabel` — "0 replies · 0 threads
 * resolved · 0 findings with code changed since · as at 12 hours ago · pull request open at
 * the last harvest" — which the design pass quoted whole as a three-line cell saying nothing
 * new. The status cell is about what came back from the author. Whether the pull request is
 * still open is a fact about the pull request, and this line is where the row's facts about
 * the pull request are: its repository, and the two links out to it.
 *
 * `prStateLabel` returns one word now, and its own note argues the sentence away. The
 * harvest basis the sentence used to carry is still said, once, by the cell beside it.
 *
 * **The ticket is `QueueRow`'s, not this line's.** It named the ticket here too, which was
 * right while the row's own ticket link was a column at the row's end: a review with an id
 * and no address drew no ticket at all unless this said it. DEV-957 put the link inline in
 * this very line, so saying it here as well drew the id twice in a row — and the component
 * draws it once, linked or not, which is that rule being true by construction.
 *
 * **It also lost its leading `· `.** The line used to sit under the title in the row's own
 * cell, where a leading separator read as a continuation; it is a line of its own now, and
 * a line beginning with a middle dot reads as a stray mark.
 *
 * **One call site now, where it had two.** "Your pull requests" reads its repository
 * straight: those rows have no review, so there is nothing for a harvest to have looked at
 * and no state for this to say.
 */
function alsoAbout(item: { pull_request: { repo: string }, pr_state?: PrState | null }): string {
  return `${item.pull_request.repo} · ${prStateLabel(item.pr_state)}`
}

/**
 * The line under the title of a row "With the agent": the repository, and which round.
 *
 * **That group draws its rows from `in_progress` directly**, not through `needingYou`, so
 * the rule that a pull request's row is its newest round has to reach it on its own. A
 * session running on round 1 while round 2 has been opened and not yet written to is the
 * shape that needs it: round 2 is in no array at all, `reviews_in_progress` has no
 * newest-round restriction, and the row was numbered for and linked to round 1 — DEV-983's
 * second fault, in the one group its merge does not touch. Found by a review of the fourth
 * fix batch.
 *
 * The step the row draws is still the session's own round, which is what the session is
 * doing; the row is still the pull request's newest. Where they differ the line says so,
 * because "a session is on step 4 of 7" under a number saying round 2 is the detail a
 * reader would otherwise have to guess at — the same sentence `alsoWaiting` says about a
 * reply.
 */
function aboutAgent(item: InboxInProgressItem): string {
  const parts = [item.pull_request.repo, `round ${item.newest_round.round}`]
  if (item.round !== item.newest_round.round) {
    parts.push(`a session is on round ${item.round}`)
  }
  return parts.join(' · ')
}

/**
 * The line under the title of a row "Requested, not started": the repository, and the
 * round where there is one.
 *
 * **No second clause about a session's round, unlike `aboutAgent`.** That group draws rows
 * whose session may be on an older round than the row is numbered for; a round in this
 * group *is* the newest round and nothing else, so the two numbers it would reconcile are
 * one number.
 *
 * **And no round at all for a review GitHub has asked for.** DEV-996: the record holds the
 * repository, the number and when GitHub asked, and no session has opened anything — so
 * there is no round, and the template literal that drew one would read "acme/portal ·
 * round null", which TypeScript cannot refuse and a reader would have to interpret.
 */
function aboutRequested(item: InboxAwaitingSessionItem): string {
  const parts = [item.pull_request.repo]
  if (item.round != null) parts.push(`round ${item.round}`)
  return parts.join(' · ')
}

/**
 * What a row of "Requested, not started" is waiting for, and where it goes.
 *
 * **Two kinds of row and two destinations.** A round has a review page, so the row links
 * it and the action says so. A request has no round to open — DEV-996 — so the row is the
 * pull request itself, the way a row of "Your pull requests" is, and the action says that
 * instead. A row whose `to` was built from a null review id would be the string
 * `/reviews/null/understand`, which `QueueRow` would happily make an anchor to a page that
 * does not exist; a row with neither would be a `div` still captioned "Open the round",
 * which is an action it cannot perform.
 *
 * **`warn` for a repository name no link can be built from**, which is `ownPr`'s rule and
 * its reason: `prUrl` refuses a name that is not `owner/repo`, and a request row has no
 * other way out of it — no review to open, no ticket, nothing. Saying so is better than a
 * row that advertises an action it cannot do.
 *
 * Called once per row from `rows`, like `ownPr`, rather than three times from the markup.
 */
function requestedLink(item: InboxAwaitingSessionItem) {
  if (item.review_id != null) {
    return {
      to: `/reviews/${item.review_id}/understand`,
      href: null,
      action: 'Open the round',
      tone: 'quiet' as const,
      prHref: prUrl(item.pull_request.repo, item.pull_request.pr),
    }
  }
  const href = prUrl(item.pull_request.repo, item.pull_request.pr)
  return {
    to: null,
    href,
    action: href ? 'Open on GitHub' : 'no link: this is not a repository name',
    tone: href ? ('quiet' as const) : ('warn' as const),
    // Left off: the row's own destination is already the pull request, and `QueueRow`'s
    // own note says a second link to where the row goes is noise.
    prHref: null,
  }
}

/**
 * The second line under a review's title: where it is, which round, what changed.
 *
 * **The ticket is not here, and it was.** `QueueRow` draws it on this same line now —
 * linked when the record holds an address and as text when it does not — so naming it here
 * as well put the id on the line twice: "acme/portal · round 1 · ABC-231 · GitHub ·
 * ABC-231". DEV-957.
 */
function about(row: Waiting): string {
  const parts = [row.repo, `round ${row.round}`]
  /*
   * **Read off the row's own round, not off the draft item's `is_delta`.** The two agree
   * wherever both exist — the contract's `is_delta` *is* "the round is above one" — and
   * only one of them is true of a row with no draft. A row raised to round 2 by a reply on
   * round 1 is a review of changes since the last one whether or not a draft has been
   * written, and it said nothing, because `is_delta` was set from the one array that
   * carries it. Found by a review of the second fix batch.
   */
  if (row.round > 1) parts.push('changes since your last review')
  return parts.join(' · ')
}

/* --------------------------- The repository filter --------------------------- */

/**
 * The repositories with something in the queue, each with the number of rows it has.
 *
 * Counted from the rows themselves rather than from `GET /repos`, so the number beside a
 * repository is exactly the number of rows pressing it shows. `/repos` counts open reviews,
 * which is a different and larger number, and a filter whose count does not match what it
 * filters to is worse than no count.
 */
function repoTabs(data: Inbox) {
  const tally = new Map<string, number>()
  const bump = (name: string) => tally.set(name, (tally.get(name) ?? 0) + 1)
  for (const row of waiting.value) bump(row.repo)
  for (const item of withAgent.value) bump(item.pull_request.repo)
  for (const item of data.own_prs_without_reviewer) bump(item.pull_request.repo)
  /*
   * The posted rows the page actually draws, which is the group minus the reviews already
   * counted under "Needs you". A chip saying 4 over five rows is the failure the comment
   * above names: "a filter whose count does not match what it filters to is worse than no
   * count."
   */
  for (const item of withAuthor.value) bump(item.pull_request.repo)
  /*
   * **And the rounds waiting for a session**, which is the group DEV-994 added. Left out,
   * a repository whose only queue content is one of those got no chip at all — so its rows
   * were on the page and unreachable from the picker — and every other chip's count was
   * short of what pressing it showed, which is the failure the comment above names. Found
   * by a red-team pass over this change's design.
   */
  for (const item of requested.value) bump(item.pull_request.repo)

  return [
    { value: ALL_REPOSITORIES, label: 'All repositories' },
    ...[...tally.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([name, rows]) => ({ value: name, label: name, count: rows })),
  ]
}

/**
 * Which pull request an inbox item is about, as the key every group is deduplicated on.
 *
 * **The key is the pull request since DEV-957**, and the sets below are keyed on it rather
 * than on `review_id` for the same reason the merge above is: a review is one round, and
 * two rounds of one pull request drawn in two groups are two rows for one thing to deal
 * with. A session running on round 1 while round 2's draft waits is exactly that, and it
 * is reachable — `reviews_in_progress` asks only whether a layer is `running`, and says
 * nothing about which round.
 */
const prKey = (item: { pull_request: { repo: string, pr: number } }) =>
  `${item.pull_request.repo}#${item.pull_request.pr}`

/**
 * Every pull request this page draws above the **"Requested, not started"** group —
 * needs-you and with-the-agent.
 *
 * **It was "above the posted group" until DEV-994, and that sentence stopped being true the
 * moment a group was drawn between the two.** `drawnAboveAuthor` is the set for the posted
 * group now, and this one is what *it* is built from — so a seventh group inserted here
 * subtracts against the right set rather than against a comment that had quietly aged.
 * Found by a review of this change, which is the second time this file's own convention —
 * that a comment is load-bearing — caught a sentence left behind by an edit.
 *
 * One row per pull request is DEV-876's rule read at DEV-957's level, and it holds across
 * groups, not only within them: a posted review has to be subtracted from this whole set
 * rather than from the needs-you merge alone. Both halves are reachable:
 *
 * - its author has answered, which puts it in `author_replies` — and makes it the
 *   developer's turn rather than the author's
 * - a session is running a layer on it, which puts it in `in_progress`. The server's
 *   query has no draft-status guard — any layer `running` qualifies — so a posted review
 *   whose harvest step is running is in both arrays at once.
 */
const drawnAbove = computed(() => {
  const drawn = new Set(waiting.value.map(row => `${row.repo}#${row.pr}`))
  for (const item of withAgent.value) drawn.add(prKey(item))
  return drawn
})

/**
 * The pull requests waiting for a session to start: the queue's "Requested, not started".
 *
 * **Two kinds of row since DEV-996**: a round nobody is working on, and a review GitHub
 * asked for that no round exists of. Both are drawn here and both are subtracted the same
 * way; `requestedLink` is where the two part company, because only one of them has a round
 * to open.
 *
 * **The one group drawn from an array that is already disjoint from the others**, because
 * `GET /inbox` computes it as the pull requests on the desk minus the ones it served in the
 * five groups above, and then the requests minus every pull request it has drawn at all. So this subtraction can only ever be a no-op against a server that
 * keeps its contract — and it is here all the same, for the reason every other group's is:
 * one row per pull request is a property this page owes its reader, not a promise it takes
 * from whatever answered it. Two backends serve this screen and the sample one is a second
 * implementation of the same rule.
 *
 * **No `byNewestRound` pass, unlike "With the agent".** That group needs one because
 * `reviews_in_progress` asks only whether a layer is running and two rounds of one pull
 * request can both be there. This array holds the newest round and nothing else, and
 * `UNIQUE (repo, pr, round)` is what makes that one row rather than a hope —
 * `UNIQUE (repo, pr)` on `review_requests` is the same guarantee for the other kind.
 */
const requested = computed(() =>
  (inbox.data.value?.awaiting_session ?? [])
    .filter(item => !drawnAbove.value.has(prKey(item))))

/**
 * Everything drawn above the posted group, which is now three groups rather than two.
 *
 * `drawnAbove` is the needs-you rows and the agent's; this adds the rounds waiting for a
 * session. **Nothing in the record can be in both this and the posted group** — a posted
 * round is in `posted_awaiting_author`, so the server never puts it in `awaiting_session` —
 * so like the filter above this can only agree. It exists because `drawnAbove`'s own
 * contract is "every pull request this page draws above the posted group", and a group
 * inserted between the two made that sentence false the moment it was drawn. The next group
 * added will need it to be true.
 */
const drawnAboveAuthor = computed(() => {
  const drawn = new Set(drawnAbove.value)
  for (const item of requested.value) drawn.add(prKey(item))
  return drawn
})

/**
 * The reviews a session is working on, minus the ones already waiting on the developer.
 *
 * **These rows carry the ticket link too, since `GET /inbox` serves it here.** It did not
 * for a few commits, and the rows were the visible cost: a review that is *only* in this
 * array — a session working, nothing waiting on the developer — drew a row with a link to
 * its pull request and none to its ticket, while the Reviews list linked both for the same
 * review. Nothing in this file could work around it, because the ticket was not in the
 * answer. It is now.
 *
 * **The same rule, one group earlier.** `reviews_in_progress` asks only whether a layer is
 * `running`, and nothing stops a review being there *and* in one of the needs-you arrays:
 * the harvester records an author's reply while a session has the harvest step running,
 * and the review is in `author_replies` and `in_progress` at once. Drawn from both, one
 * review becomes two rows — the defect DEV-876 exists to prevent, one group further up
 * than the posted group where it was first noticed.
 *
 * It stays where the developer has something to do about it. "Needs you" is a to-do list;
 * "With the agent" is captioned "nothing for you to do", which stops being true the
 * moment the author has answered.
 */
const withAgent = computed(() => {
  const needing = new Set(waiting.value.map(row => `${row.repo}#${row.pr}`))
  return byNewestRound(
    (inbox.data.value?.in_progress ?? []).filter(item => !needing.has(prKey(item))),
  )
})

/** How many pull requests a session is working on are drawn above, for the note. */
const alsoWithAgent = computed(() => {
  const needing = new Set(waiting.value.map(row => `${row.repo}#${row.pr}`))
  return byNewestRound(
    (inbox.data.value?.in_progress ?? []).filter(
      item => showing(item.pull_request.repo) && needing.has(prKey(item)),
    ),
  ).length
})

/**
 * One item per pull request, keeping the newest round's.
 *
 * **"With the agent" needed this and had it only by accident.** `reviews_in_progress` asks
 * whether a layer is `running` and says nothing about which round — the group's own comment
 * says so — so a session working on round 2 while round 1's harvest step is still running
 * drew #9541 twice, and the note beside the heading counted rounds. That is the defect
 * DEV-957 names, in the one group the merge above does not reach: a needs-you row is built
 * by `needingYou`, and these items are drawn as they come.
 *
 * **On the round, since `InboxInProgressItem` carries one.** The first version broke the tie
 * on the review id, which rises with the round for a pull request whose rounds this record
 * opened in order — a guess at the newest round rather than an answer. A review of this
 * branch pointed out that the same array had no round while the other three had been given
 * one; now it has, and this asks the question directly.
 */
function byNewestRound<T extends {
  round: number
  pull_request: { repo: string, pr: number }
}>(items: T[]): T[] {
  const byPr = new Map<string, T>()
  for (const item of items) {
    const held = byPr.get(prKey(item))
    if (held == null || item.round > held.round) byPr.set(prKey(item), item)
  }
  return [...byPr.values()]
}

/**
 * The posted reviews this page draws under "Waiting on the author", before the picker.
 *
 * **The group, minus whatever is already drawn above it.** A review that is waiting on the
 * developer, or that a session is working on, is not waiting on its author — it belongs to
 * the earlier group, with the figures about the author's response on it. How many were left
 * out is said beside the heading, because a count that silently disagrees with the record
 * is worse than no count.
 *
 * A computed over one source rather than a function taking the inbox, because half of it
 * — `waiting` — is already one: a function reading the slot's `data` for one half and the
 * resource for the other would be two answers to the same question, equal only because the
 * slot happens to be handed the resource's own value.
 */
const withAuthor = computed(() =>
  (inbox.data.value?.posted_awaiting_author ?? [])
    .filter(item => !drawnAboveAuthor.value.has(prKey(item))))

/**
 * How many posted reviews the group left to the groups above it, for the note beside it.
 *
 * **Counted over the repository being shown**, which is the whole point of the note: the
 * rows are filtered by the picker, so a count taken over the whole record would point at
 * a row that is not on the page — "1 more is posted and is under Needs you" over a review
 * on a repository the reader has filtered out.
 */
const alsoPosted = computed(() =>
  (inbox.data.value?.posted_awaiting_author ?? []).filter(
    item => showing(item.pull_request.repo) && drawnAboveAuthor.value.has(prKey(item)),
  ).length)

const showing = (name: string) => repo.value === ALL_REPOSITORIES || repo.value === name

/* ------------------------------ The groups ------------------------------ */

/**
 * Oldest first, with the rows whose time has no source after the ones that have it.
 *
 * The board says "Oldest first" and said it over groups that carried no time at all, so the
 * caption says what this really did rather than implying a sort the data cannot support.
 * Every group has a time to sort by now — the posted rows by when they were posted, which
 * is the last of the four to get one — and a row whose own time is still absent sorts last
 * and says so where the figure goes.
 *
 * **Tie-broken by the review's id, which is stable.** Without it two rows with the same
 * time — or two rows with no time at all — came back in whatever order the arrays had been
 * read in, and `needingYou` reads three of them. DEV-957 reordered those reads for a reason
 * that has nothing to do with the sort, and the first row of the seeded queue changed: a
 * page whose order depends on the order its own merge happens to iterate is a page whose
 * order moves under an unrelated edit. `Store::work_list` tie-breaks on the id for the same
 * reason and states it: "a list whose order changed between two polls of an unchanged
 * record would make a caller's progress look like churn."
 */
function oldestFirst<T>(rows: T[], at: (row: T) => string | null, id: (row: T) => number): T[] {
  return [...rows].sort((a, b) => {
    const left = at(a)
    const right = at(b)
    if (left != null && right != null) {
      const by = new Date(left).getTime() - new Date(right).getTime()
      if (by !== 0) return by
    }
    else if (left == null && right != null) return 1
    else if (left != null && right == null) return -1
    return id(a) - id(b)
  })
}

/** One needs-you row, display-ready: the merge, its next action and what is behind it. */
function queueRow(row: Waiting) {
  const next = nextAction(row)
  return {
    ...next,
    review_id: row.review_id,
    pr: row.pr,
    title: row.title,
    about: about(row),
    detail: [next.detail, ...alsoWaiting(row)].join(' · '),
    /*
     * **The two ways out of the review, built here rather than in the template.** The pull
     * request's address is built from the row's own repository and number; the ticket's is
     * the one the record holds, or nothing. `prUrl` is a function for a reason its own
     * comment in `app/utils/links.ts` gives, and this is the call site that reason is about.
     */
    prHref: prUrl(row.repo, row.pr),
    ticketHref: row.ticketUrl,
    ticketId: row.ticket,
  }
}

const rows = computed(() => {
  const data = inbox.data.value
  if (!data) {
    return {
      needsYou: [],
      withAgent: [],
      requested: [],
      ownPrs: [],
      withAuthor: [],
      alsoPosted: 0,
      alsoWithAgent: 0,
    }
  }

  const needsYou = oldestFirst(
    waiting.value.filter(row => showing(row.repo)).map(queueRow),
    row => row.at,
    row => row.review_id,
  )
  // Sorted like every other group, now that it has a time to sort by: the caption over
  // the page says "oldest first", and until DEV-890 this group was the one it could not
  // be true of. The store serves these newest first, which is the opposite.
  const agentRows: InboxInProgressItem[] = oldestFirst(
    withAgent.value.filter(item => showing(item.pull_request.repo)),
    item => item.in_state_since ?? null,
    item => item.review_id,
  )
  // The link, its label and its tone worked out once per row, here. Three bindings in the
  // template each needed the answer, and three calls there are three evaluations per render
  // — on every press of a repository chip — plus three chances for them to drift apart.
  /*
   * **Tie-broken by the pull request's number**, because these rows have no review at all:
   * `own_prs_without_reviewer` is a pull request nobody has reviewed, which is the one
   * array with no `review_id` to be stable on.
   */
  const ownPrs = oldestFirst(
    data.own_prs_without_reviewer.filter(item => showing(item.pull_request.repo)),
    item => item.opened_at ?? null,
    item => item.pull_request.pr,
  ).map(item => ({ ...item, link: ownPr(item) }))
  /*
   * Oldest ask first, like every other group. **Every row here has a time**, whichever kind
   * it is: `requested_at` is when the round was opened, or when GitHub asked for a review
   * no round exists of, and the record stamps both — so this is the one group whose sort
   * can never fall back on the tie-break, and the one that never draws "not recorded"
   * where the age goes.
   *
   * **Tie-broken on the pull request's number rather than the review's id**, because since
   * DEV-996 a row in this group may have no review at all. The id is this page's tie-break
   * everywhere else and it cannot be here; the number is the identity every row in this
   * group has, and one pull request is one row in it by construction.
   */
  const requestedRows = oldestFirst(
    requested.value.filter(item => showing(item.pull_request.repo)),
    item => item.requested_at,
    item => item.pull_request.pr,
  ).map(item => ({ ...item, link: requestedLink(item) }))
  // Oldest posting first, like every other group, so the caption over the page stays
  // true: the API serves these newest first, which is the opposite.
  const shown = oldestFirst(
    withAuthor.value.filter(item => showing(item.pull_request.repo)),
    item => item.posted_at ?? null,
    item => item.review_id,
  )
  return {
    needsYou,
    withAgent: agentRows,
    requested: requestedRows,
    ownPrs,
    withAuthor: shown,
    alsoPosted: alsoPosted.value,
    alsoWithAgent: alsoWithAgent.value,
  }
})

/**
 * Whether any row on the page has no recorded time, which the caption then says.
 *
 * **"Requested, not started" is deliberately not a term here**, and it is the only group
 * that is not: `requested_at` is required of both kinds of row that group holds — the
 * record stamps every round as it is created, and a request carries the time the
 * dispatcher reported GitHub asking, which `review requested` will not record without. So
 * there is no absence for that group to report, and a term for it would be a condition
 * that can never be true.
 */
const someTimesMissing = computed(
  () =>
    rows.value.needsYou.some(row => row.at == null)
    || rows.value.withAgent.some(item => item.in_state_since == null)
    || rows.value.ownPrs.some(item => item.opened_at == null)
    || rows.value.withAuthor.some(item => item.posted_at == null),
)

/**
 * The line under the headline: what is on the desk that is not waiting on the developer.
 *
 * The board reads "2 with the agent · 2 waiting on the author · 2 of your own". Only the
 * parts with something in them are said — "0 with the agent · 0 of your own" is noise on a
 * database with one review — and since DEV-930 the author's part is one of them, where
 * before it had to be left out because nothing could count it.
 *
 * **It is also the signpost for a group that is below the fold at 390 pixels.** The posted
 * group sits under the needs-you rows, and on a phone that is off the bottom of the first
 * screen; this line is above it and says the number.
 */
const besides = computed(() => {
  const said: string[] = []
  if (rows.value.withAgent.length) said.push(`${rows.value.withAgent.length} with the agent`)
  /*
   * **The group furthest down the page, so the line that signposts the fold says it too.**
   * "Requested, not started" sits under "With the agent", which on a phone is below the
   * group this line was written for — and it is the one group whose rows nobody is working
   * on, so a reader who never scrolls to it is the reader who most needs to know it is
   * there.
   */
  if (rows.value.requested.length) {
    said.push(`${rows.value.requested.length} not started`)
  }
  if (rows.value.withAuthor.length) {
    said.push(`${rows.value.withAuthor.length} waiting on the author`)
  }
  if (rows.value.ownPrs.length) said.push(`${rows.value.ownPrs.length} of your own`)
  return said.join(' · ')
})

/**
 * What the right-hand cell says about a review a session is working on.
 *
 * **"Session state unknown" was all it could say until DEV-890.** Allele owns whether a
 * session is running and tells Review Desk nothing, so `session_alive` is null on every
 * row this server serves — which was honest and useless. What the record does hold is when
 * anything last wrote to the review, so the row says that instead, and says when nothing
 * has. `activityLabel` holds the order: an answer from the system that owns the fact
 * outranks an inference from the record.
 */
function sessionAction(item: InboxInProgressItem): string {
  return activityLabel(item.session_alive, item.last_activity_at)
}

/**
 * The note beside the group's count: how many posted reviews are drawn elsewhere.
 *
 * **Said, because the count would otherwise be quietly short.** A posted review whose
 * author has answered is under "Needs you" instead, and a reader scanning headings would
 * take this number for the number of reviews out with authors. Empty when none was left
 * out, so the usual case carries no footnote.
 */
/**
 * What a group that held a row back says beside its count.
 *
 * **Every group that subtracts says so.** One row per review means a row drawn in one
 * group is missing from another, and a count that is quietly short is worse than no
 * count. The wording gives no reason, deliberately: the subtraction covers two unlike
 * cases — the developer has something to do, or a session is working on it — and the one
 * reason that fitted both turned out to be true of neither.
 */
function heldBackNote(held: number, what: string): string {
  if (held === 0) return 'nothing for you to do'
  return held === 1
    ? `1 more is ${what} and is already drawn above`
    : `${held} more are ${what} and are already drawn above`
}

const postedNote = computed(() => heldBackNote(rows.value.alsoPosted, 'posted'))

/**
 * The same, for the reviews a session is working on that are drawn under "Needs you".
 *
 * Its count was silently short until this said so, which is the thing the posted group's
 * own note exists to prevent — and the standard this file holds everything else to.
 */
const agentNote = computed(() =>
  heldBackNote(rows.value.alsoWithAgent, 'with the agent'))

/** The first line of a posted row: what was posted, as what, and which round. */
function postedTitle(item: InboxPostedItem): string {
  const round
    = item.rounds_on_pr > 1 ? `Round ${item.round} of ${item.rounds_on_pr} posted` : 'Posted'
  const event = item.posted_as == null ? null : eventLabel(item.posted_as)
  return event == null ? round : `${round} as ${event}`
}

/** The seven layers, as the bar's segments: done, the one running, then the rest. */
function layerSegments(item: InboxInProgressItem): Array<'done' | 'now' | 'todo'> {
  return Array.from({ length: item.layers_total }, (_, at) => {
    const number = at + 1
    if (number < item.layer_number) return 'done'
    if (number === item.layer_number) return 'now'
    return 'todo'
  })
}

/**
 * Whether the inbox has anything in it at all.
 *
 * The waiting groups *and* the two panels. A database with one running session is not an
 * empty desk, even though nothing is waiting on the developer — "nothing is waiting on you"
 * and "there is nothing here yet" are different sentences and a new database deserves the
 * second one.
 *
 * **The posted group counts here, and leaving it out was the reported bug surviving its own
 * fix.** `totals.items` is the five waiting groups and deliberately excludes this one, so a
 * desk whose only review was posted had `items === 0` and no session running — and this
 * function sent it to the fresh-database card, which says "This database holds no reviews"
 * over a record holding exactly the review the developer came to find. The group built to
 * show it is inside the other branch and would never have drawn.
 *
 * **`awaiting_session` counts for the same reason, and it would have been the same bug a
 * second time.** A desk whose only pull request is one nobody has picked up has `items`
 * nought, nothing in progress and nothing posted — which is the headline case of the group
 * DEV-994 adds, sent to the fresh-database card by the very function whose comment records
 * the first occurrence. Caught by a red-team pass over this change's design rather than by
 * the page; three of these terms now exist and the fourth was predictable from the third.
 */
function isEmpty(data: Inbox): boolean {
  return (
    data.totals.items === 0
    && data.in_progress.length === 0
    && data.awaiting_session.length === 0
    && data.posted_awaiting_author.length === 0
  )
}

/**
 * What the page says when no group is asking the developer for anything.
 *
 * **The second sentence is conditional, and that is DEV-994.** "Anything a session produces
 * turns up here" was unconditional, and it can now sit directly above a populated
 * "Requested, not started" — a group of rounds a session has produced nothing for — which
 * reads as the page contradicting itself. The first sentence is true whatever else is on the
 * page, because `totals.items` counts the groups that ask the developer for something.
 * Found by a red-team pass over this change's design.
 *
 * **Assembled here rather than from `<template v-if>` fragments**, which is the correction
 * `handledLine` below carries and its reason: the linter puts those on their own lines and
 * the renderer then separates them with spaces, so the conditional sentence would have
 * arrived with a double space in front of it.
 */
const nothingWaiting = computed(() =>
  rows.value.requested.length
    ? 'Nothing is waiting on you.'
    : 'Nothing is waiting on you. Anything a session produces turns up here.')

/**
 * What was dealt with without the developer, as one sentence.
 *
 * The boards do not draw this and the API serves it, so it is kept rather than dropped —
 * one quiet line at the foot. Assembled as a string rather than from `<template v-if>`
 * fragments, which the linter puts on their own lines and the renderer then separates with
 * spaces: "Handled without you  this week :" is what that produced.
 */
function handledLine(data: Inbox): string {
  const h = data.handled_without_you
  const when
    = h.window_days === 7 ? 'this week' : `in ${count(h.window_days, 'day')}`
  const said: string[] = []
  if (h.description_problems_caught != null) {
    said.push(
      `${count(h.description_problems_caught, 'description problem')} caught by the CI check`,
    )
  }
  if (h.nits_filed != null && h.nit_tickets != null) {
    said.push(
      `${count(h.nits_filed, 'nit')} filed as ${count(h.nit_tickets, 'ticket')} for an agent`,
    )
  }
  return `Handled without you ${when}: ${
    said.length ? said.join(' · ') : 'nothing has reported in yet'
  }`
}
</script>

<template>
  <div class="queue">
    <DeskState
      :resource="inbox"
      :is-empty="isEmpty"
      what="the queue"
    >
      <template #default="{ data }">
        <div class="top">
          <div class="headline">
            <!--
              **Pull requests, not rounds**, which is what the number has counted since the
              merge above was keyed on the pull request: "12 reviews need you" over a list
              naming #9541 twice and #9585 twice is the heading DEV-957's comment is about.
              One pull request is one thing to deal with, and that is what a reader is
              counting when they read this line.
            -->
            <h1 class="rd-display">
              {{ rows.needsYou.length === 1
                ? '1 pull request needs you'
                : `${rows.needsYou.length} pull requests need you` }}
            </h1>
            <p
              v-if="besides"
              class="rd-meta aside-line"
            >
              {{ besides }}
            </p>
          </div>
          <LinkField
            id="start"
            v-model="link"
            class="start"
            label="Start a review"
            :working="starting"
            working-label="Reading…"
            :refusal="notice && notice.tone !== 'plain' ? notice.text : null"
            :hint="notice && notice.tone === 'plain' ? notice.text : null"
            @submit="start"
          />
        </div>

        <div class="filter">
          <DecisionTrack
            v-model="repo"
            label="Which repository to show"
            :options="repoTabs(data)"
          />
          <span class="rd-meta order">
            Oldest first<template v-if="someTimesMissing">, and the rows with no recorded
              time last</template>
          </span>
        </div>

        <p
          v-if="data.totals.items === 0"
          class="rd-empty"
        >
          {{ nothingWaiting }}
        </p>

        <!-- Needs you: the three waiting groups, merged into one row per pull request. -->
        <DeskSection
          v-if="rows.needsYou.length"
          heading="Needs you"
          :count="rows.needsYou.length"
        >
          <QueueRow
            v-for="row in rows.needsYou"
            :key="row.review_id"
            emphasis
            :pr="row.pr"
            :title="row.title"
            :meta="row.about"
            :to="row.to"
            :action="row.action"
            action-tone="go"
            :age="row.at ? sinceLabel(row.at) : null"
            :age-unknown-because="row.atUnknown"
            :pr-href="row.prHref"
            :ticket-href="row.ticketHref"
            :ticket-id="row.ticketId"
          >
            <span class="what-title">{{ row.what }}</span>
            <span class="rd-meta what-detail">{{ row.detail }}</span>
          </QueueRow>
        </DeskSection>

        <!-- With the agent: nothing for the developer to do, and which layer is running. -->
        <DeskSection
          v-if="rows.withAgent.length"
          heading="With the agent"
          :count="rows.withAgent.length"
          :note="agentNote"
        >
          <QueueRow
            v-for="item in rows.withAgent"
            :key="item.newest_round.review_id"
            :pr="item.pull_request.pr"
            :title="item.newest_round.title ?? 'no title recorded'"
            :meta="aboutAgent(item)"
            :to="`/reviews/${item.newest_round.review_id}/understand`"
            :action="sessionAction(item)"
            :action-tone="item.session_alive === false ? 'warn' : 'quiet'"
            :age="item.in_state_since ? sinceLabel(item.in_state_since) : null"
            age-unknown-because="this review was recorded before Review Desk kept when a step started"
            :pr-href="prUrl(item.pull_request.repo, item.pull_request.pr)"
            :ticket-href="ticketUrl(item.newest_round.ticket_url)"
            :ticket-id="item.newest_round.ticket_id"
          >
            <SegmentBar
              :segments="layerSegments(item)"
              :label="`Layer ${item.layer_number} of ${item.layers_total}, ${layerLabel(item.layer_name).toLowerCase()}`"
            />
            <span class="rd-meta">
              {{ layerLabel(item.layer_name) }}
              <!--
                How long the review has been in its present state, beside the step it is
                in: "Code review · 20 min". The age cell says when that began; this says
                how long it has lasted, which is the figure Patrick asked for.
              -->
              <template v-if="item.in_state_since">
                · {{ forLabel(item.in_state_since) }}
              </template>
            </span>
          </QueueRow>
        </DeskSection>

        <!--
          **Requested, not started: nobody has this.** DEV-994, and since DEV-996 two kinds
          of row: a round Review Desk holds that no session is working on, and a review
          GitHub has asked for that no round exists of. The group below "With the agent"
          and above "Waiting on the author", which is where it belongs in the order the
          page reads: who has it — you, the agent, nobody, the author.

          **Hidden when empty, and empty is a claim**: the array is the pull requests on the
          desk minus the ones every other group was served for, and then the review requests
          nothing on the desk answers — so nothing in it says every pull request on the desk
          is being dealt with and nothing has been asked for that nobody has started. That
          is a true sentence rather than an unmeasured one, which is what the posted group's
          own note argues about itself.

          The note says why no row here asks the developer for anything, including the one
          thing they cannot do about it: Review Desk does not start a session.
        -->
        <DeskSection
          v-if="rows.requested.length"
          heading="Requested, not started"
          :count="rows.requested.length"
          note="waiting for a session — Review Desk does not start one"
        >
          <QueueRow
            v-for="item in rows.requested"
            :key="prKey(item)"
            :pr="item.pull_request.pr"
            :title="item.pull_request.title ?? 'no title recorded'"
            :meta="aboutRequested(item)"
            :to="item.link.to"
            :href="item.link.href"
            :action="item.link.action"
            :action-tone="item.link.tone"
            :age="sinceLabel(item.requested_at)"
            :pr-href="item.link.prHref"
            :ticket-href="ticketUrl(item.ticket_url)"
            :ticket-id="item.ticket_id"
          >
            <span class="what-title">Waiting for a session</span>
            <!--
              **"No session is working on it", and not "nothing has picked it up".** The
              second was here and was false of the group's most reachable member: approving a
              draft writes step 6 `done`, so the round has no running layer, a status that is
              not `ready` and nothing posted — it arrives here seconds after the developer's
              own act, and told them nothing had touched a review they had just signed off.
              It was wrong transiently for a live session too, between the two
              `review-desk layer set` calls that finish one step and start the next. What is
              true of every member is that no session is *working* on it, which is the same
              reading of "running" that `reviews_in_progress` takes when it leaves the
              harvest out. Found by a review of this change.

              **And no duration beside it.** The age cell already says when the round was
              requested; a second rendering of that same instant as "for 6 days" reads as
              how long the work has been abandoned, which is a different measurement and one
              this record has not got.
            -->
            <!--
              **Which of the two things the row is**, said rather than left to the reader
              to infer from an absent round. A round in this group is one Review Desk
              holds and nobody is working on; a request is one GitHub has asked for that
              Review Desk has never been told anything else about — no title, no ticket,
              no round — and "no session is working on it" would read, of the second, as
              though there were something for a session to pick up here. There is: it is
              on GitHub, which is where the row goes. DEV-996.
            -->
            <span class="rd-meta what-detail">{{
              item.review_id == null
                ? 'no round has been opened'
                : 'no session is working on it'
            }}</span>
          </QueueRow>
        </DeskSection>

        <!--
          Waiting on the author: posted, and the pull request not seen merged or closed.
          **Hidden when empty, like the three groups above**, and that is a change of
          position rather than of rule. It was drawn unconditionally while nothing could
          fill it, because hiding it would have read as "no review is waiting on an
          author" — a claim the API could not make. It can now: an empty
          `posted_awaiting_author` says the record holds no posted review with an open
          pull request, so an empty group is a true sentence and "Waiting on the author ·
          0" on a fresh database is noise.
        -->
        <DeskSection
          v-if="rows.withAuthor.length"
          heading="Waiting on the author"
          :count="rows.withAuthor.length"
          :note="postedNote"
        >
          <QueueRow
            v-for="item in rows.withAuthor"
            :key="item.review_id"
            :pr="item.pull_request.pr"
            :title="item.pull_request.title ?? 'no title recorded'"
            :meta="alsoAbout(item)"
            :to="`/reviews/${item.review_id}/understand`"
            action="Read the review"
            action-tone="quiet"
            :age="item.posted_at ? sinceLabel(item.posted_at) : null"
            age-unknown-because="this review's GitHub id was recorded without a draft, so nothing holds when it was posted"
            :pr-href="prUrl(item.pull_request.repo, item.pull_request.pr)"
            :ticket-href="ticketUrl(item.ticket_url)"
            :ticket-id="item.ticket_id"
          >
            <span class="what-title">{{ postedTitle(item) }}</span>
            <!--
              What came back, and where the record has no answer, which absence it is.
              Never a nought over an unread thread: `authorResponseLabel` holds that rule
              and says the measurement's own time beside the figures it has.

              **One line, which is what this change is for.** Three measured noughts are
              "Nothing back" there, and the pull request's state has moved up to the line
              under the title where the row's other facts about the pull request are. What
              is left is what came back and when it was read.
            -->
            <span class="rd-meta what-detail">{{ authorResponseLabel(item) }}</span>
          </QueueRow>
        </DeskSection>

        <!-- The developer's own pull requests. GitHub's state, which Review Desk only reads. -->
        <DeskSection
          v-if="rows.ownPrs.length"
          heading="Your pull requests"
          :count="rows.ownPrs.length"
        >
          <!--
            **The repository, with no leading separator** — the same correction `alsoAbout`
            took. The line used to sit inside the row's own cell, where a leading dot read as
            a continuation; it is a line of its own since DEV-957, and one beginning with a
            middle dot reads as a stray mark.
          -->
          <QueueRow
            v-for="item in rows.ownPrs"
            :key="`${item.pull_request.repo}-${item.pull_request.pr}`"
            :pr="item.pull_request.pr"
            :title="item.pull_request.title ?? 'no title recorded'"
            :meta="item.pull_request.repo"
            :href="item.link.href"
            :action="item.link.action"
            :action-tone="item.link.tone"
            :age="item.opened_at ? openForLabel(item.opened_at).replace('open ', '') : null"
            age-unknown-because="nothing has told Review Desk when this pull request was opened"
          >
            <span class="own">
              <span class="own-warn">No reviewer requested</span>
              <span class="rd-meta">· {{ descriptionLintLabel(item.description_lint) }}</span>
            </span>
          </QueueRow>
        </DeskSection>

        <!--
          Two facts the boards do not draw, kept rather than dropped: the API serves both,
          and a figure that reaches the screen and is thrown away is a figure nobody can
          act on. One quiet line each, at the foot, out of the groups' way.
        -->
        <div class="foot">
          <p class="rd-meta foot-line">
            {{ handledLine(data) }}
          </p>
          <p
            v-if="data.proposed_rules.length"
            class="rd-meta foot-line"
          >
            {{ count(data.proposed_rules.length, 'rule proposal') }} waiting on you ·
            <NuxtLink to="/rules">{{
              data.proposed_rules.length === 1 ? 'decide on it' : 'decide on them'
            }}</NuxtLink>
          </p>
        </div>
      </template>

      <!--
        A database with nothing in it. It reads as new rather than as broken, and it says
        the one thing that fills it: a session recording a review.
      -->
      <template #empty>
        <DeskCard
          tone="raised"
          class="fresh"
        >
          <h1 class="rd-display">
            Nothing here yet
          </h1>
          <p class="rd-note rd-measure">
            This database holds no reviews. A review turns up here once an agent session has
            recorded one — it writes the brief and the findings through
            <code class="rd-mono">review-desk</code>, and what is waiting on you appears in
            this list.
          </p>
          <p class="rd-note rd-measure">
            To start one yourself, paste a pull request link below. Review Desk reads the
            pull request through your own <code class="rd-mono">gh</code> login, records the
            review at the problem brief, and writes nothing to GitHub.
          </p>
          <LinkField
            id="start-empty"
            v-model="link"
            class="fresh-start"
            label="Start a review"
            :working="starting"
            working-label="Reading…"
            :refusal="notice && notice.tone !== 'plain' ? notice.text : null"
            :hint="notice && notice.tone === 'plain' ? notice.text : null"
            @submit="start"
          />
        </DeskCard>
      </template>
    </DeskState>
  </div>
</template>

<style scoped>
.queue {
  display: flex;
  flex-direction: column;
  gap: 40px;
  line-height: 1.45;
}

.top {
  display: flex;
  flex-wrap: wrap;
  gap: 20px 40px;
  align-items: flex-end;
}

.headline {
  flex: 1 1 360px;
  min-width: 0;
}

.aside-line {
  margin: 8px 0 0;
}

.start {
  flex: 1 1 360px;
  min-width: 0;
}

.filter {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
  align-items: center;
  font-size: 14px;
}

.order {
  margin-left: auto;
}

.what-title {
  display: block;
  flex: 1 1 100%;
  font-weight: 600;
}

/*
 * **The line of facts under the state.** `text-wrap: pretty` because this line is a list of
 * clauses joined by middle dots, and the browser's greedy line breaking left "replied to
 * round" on one line and "4" alone on the next — a figure orphaned from the word it belongs
 * to, which is exactly the kind of thing a reader stops on. Where the browser does not
 * support it the line breaks as it did, which is the old behaviour rather than a broken one.
 */
.what-detail {
  display: block;
  flex: 1 1 100%;
  text-wrap: pretty;
}

.own {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 6px;
  font-size: 13px;
}

.own-warn {
  color: var(--rd-warn-ink);
  font-weight: 600;
}

.foot {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.foot-line {
  margin: 0;
}

.fresh {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 32px;
}

.fresh-start {
  max-width: 28rem;
  padding-top: 8px;
}

@media (max-width: 560px) {
  .order {
    margin-left: 0;
  }
}
</style>
