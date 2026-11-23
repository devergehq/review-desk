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
import type { ListRow, ListView } from '~/utils/pullRequests'
import type { ReviewState } from '~/utils/stateWord'

/**
 * **Pull requests: the one list.** DEV-1325, under DEV-1322, and the S1 screen.
 *
 * Queue and Reviews were two pages; they are one list now, as tabs over one table with a
 * preview of the selected row beside it. Opening a row goes to its review's Overview
 * (`tabPath`). `/reviews` redirects here, to the *Every review* view, which is the Reviews
 * list drawn whole (`ReviewsList`).
 *
 * **What moved and what did not.** Everything below this paragraph is still the queue's,
 * and so are its rules: which pull request is one row, which round the row is, which group
 * it is in and what it says. The groups became tabs under the sidebar's words, so a pull
 * request sits in the same place in both — *Needs you* is *Waiting on you*, *With the agent*
 * and *Requested, not started* are *Being reviewed*, *Your pull requests* is *Your own* — and
 * `test/screens.nuxt.spec.ts` asserts the tabs hold exactly `sidebarGroups`'s rows.
 *
 * **The status cell is the state word and one line of facts**, the word from `stateWord`.
 * The session folds into that line rather than taking a column of its own: what a session
 * is doing is said where what the review is waiting for is said.
 *
 * **Every tab's panel is drawn and the inactive ones hidden**, so a row is on the page
 * whichever tab is open — the tabs fold the list; they do not remove any of it.
 *
 * ---
 *
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
definePageMeta({ split: true })

const desk = useReviewDesk()
const inbox = desk.inbox()

useHead({ title: 'Pull requests · Review Desk' })

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
  return tabPath(row.review_id, 'overview')
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
 * `/reviews/null`, which `QueueRow` would happily make an anchor to a page that
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
      to: `/reviews/${item.review_id}`,
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
  /*
   * **The state word, the sidebar's.** The three arrays are read in the order the sidebar
   * claims them — the brief, then the draft, then what came back — which is also the order
   * `nextAction` picks the next action in, so the word and the action are about one thing.
   */
  const state: ReviewState = row.brief ? 'brief_ready' : row.draft ? 'draft_ready' : 'replies_in'
  return {
    ...next,
    key: `${row.repo}#${row.pr}`,
    state,
    repo: row.repo,
    session: sessionLine(row.agent),
    review_id: row.review_id,
    pr: row.pr,
    title: row.title,
    about: about(row),
    /*
     * **`what` folds into the facts when it says more than the state word.** "Problem brief
     * ready" and "Draft ready" are the word itself; what came back names the finding —
     * "The author replied to F1" — so that one leads the line rather than being lost.
     */
    detail: [
      ...(row.brief || row.draft ? [] : [next.what]),
      next.detail,
      ...alsoWaiting(row),
    ].join(' · '),
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
  /*
   * **"Under an earlier tab", and it was "already drawn above".** The groups were one page
   * read down; they are tabs now (DEV-1325), and a row held back from this one is under
   * a tab to the left of it.
   */
  return held === 1
    ? `1 more is ${what} and is under an earlier tab`
    : `${held} more are ${what} and are under an earlier tab`
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

/* -------------------------- The one list (DEV-1325) -------------------------- */

/**
 * What the record holds about a session working on this pull request, as one phrase.
 *
 * **The step, and how long it has been on it**, from the in-progress item — the facts the
 * queue's *With the agent* group drew as a bar and a line. Null when no session is running a
 * step on the row's round, which says nothing rather than "no session": the record cannot
 * tell an idle session from none.
 */
function sessionLine(agent: InboxInProgressItem | null): string | null {
  if (!agent) return null
  const forHow = agent.in_state_since ? ` for ${forLabel(agent.in_state_since)}` : ''
  return `on step ${agent.layer_number} of ${agent.layers_total}, `
    + `${layerLabel(agent.layer_name).toLowerCase()}${forHow}`
}

/**
 * The finished reviews, for *Done* — **the sidebar's read and the sidebar's filter**, so the
 * tab and the group cannot disagree about which week a review finished in.
 */
const finished = desk.finishedReviews()

/** The sidebar's four groups, over the same two reads. The tabs are checked against them. */
const groups = computed(() =>
  inbox.data.value ? sidebarGroups(inbox.data.value, finished.data.value) : null)

/**
 * *Done*'s rows: the sidebar's *Done this week*, one per pull request.
 *
 * **Only the ones the sidebar's group kept**, which is `doneThisWeek` minus any pull request
 * an earlier group claimed — the sidebar's own `keep`, read back off its answer rather than
 * worked out a second time here.
 */
const doneRows = computed(() => {
  const kept = new Set(groups.value?.find(group => group.id === 'done')?.rows.map(row => row.key) ?? [])
  return doneThisWeek(finished.data.value)
    .filter(item => kept.has(`${item.review.repo}#${item.review.pr}`))
    .filter(item => showing(item.review.repo))
})

/**
 * Which tab is open, held in the address so a link, a reload and Back all keep it.
 *
 * **Pushed, not replaced**: a tab is a place in the list, so Back from *Every review* is
 * *Waiting on you* again rather than wherever the reader was before the list.
 */
const route = useRoute()
const router = useRouter()

/**
 * Every tab's row keys before the find field narrows them, which is what `?pr=` is found in.
 * The repository filter has narrowed them already; the arrival watch below lifts it when it
 * hides the asked-for row.
 */
const allKeys = computed<Record<ListView, Array<{ key: string }>>>(() => ({
  waiting: rows.value.needsYou.map(row => ({ key: row.key })),
  reviewing: [...rows.value.withAgent, ...rows.value.requested].map(item => ({ key: prKey(item) })),
  author: rows.value.withAuthor.map(item => ({ key: prKey(item) })),
  own: rows.value.ownPrs.map(item => ({ key: prKey(item) })),
  done: doneRows.value.map(item => ({ key: `${item.review.repo}#${item.review.pr}` })),
  every: [],
}))

/** The pull request the address asks to have selected (`?pr=`, DEV-1373), or null. */
const askedPr = computed(() => selectionFromQuery(route.query.pr))

/*
 * **While `?pr=` waits to be taken, the tab is the one that holds it**, so a sidebar click
 * does not draw *Waiting on you* for the frame before the replace below lands (self-review
 * round 2, F5). Found in the rows before the find field, so typing cannot move it (F1).
 *
 * **Choosing a tab drops `pr`** (F4): an ask that rode along on a tab the reader chose would
 * move them off it the next time the inbox brought that pull request in.
 */
const view = computed<ListView>({
  get: () => (askedPr.value && viewHolding(allKeys.value, askedPr.value))
    || viewFromQuery(route.query.view),
  set: (next) => {
    if (next === view.value) return
    void router.push({
      query: { ...route.query, view: next === 'waiting' ? undefined : next, pr: undefined },
    })
  },
})

/**
 * *Every review* is drawn the first time it is opened and kept after, rather than read on
 * every visit to the list: it is a read of its own (`GET /reviews`) and most visits never
 * open it.
 */
const everyOpened = ref(view.value === 'every')
watch(view, (next) => {
  if (next === 'every') everyOpened.value = true
})

/** The rows *Every review* reports, which it draws and reads itself. */
const everyRows = ref<ListRow[]>([])

/** What is in the find field, which is also the field a review is started from. */
const find = computed(() => link.value)

/** A list narrowed to what the find field matches, which is all of it for a link. */
function found<T>(list: T[], of: (item: T) => { pr: number, title: string, repo: string }): T[] {
  return list.filter(item => rowMatches(of(item), find.value))
}

/** Each tab's rows, as drawn: the queue's own, narrowed by the find field. */
const shown = computed(() => ({
  needsYou: found(rows.value.needsYou, row => row),
  withAgent: found(rows.value.withAgent, item => ({
    pr: item.pull_request.pr,
    title: item.newest_round.title ?? '',
    repo: item.pull_request.repo,
  })),
  requested: found(rows.value.requested, item => ({
    pr: item.pull_request.pr,
    title: item.pull_request.title ?? '',
    repo: item.pull_request.repo,
  })),
  withAuthor: found(rows.value.withAuthor, item => ({
    pr: item.pull_request.pr,
    title: item.pull_request.title ?? '',
    repo: item.pull_request.repo,
  })),
  ownPrs: found(rows.value.ownPrs, item => ({
    pr: item.pull_request.pr,
    title: item.pull_request.title ?? '',
    repo: item.pull_request.repo,
  })),
  done: found(doneRows.value, item => ({
    pr: item.review.pr,
    title: item.review.title ?? '',
    repo: item.review.repo,
  })),
}))

/**
 * Every tab's rows as the selection, the keys and the preview walk them, in drawn order.
 *
 * Built from the same arrays the template draws, so `J` moves to the row under the one
 * selected and never to one the page is not showing.
 */
const viewRows = computed<Record<ListView, ListRow[]>>(() => ({
  waiting: shown.value.needsYou.map(row => ({
    key: row.key,
    reviewId: row.review_id,
    pr: row.pr,
    repo: row.repo,
    title: row.title,
    state: row.state,
    to: row.to,
    href: null,
    action: row.action,
    session: row.session,
  })),
  reviewing: [
    ...shown.value.withAgent.map(item => ({
      key: prKey(item),
      reviewId: item.newest_round.review_id,
      pr: item.pull_request.pr,
      repo: item.pull_request.repo,
      title: item.newest_round.title ?? 'no title recorded',
      state: 'being_written' as const,
      to: tabPath(item.newest_round.review_id, 'overview'),
      href: null,
      action: null,
      session: `${sessionLine(item)} · ${sessionAction(item).replace(/^./, first => first.toLowerCase())}`,
    })),
    ...shown.value.requested.map(item => ({
      key: prKey(item),
      reviewId: item.review_id ?? null,
      pr: item.pull_request.pr,
      repo: item.pull_request.repo,
      title: item.pull_request.title ?? 'no title recorded',
      state: requestedState(item),
      to: item.link.to,
      href: item.link.href,
      action: item.link.action,
      session: null,
    })),
  ],
  author: shown.value.withAuthor.map(item => ({
    key: prKey(item),
    reviewId: item.review_id,
    pr: item.pull_request.pr,
    repo: item.pull_request.repo,
    title: item.pull_request.title ?? 'no title recorded',
    state: 'posted' as const,
    to: tabPath(item.review_id, 'overview'),
    href: null,
    action: 'Read the review',
    session: null,
  })),
  own: shown.value.ownPrs.map(item => ({
    key: prKey(item),
    reviewId: null,
    pr: item.pull_request.pr,
    repo: item.pull_request.repo,
    title: item.pull_request.title ?? 'no title recorded',
    state: null,
    to: null,
    href: item.link.href,
    action: item.link.action,
    session: null,
  })),
  done: shown.value.done.map(item => ({
    key: `${item.review.repo}#${item.review.pr}`,
    reviewId: item.review.id,
    pr: item.review.pr,
    repo: item.review.repo,
    title: item.review.title ?? 'no title recorded',
    state: item.finished_how === 'closed' ? 'closed' as const : 'merged' as const,
    to: tabPath(item.review.id, 'overview'),
    href: null,
    action: 'Read the review',
    session: null,
  })),
  every: everyRows.value.filter(row => rowMatches(row, find.value)),
}))

/** How many rows each tab draws, which is the number beside its name. */
const viewCount = (which: ListView) => viewRows.value[which].length

/** The age column's heading, which is a different measurement on each tab. */
const AGE_HEADS: Record<ListView, string> = {
  waiting: 'Waiting',
  reviewing: 'Since',
  author: 'Posted',
  own: 'Open for',
  done: 'Finished',
  every: 'Last written',
}

/* ------------------------------ The selection ------------------------------ */

/** The selected row, by `repo#pr`: the one the preview draws and `Enter` opens. */
const selected = ref<string | null>(null)

const currentRows = computed(() => viewRows.value[view.value])

/**
 * **The first row is selected whenever the selection is not on the page**: on arrival, on a
 * change of tab, and when the find field narrows the selected row away. S1 draws the
 * preview with the first row in it; an empty pane beside a full list would be a pane that
 * says nothing until a reader learns to press something.
 */
watch(currentRows, (list) => {
  if (!list.some(row => row.key === selected.value)) selected.value = list[0]?.key ?? null
}, { immediate: true })

/**
 * **`?pr=` is taken once, on arrival** (DEV-1373). The sidebar links a row with no round as
 * `/?pr=acme/portal%23901`. Once the inbox holds that pull request, it is selected and the
 * address is *replaced* with the tab that draws it, `?view=reviewing`, so the tab is fixed
 * from then on.
 *
 * Self-review round 1 found the two faults of reading it on every render instead. The tab was
 * worked out from the rows the find field had narrowed, so typing a query that hid the row
 * flipped the list to *Waiting on you* (F1). And the address kept saying `?pr=` after `J` had
 * moved on, so clicking the same sidebar row again went nowhere (F2). Taken and dropped,
 * every later click on that row is a navigation of its own.
 *
 * Round 2 found two ways the row could still be off the page when the ask was taken. **The
 * repository filter hid it** (F3): the ask was never found, and the click went nowhere. So a
 * filter on another repository is lifted. **The find field hid it** (F6): the first row was
 * selected in its place. So the field is cleared.
 *
 * An address naming a pull request no tab holds is left as it is, and the first row selected.
 */
watch([askedPr, allKeys], ([asked, keys]) => {
  if (!asked) return
  const holding = viewHolding(keys, asked)
  if (!holding) {
    const askedRepo = asked.slice(0, asked.lastIndexOf('#'))
    if (repo.value !== ALL_REPOSITORIES && repo.value !== askedRepo) repo.value = ALL_REPOSITORIES
    return
  }
  if (!viewRows.value[holding].some(row => row.key === asked)) link.value = ''
  selected.value = asked
  void router.replace({
    query: { ...route.query, view: holding === 'waiting' ? undefined : holding, pr: undefined },
  })
}, { immediate: true })

const selectedRow = computed(() =>
  currentRows.value.find(row => row.key === selected.value) ?? null)

/** The row a holder element is, when it is one the open tab draws. */
function rowKeyOf(target: EventTarget | null): string | null {
  const holder = (target as Element | null)?.closest?.('[data-row-key]')
  const key = holder?.getAttribute('data-row-key') ?? null
  return key && currentRows.value.some(row => row.key === key) ? key : null
}

/** Focus arriving in a row selects it at once: a keyboard means it. */
function selectFromFocus(event: FocusEvent) {
  const key = rowKeyOf(event.target)
  if (key) selected.value = key
}

/*
 * **A pointer selects a row once it rests there, and only if it moved to get there.**
 *
 * Two findings of the self-review, round 1. A pointer swept down the list selected every row
 * it crossed, and each selection is a read of that review for the preview — ten rows, ten
 * reads, ten flashes of "Reading the review…" (F6). So it waits 120ms, which a sweep does not
 * and a reader does. And `J` scrolls the selected row into view, which slides a different
 * row under a pointer that has not moved; the browser reports that as the pointer entering
 * the row, and the selection snapped back to it (F7). So a pointer selects only from a
 * position other than where it was when a key last moved the selection.
 */
const RESTS_MS = 120
let resting: ReturnType<typeof setTimeout> | null = null
let pointer = { x: -1, y: -1 }
let pointerAtKey = { x: -2, y: -2 }

function selectFromPointer(event: MouseEvent) {
  pointer = { x: event.clientX, y: event.clientY }
  if (pointer.x === pointerAtKey.x && pointer.y === pointerAtKey.y) return
  const key = rowKeyOf(event.target)
  if (resting) clearTimeout(resting)
  if (!key) return
  resting = setTimeout(() => {
    resting = null
    selected.value = key
  }, RESTS_MS)
}

/*
 * **Leaving the list cancels a pending selection** (self-review round 2): a pointer that
 * crossed a row on its way to the preview's *Open* or its next action set the timer, and
 * nothing inside the list fired again to clear it — so the preview swapped to the crossed row
 * under the pointer, and the click opened the wrong review.
 */
function leaveList() {
  if (resting) clearTimeout(resting)
  resting = null
}

onBeforeUnmount(leaveList)

/** Open a row: its review's Overview, or the pull request on GitHub when there is no review. */
function open(row: ListRow | null) {
  if (!row) return
  /* The router from setup: a window listener has no component for `navigateTo` to find. */
  if (row.to) void router.push(row.to)
  else if (row.href) window.open(row.href, '_blank', 'noopener,noreferrer')
}

/** Put focus on the selected row's link, so the browser's own `Enter` and Tab start there. */
function focusSelected() {
  const key = selected.value
  if (!key) return
  const holder = [...document.querySelectorAll<HTMLElement>('[data-row-key]')]
    .find(one => one.getAttribute('data-row-key') === key && one.offsetParent !== null)
  const link = holder?.querySelector<HTMLElement>('.row')
  if (link && link.tagName !== 'DIV') link.focus()
  holder?.scrollIntoView({ block: 'nearest' })
}

/**
 * **`J` and `K` move the selection; `Enter` opens it.** `docs/design.md`'s keys table, and its
 * one rule about when a key is not a key — `keyIsForThePage` — so none of them fires while
 * the find field has the cursor.
 *
 * `Enter` on a link or a button is the browser's: a focused row is a link already, and a tab
 * or the find field's Start are buttons. This answers `Enter` only where nothing else would.
 */
function onKey(event: KeyboardEvent) {
  if (!keyIsForThePage(event)) return
  const key = event.key.toLowerCase()
  if (key === 'j' || key === 'k') {
    event.preventDefault()
    pointerAtKey = { ...pointer }
    if (resting) clearTimeout(resting)
    selected.value = moveSelection(currentRows.value, selected.value, key === 'j' ? 1 : -1)
    void nextTick(focusSelected)
    return
  }
  if (event.key === 'Enter') {
    const target = event.target as HTMLElement | null
    if (target?.closest?.('a, button, summary, [role="tab"], [role="radio"]')) return
    event.preventDefault()
    open(selectedRow.value)
  }
}

onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))

/* ------------------------------ The list's heading ------------------------------ */

/**
 * The line under the heading, with **when GitHub was read only when the record holds it** —
 * `githubReadAt`, the sidebar's own reading, and no clause at all when there is none.
 */
const subtitle = computed(() => {
  const read = githubReadAt(inbox.data.value ?? null, finished.data.value)
  return read
    ? `Every pull request on the desk, and your own, read from GitHub ${sinceLabel(read)}.`
    : 'Every pull request on the desk, and your own.'
})

/* --------------------------- Nothing needs you (S17) --------------------------- */

/** Whether the desk has nothing waiting on the developer at all, whatever is filtered. */
const nothingNeedsYou = computed(() => waiting.value.length === 0)

/**
 * One line per other group, as S17 draws them: the group, its count and its pull requests.
 *
 * Every figure is a count of rows the tabs draw, and every number named is one of those
 * rows. *Being reviewed* says the step when it is one session, and *Done this week* says how
 * many merged and how many closed; anything S17 says beyond that would be a sentence nothing
 * in the record wrote.
 */
const otherGroups = computed(() => {
  const reviewing = viewRows.value.reviewing
  const agentOnly = shown.value.withAgent.length === 1 && shown.value.requested.length === 0
    ? ` · a session is ${sessionLine(shown.value.withAgent[0]!)}`
    : ''
  const merged = shown.value.done.filter(item => item.finished_how === 'merged').length
  const closed = shown.value.done.filter(item => item.finished_how === 'closed').length
  const how = [merged ? `${merged} merged` : '', closed ? `${closed} closed` : '']
    .filter(Boolean).join(', ')
  const say = (rowsOf: ListRow[], extra = '') =>
    rowsOf.length ? `${numbersSaid(rowsOf.map(row => row.pr))}${extra}` : 'none'
  return [
    { label: 'Being reviewed', count: reviewing.length, says: say(reviewing, agentOnly) },
    { label: 'Waiting on the author', count: viewRows.value.author.length, says: say(viewRows.value.author) },
    { label: 'Your own', count: viewRows.value.own.length, says: say(viewRows.value.own, viewRows.value.own.length ? ' · no reviewer requested' : '') },
    { label: 'Done this week', count: viewRows.value.done.length, says: viewRows.value.done.length ? how || say(viewRows.value.done) : 'none' },
  ]
})

/** *Since yesterday*, from recorded times only. */
const since = computed(() =>
  inbox.data.value ? sinceYesterday(inbox.data.value, doneRows.value) : [])

/** "Review one anyway": the field is the way to, so the link puts the cursor in it. */
function focusStart() {
  document.getElementById('start')?.focus()
}

/**
 * The line under each table: the order, said out loud as the queue said it. *Being
 * reviewed*'s two groups carry their own notes beside their headings. **Assembled as
 * strings**, for `handledLine`'s reason: `<template v-if>` fragments come out with doubled
 * spaces.
 */
const order = computed(() =>
  someTimesMissing.value ? 'Oldest first, and the rows with no recorded time last.' : 'Oldest first.')

const waitingCaption = computed(() =>
  `${order.value} Times are from when the brief or the draft was ready, or when something came back.`)

const reviewingCaption = computed(() => order.value)

/**
 * The facts line of a row a session is working on: **the step and how long it has lasted,
 * where the session is in the seven, and what the record holds about the session** —
 * "Code review · 20 min · step 4 of 7 · Session not seen for 20 min". The first two are
 * what the queue's *With the agent* row said beside its bar; the bar's figure is the third,
 * in words; the fourth was that row's action, and is the session folded into the state.
 */
function agentFacts(item: InboxInProgressItem): string {
  const parts = [layerLabel(item.layer_name)]
  if (item.in_state_since) parts.push(forLabel(item.in_state_since) ?? '')
  parts.push(`step ${item.layer_number} of ${item.layers_total}`)
  parts.push(sessionAction(item))
  return parts.filter(Boolean).join(' · ')
}

/**
 * The state word of a row nobody is working on — **the sidebar's rule, read the same way**:
 * a request GitHub made that no round exists of is *Not started*; a round opened and not
 * picked up is *Waiting for a session*. `utils/sidebar.ts` chooses between the two on
 * `review_id` too, so a pull request reads the same in both columns.
 */
function requestedState(item: InboxAwaitingSessionItem): ReviewState {
  return item.review_id == null ? 'not_started' : 'waiting_for_session'
}

/**
 * Whether to draw the fresh-database card: an empty inbox, **except on *Every review***.
 *
 * That view reads `GET /reviews` and not the inbox, and it is the one place a dismissed or
 * finished review is found — which is exactly what a desk with an empty inbox may still
 * hold. Drawing the card over it would make `/reviews` say "Nothing here yet" about a
 * record holding reviews. Found by the end-to-end run on an empty database.
 */
function emptyHere(data: Inbox): boolean {
  /*
   * **And never over a desk whose only reviews finished this week** (self-review round 1,
   * F2): the inbox leaves them out, so `isEmpty` called that desk empty while the sidebar
   * listed them under *Done this week*. Those rows are the *Done* tab's and S17's to draw.
   */
  return view.value !== 'every' && isEmpty(data) && doneRows.value.length === 0
}

/**
 * **The tabs' own keys, as the ARIA tabs pattern has them**: Left and Right move to the
 * previous and next tab, Home and End to the first and last, and the tab is opened as focus
 * reaches it. Self-review round 1, F1: the tabs used a roving tabindex and nothing answered
 * the arrows, so a keyboard reached only the open tab — and five of six, *Every review* and
 * its dismissed reviews among them, were out of its reach.
 */
function onTabKey(event: KeyboardEvent) {
  /* A modifier makes it the browser's — Alt+Left is Back (self-review round 2). */
  if (event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return
  const at = LIST_VIEWS.findIndex(one => one.view === view.value)
  const last = LIST_VIEWS.length - 1
  const to = event.key === 'ArrowRight'
    ? (at + 1) % LIST_VIEWS.length
    : event.key === 'ArrowLeft'
      ? (at + last) % LIST_VIEWS.length
      : event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? last
          : null
  if (to == null) return
  event.preventDefault()
  const next = LIST_VIEWS[to]!.view
  view.value = next
  void nextTick(() => document.getElementById(`tab-${next}`)?.focus())
}
</script>

<template>
  <div class="pull-requests">
    <div
      class="list"
      @mouseover="selectFromPointer"
      @mousemove="selectFromPointer"
      @focusin="selectFromFocus"
      @mouseleave="leaveList"
    >
      <DeskState
        :resource="inbox"
        :is-empty="emptyHere"
        what="the queue"
      >
        <template #default="{ data }">
          <div class="top">
            <div class="headline">
              <h1 class="rd-display">
                Pull requests
              </h1>
              <p class="rd-meta aside-line">
                {{ subtitle }}
              </p>
            </div>
            <!--
              **One field for both, as S1 draws it**: words narrow the list, a link starts a
              review. `rowMatches` lets a link through unfiltered, so pasting one never empties
              the list while it is being read.
            -->
            <LinkField
              id="start"
              v-model="link"
              class="start"
              label="Find a pull request, or paste a link to review one"
              placeholder="#514, a title, or a pull request link"
              :working="starting"
              working-label="Reading…"
              :refusal="notice && notice.tone !== 'plain' ? notice.text : null"
              :hint="notice && notice.tone === 'plain' ? notice.text : null"
              @submit="start"
            />
          </div>

          <div
            class="tabs"
            role="tablist"
            aria-label="Which pull requests to show"
          >
            <button
              v-for="tab in LIST_VIEWS"
              :id="`tab-${tab.view}`"
              :key="tab.view"
              type="button"
              role="tab"
              class="tab"
              :class="{ 'tab--current': view === tab.view }"
              :aria-selected="view === tab.view"
              :aria-controls="`panel-${tab.view}`"
              :tabindex="view === tab.view ? 0 : -1"
              @click="view = tab.view"
              @keydown="onTabKey"
            >
              {{ tab.label }}<span
                v-if="tab.view !== 'every'"
                class="tab-count"
              >{{ viewCount(tab.view) }}</span>
            </button>
          </div>

          <div
            v-if="view !== 'every'"
            class="filter"
          >
            <DecisionTrack
              v-model="repo"
              label="Which repository to show"
              :options="repoTabs(data)"
            />
          </div>

          <!-- The column heads: one grid with the rows', so a head sits over its column. -->
          <div
            v-if="view !== 'every' && currentRows.length"
            class="list-head rd-meta"
            aria-hidden="true"
          >
            <span>PR</span>
            <span>Title</span>
            <span>State · session</span>
            <span>{{ AGE_HEADS[view] }}</span>
            <span>Next step</span>
          </div>

          <!-- Waiting on you: the queue's Needs you, one row per pull request. -->
          <section
            v-show="view === 'waiting'"
            id="panel-waiting"
            role="tabpanel"
            aria-labelledby="tab-waiting"
            class="panel"
          >
            <h2 class="rd-visually-hidden">
              Waiting on you
            </h2>
            <div
              v-if="nothingNeedsYou"
              class="nothing"
              data-testid="nothing-needs-you"
            >
              <h3 class="nothing-head">
                Nothing needs you
              </h3>
              <p class="nothing-lead">
                Nothing on the desk is waiting on you. A pull request appears here first when
                it is.
              </p>
              <dl class="nothing-groups">
                <div
                  v-for="group in otherGroups"
                  :key="group.label"
                  class="nothing-group"
                >
                  <dt>{{ group.label }} · {{ group.count }}</dt>
                  <dd>{{ group.says }}</dd>
                </div>
              </dl>
              <a
                href="#start"
                class="nothing-link"
                @click.prevent="focusStart"
              >Review one anyway: paste a link in the find field.</a>
            </div>
            <p
              v-else-if="!shown.needsYou.length"
              class="rd-empty"
            >
              No pull request waiting on you matches.
            </p>
            <QueueRow
              v-for="row in shown.needsYou"
              :key="row.review_id"
              :data-row-key="row.key"
              :selected="selected === row.key"
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
              <span class="what-title">
                <DeskPill tone="go-quiet">{{ stateWord(row.state) }}</DeskPill>
              </span>
              <span class="rd-meta what-detail">{{ row.detail }}</span>
            </QueueRow>
          </section>

          <!--
            Being reviewed: the queue's With the agent, then its Requested, not started — the
            sidebar's group, which holds both. The session is said in the state cell.
          -->
          <section
            v-show="view === 'reviewing'"
            id="panel-reviewing"
            role="tabpanel"
            aria-labelledby="tab-reviewing"
            class="panel"
          >
            <h2 class="rd-visually-hidden">
              Being reviewed
            </h2>
            <p
              v-if="!viewRows.reviewing.length"
              class="rd-empty"
            >
              No pull request is being reviewed.
            </p>
            <!--
              **Its two groups, under the queue's own headings and notes.** The sidebar's
              *Being reviewed* holds both; the tab keeps them apart because they are not the
              same thing to know — a session is working on one, and nobody on the other.
            -->
            <DeskSection
              v-if="shown.withAgent.length"
              heading="With the agent"
              :count="shown.withAgent.length"
              :note="agentNote"
            >
              <QueueRow
                v-for="item in shown.withAgent"
                :key="item.newest_round.review_id"
                :data-row-key="prKey(item)"
                :selected="selected === prKey(item)"
                :pr="item.pull_request.pr"
                :title="item.newest_round.title ?? 'no title recorded'"
                :meta="aboutAgent(item)"
                :to="tabPath(item.newest_round.review_id, 'overview')"
                action="Read the review"
                action-tone="quiet"
                :age="item.in_state_since ? sinceLabel(item.in_state_since) : null"
                age-unknown-because="this review was recorded before Review Desk kept when a step started"
                :pr-href="prUrl(item.pull_request.repo, item.pull_request.pr)"
                :ticket-href="ticketUrl(item.newest_round.ticket_url)"
                :ticket-id="item.newest_round.ticket_id"
              >
                <span class="what-title">
                  <DeskPill>{{ stateWord('being_written') }}</DeskPill>
                </span>
                <!--
                **The session, folded into the state cell.** Its step, how long it has been on
                it, and what the record holds about it — `activityLabel`, which said it in the
                action column when this was the queue's With the agent group.
              -->
                <span
                  class="rd-meta what-detail"
                  :class="{ 'session-warn': item.session_alive === false }"
                >{{ agentFacts(item) }}</span>
              </QueueRow>
            </DeskSection>
            <DeskSection
              v-if="shown.requested.length"
              heading="Requested, not started"
              :count="shown.requested.length"
              note="waiting for a session — Review Desk does not start one"
            >
              <QueueRow
                v-for="item in shown.requested"
                :key="prKey(item)"
                :data-row-key="prKey(item)"
                :selected="selected === prKey(item)"
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
                <span class="what-title">
                  <DeskPill>{{ stateWord(requestedState(item)) }}</DeskPill>
                </span>
                <!--
                Which of the two kinds of row this is, as the queue said it. The group's note
                says the rest: Review Desk does not start a session.
              -->
                <span class="rd-meta what-detail">{{
                  item.review_id == null
                    ? 'no round has been opened'
                    : 'no session is working on it'
                }}</span>
              </QueueRow>
            </DeskSection>
            <p
              v-if="viewRows.reviewing.length"
              class="rd-meta caption"
            >
              {{ reviewingCaption }}
            </p>
          </section>

          <!-- Waiting on the author: posted, and the pull request not seen merged or closed. -->
          <section
            v-show="view === 'author'"
            id="panel-author"
            role="tabpanel"
            aria-labelledby="tab-author"
            class="panel"
          >
            <h2 class="rd-visually-hidden">
              Waiting on the author
            </h2>
            <p
              v-if="!shown.withAuthor.length"
              class="rd-empty"
            >
              No posted review is waiting on its author.
            </p>
            <QueueRow
              v-for="item in shown.withAuthor"
              :key="item.review_id"
              :data-row-key="prKey(item)"
              :selected="selected === prKey(item)"
              :pr="item.pull_request.pr"
              :title="item.pull_request.title ?? 'no title recorded'"
              :meta="alsoAbout(item)"
              :to="tabPath(item.review_id, 'overview')"
              action="Read the review"
              action-tone="quiet"
              :age="item.posted_at ? sinceLabel(item.posted_at) : null"
              age-unknown-because="this review's GitHub id was recorded without a draft, so nothing holds when it was posted"
              :pr-href="prUrl(item.pull_request.repo, item.pull_request.pr)"
              :ticket-href="ticketUrl(item.ticket_url)"
              :ticket-id="item.ticket_id"
            >
              <span class="what-title">
                <DeskPill>{{ stateWord('posted') }}</DeskPill>
                <span class="rd-meta state-aside">{{ postedTitle(item) }}</span>
              </span>
              <!--
                **The state line carries which round posted and as what** — `postedTitle`, the
                words this line had before the pill — and the facts line what came back, alone,
                so it stays one line. `authorResponseLabel` keeps its rule: never a nought over
                an unread thread.
              -->
              <span class="rd-meta what-detail">{{ authorResponseLabel(item) }}</span>
            </QueueRow>
            <p
              v-if="shown.withAuthor.length && rows.alsoPosted"
              class="rd-meta caption"
            >
              {{ postedNote }}.
            </p>
          </section>

          <!-- Your own: the developer's pull requests. GitHub's state, which Review Desk only reads. -->
          <section
            v-show="view === 'own'"
            id="panel-own"
            role="tabpanel"
            aria-labelledby="tab-own"
            class="panel"
          >
            <h2 class="rd-visually-hidden">
              Your own
            </h2>
            <p
              v-if="!shown.ownPrs.length"
              class="rd-empty"
            >
              None of your own pull requests is waiting for a reviewer.
            </p>
            <QueueRow
              v-for="item in shown.ownPrs"
              :key="`${item.pull_request.repo}-${item.pull_request.pr}`"
              :data-row-key="prKey(item)"
              :selected="selected === prKey(item)"
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
          </section>

          <!-- Done: the sidebar's Done this week, one row per pull request. -->
          <section
            v-show="view === 'done'"
            id="panel-done"
            role="tabpanel"
            aria-labelledby="tab-done"
            class="panel"
          >
            <h2 class="rd-visually-hidden">
              Done
            </h2>
            <p
              v-if="!shown.done.length"
              class="rd-empty"
            >
              Nothing finished this week. Every finished review is under Every review.
            </p>
            <QueueRow
              v-for="item in shown.done"
              :key="item.review.id"
              :data-row-key="`${item.review.repo}#${item.review.pr}`"
              :selected="selected === `${item.review.repo}#${item.review.pr}`"
              :pr="item.review.pr"
              :title="item.review.title ?? 'no title recorded'"
              :meta="`${item.review.repo} · round ${item.review.round} of ${item.rounds_on_pr}`"
              :to="tabPath(item.review.id, 'overview')"
              action="Read the review"
              action-tone="quiet"
              :age="item.finished_at ? sinceLabel(item.finished_at) : null"
              :pr-href="prUrl(item.review.repo, item.review.pr)"
              :ticket-href="ticketUrl(item.review.ticket_url)"
              :ticket-id="item.review.ticket_id"
            >
              <span class="what-title">
                <DeskPill>{{ stateWord(item.finished_how === 'closed' ? 'closed' : 'merged') }}</DeskPill>
              </span>
              <span class="rd-meta what-detail">No round of it can be posted.</span>
            </QueueRow>
            <p
              v-if="shown.done.length"
              class="rd-meta caption"
            >
              Finished in the last seven days. Every finished review is under Every review.
            </p>
          </section>

          <!-- Every review: the Reviews list, whole. -->
          <section
            v-show="view === 'every'"
            id="panel-every"
            role="tabpanel"
            aria-labelledby="tab-every"
            class="panel"
          >
            <ReviewsList
              v-if="everyOpened"
              :selected="selected"
              @rows="everyRows = $event"
            />
          </section>

          <p
            v-if="view === 'waiting' && shown.needsYou.length"
            class="rd-meta caption"
          >
            {{ waitingCaption }}
          </p>

          <!--
            Two facts the boards do not draw, kept rather than dropped: the API serves both,
            and a figure that reaches the screen and is thrown away is a figure nobody can
            act on. One quiet line each, at the foot, out of the list's way.
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
              Nothing is on the desk. A review turns up here once an agent session has
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
            <!--
              **The way to Every review from a desk with nothing on it.** The sidebar's
              Reviews item was it until DEV-1325; a database whose only review was dismissed
              or finished has an empty inbox and still holds that review.
            -->
            <p class="rd-note">
              <NuxtLink :to="{ path: '/', query: { view: 'every' } }">Every review the record holds</NuxtLink>,
              including any you dismissed or that finished.
            </p>
          </DeskCard>
        </template>
      </DeskState>
    </div>

    <!--
      **The preview, S1's right-hand column.** The selected row's bottom line, state, next
      action and session; or, when nothing waits on the developer, what changed since
      yesterday (S17). Not drawn under 1,200 pixels, where the table needs the width — `J`,
      `K` and `Enter` still walk the list there.
    -->
    <aside
      class="preview-column"
      aria-label="Preview"
    >
      <div
        v-if="view === 'waiting' && nothingNeedsYou && inbox.data.value"
        class="since"
        data-testid="since-yesterday"
      >
        <h2 class="since-head">
          Since yesterday
        </h2>
        <ul
          v-if="since.length"
          class="since-list"
        >
          <li
            v-for="line in since"
            :key="`${line.at}-${line.says}`"
          >
            {{ line.says }}
          </li>
        </ul>
        <p
          v-else
          class="rd-meta"
        >
          Nothing the record keeps a time for has changed in the last day.
        </p>
      </div>
      <ListPreview
        v-else-if="selectedRow"
        :key="selectedRow.key"
        :row="selectedRow"
      />
    </aside>
  </div>
</template>

<style scoped>
/*
 * **Two columns: the list, and S1's preview beside it.** The layout gives this page the
 * main column without its right-hand padding (`definePageMeta({ split: true })`), so the
 * preview's rule runs the window's height at the shell's edge, as S1 draws it.
 */
.pull-requests {
  display: grid;
  grid-template-columns: minmax(0, 1fr) var(--rd-conversation-width);
  min-height: 100%;
}

/*
 * **The row tracks, tightened for a list with a preview beside it.** `QueueRow` reads these
 * (and the Every review fold, and the column heads above): 48 for the number, 80 for the
 * age, 136 for the action and a 16-pixel gap, which hands the two text tracks the 44 pixels
 * that keep "Nothing back · read 12 hours ago" on one line at 1,440 with the preview open.
 * The indent is the number's track plus one gap, where a fold's summary starts.
 */
.list {
  --rd-row-tracks: 48px minmax(0, 1fr) minmax(0, 1fr) 80px 136px;
  --rd-row-gap: 16px;
  --rd-row-indent: 64px;

  display: flex;
  flex-direction: column;
  gap: 20px;
  min-width: 0;
  padding-right: 40px;
  line-height: 1.45;
}

.list > :deep(*) {
  min-width: 0;
}

.top {
  display: flex;
  flex-wrap: wrap;
  gap: 16px 32px;
  align-items: flex-end;
}

.headline {
  flex: 1 1 320px;
  min-width: 0;
}

.aside-line {
  margin: 6px 0 0;
}

.start {
  flex: 1 1 320px;
  min-width: 0;
}

/* S1's tabs: the label, its count beside it, and a rule under the one that is open. */
.tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 0 24px;
  border-bottom: 1px solid var(--rd-line);
}

.tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: var(--rd-control-min);
  padding: 0;
  color: var(--rd-ink-muted);
  font: inherit;
  font-size: var(--rd-type-lead);
  background: none;
  border: 0;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  cursor: pointer;
}

.tab:hover {
  color: var(--rd-ink);
}

.tab--current {
  color: var(--rd-ink);
  font-weight: 600;
  border-bottom-color: var(--rd-ink);
}

.tab:focus-visible {
  outline: 3px solid var(--rd-focus-ring);
  outline-offset: 2px;
}

.tab-count {
  color: var(--rd-ink-faint);
  font-size: var(--rd-type-ui);
  font-weight: 400;
}

.filter {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
  align-items: center;
  font-size: 14px;
}

/*
 * **The column heads, on the rows' own tracks.** `QueueRow`'s grid, with the 12 pixels the
 * list pads each row by, so every head sits over the column it names.
 */
.list-head {
  display: grid;
  grid-template-columns: var(--rd-row-tracks);
  gap: var(--rd-row-gap);
  padding: 0 12px;
  font-weight: 600;
}

.panel {
  display: flex;
  flex-direction: column;
}

/* The list's rows carry 12 pixels each side, so the selected row's bar is not on the text. */
.panel :deep(.queue-line),
.queue-line {
  padding-right: 12px;
  padding-left: 12px;
}

.what-title {
  display: flex;
  flex: 1 1 100%;
  flex-wrap: wrap;
  gap: 2px 8px;
  align-items: center;
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

.session-warn {
  color: var(--rd-warn-ink);
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

.caption {
  margin: 0;
  padding: 0 12px;
}

/* S17: what the table area says when nothing waits on the developer. */
.nothing {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 28px 32px;
  background: var(--rd-ground);
  border-radius: var(--rd-radius-control);
}

.nothing-head {
  margin: 0;
  font-size: var(--rd-type-heading);
  font-weight: 600;
}

.nothing-lead {
  margin: 0;
}

.nothing-groups {
  display: grid;
  gap: 6px;
  margin: 0;
}

.nothing-group {
  display: grid;
  grid-template-columns: minmax(0, 14rem) minmax(0, 1fr);
  gap: 4px 24px;
}

.nothing-group dt {
  font-weight: 600;
}

.nothing-group dd {
  margin: 0;
  overflow-wrap: anywhere;
}

.nothing-link {
  align-self: flex-start;
}

.foot {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 0 12px;
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

/*
 * **The preview column, held in view as the list scrolls.** Its own scroll when the
 * selected review's facts are longer than the window, so the next action at its foot is
 * always reachable.
 */
.preview-column {
  position: sticky;
  top: 0;
  align-self: start;
  height: 100vh;
  margin-top: -36px;
  padding: 36px 24px 24px;
  overflow-y: auto;
  border-left: 1px solid var(--rd-line);
}

.since {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.since-head {
  margin: 0;
  color: var(--rd-ink-muted);
  font-size: var(--rd-type-label);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.since-list {
  display: grid;
  gap: 8px;
  margin: 0;
  padding-left: 1.1em;
}

/*
 * **Under 1,200 pixels the table takes the width.** At 1,280 the shell leaves the main
 * column about 1,050 pixels, and a 380-pixel preview beside five tracks squeezed the status
 * cell to a word a line. The keys still walk the list; the preview is what is folded.
 */
@media (max-width: 1199px) {
  .pull-requests {
    grid-template-columns: minmax(0, 1fr);
  }

  .preview-column {
    display: none;
  }

  .list {
    padding-right: 0;
  }
}

/*
 * **On a phone the tabs scroll on their own line** rather than wrapping to three: the strip
 * is one control, and three lines of it pushed the first row below the fold. The selected
 * row is not marked there either — with no preview beside it, a tinted first row says
 * nothing a reader can use.
 */
@media (max-width: 699px) {
  .tabs {
    flex-wrap: nowrap;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab {
    flex: none;
  }

  .queue-line--selected {
    background: none;
    box-shadow: none;
  }
}

@media (max-width: 560px) {
  .list-head {
    display: none;
  }

  .nothing {
    padding: 20px;
  }

  .nothing-group {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
