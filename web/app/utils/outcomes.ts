/*
 * **What came back on a finding, in words.** Auto-imported, so a screen calls these by name.
 *
 * Nothing here reads or writes a record. It takes an outcome as the store serves it and
 * answers what a reader should see.
 *
 * ## The defect this module exists to close
 *
 * The harvester writes an outcome's `detail` as JSON, because the fields it records are
 * several and are read back by `review-desk outcome list` as well as by a screen. On
 * 13 October 2026 the queue drew one of them as it was stored:
 *
 * ```
 * The author replied to F10
 * {"basis":"compare 633e04a...65eff63","commit":"65eff63","file":"app/Reporting/…
 * ```
 *
 * — a row of JSON running off the page, over a review whose whole story the developer
 * could not read: "I don't really know what that means." A stored detail is a record of
 * what was found, not a sentence about it, and the two are not interchangeable.
 *
 * ## The rule, written so it can be checked rather than remembered
 *
 * **A detail that parses as a structure is never printed.** [`freeDetail`] is the only
 * route from a stored detail to a screen, and it answers `null` for anything that parses
 * as an object or an array — so the harvester's three shapes, and any shape a later
 * harvester invents, are unprintable by construction. What reaches a reader is composed
 * here out of named fields, or it is a detail a person typed, which is prose and always
 * was: `outcome add --kind rejected --detail "their reason"`, the rejection reason the
 * rule counts.
 *
 * That is stronger than wording the three kinds the harvester writes today and trusting
 * the next one to be worded too. A kind added to `OutcomeKind` is a type error in
 * [`SAID`] below *and* degrades rather than throwing at run time — see [`phraseFor`],
 * because this bundle is built separately from the server that serves it. A *field* added
 * to a detail the harvester already writes is simply not drawn, which is the safe
 * direction to fail in, and `test/outcomes.spec.ts` reads the field names out of
 * `crates/harvest/src/run.rs` so that adding one fails there.
 *
 * ## The four facts, and why the fourth is here
 *
 * `crates/harvest/src/lib.rs` records four facts and three of them are outcome kinds; the
 * fourth is the pull request's own state, which lands on `Review.pr_state`. All four are
 * worded here, because they are read together: the step-7 page's one line is the fourth
 * and its rows are the other three.
 *
 * Two places already word that state and neither suits a headline — `prStateLabel` says
 * "pull request merged at the last harvest" for a row of the queue, and
 * `harvest::settled_as` prefixes the step's own detail with `merged, `. [`prEndLabel`]
 * below is the third and says so, rather than a page composing a fourth inline.
 *
 * ## It describes; it does not grade
 *
 * **No sentence here says a finding was fixed**, and that is a correction rather than a
 * style. `format.ts`'s `replyLabel` worded `code_changed_after` as "F10 fixed in a later
 * commit", which is a judgement the record does not support and the harvester refuses to
 * make: it compares two commits and reports that the lines a finding points at are no
 * longer the lines that were reviewed. They may have been fixed, rewritten, moved, or
 * deleted along with the feature. `crates/harvest/src/lib.rs` is explicit that reading an
 * outcome needs judgement and that the crate has none; a screen inherits the same
 * position.
 */

import { sinceLabel } from './format'

import type { Actor, Instant, OutcomeKind } from '~/types/desk'

/**
 * As much of an outcome as any of this needs.
 *
 * Narrower than `Outcome` on purpose: the queue reads `InboxReplyEvent`, which carries no
 * actor and no time, and the step-7 page reads `Outcome`, which carries both. One function
 * over the fields they share is what keeps the two screens saying the same thing about one
 * record — the drift `replyLabel` was extracted from a template to close, closed again one
 * level down.
 */
export interface OutcomeLike {
  kind: OutcomeKind
  detail?: string | null
  actor?: Actor | null
}

/**
 * The fields the harvester writes, by the kind that writes them.
 *
 * Declared rather than read loosely, so that composing a sentence out of a field the
 * harvester does not write is a type error here instead of `undefined` on a screen. Every
 * one is optional: a detail is a stored string, and a string can be anything.
 *
 * @see `crates/harvest/src/run.rs` — `harvest_thread` and `harvest_code_changes`, which are
 * the only two functions that write any of this.
 */
interface HarvestFacts {
  /** `replied` — the reply's own comment id, and the thread it hangs under. */
  comment_id?: unknown
  in_reply_to?: unknown
  /** `replied` — the login that wrote it, as GitHub spells it. */
  login?: unknown
  /** `replied` — the reply, verbatim. */
  body?: unknown
  /** `thread_resolved` — the thread's root comment, and who resolved it. */
  thread?: unknown
  resolved_by?: unknown
  /** `code_changed_after` — the file, the finding's line, and whether that line moved. */
  file?: unknown
  line?: unknown
  lines_changed?: unknown
  file_status?: unknown
  /** `code_changed_after` — the commit the change was seen in, and the comparison behind it. */
  commit?: unknown
  basis?: unknown
  /**
   * Whether the time recorded is when the thing happened or when the harvester saw it.
   *
   * `github` for a reply, which carries its own `createdAt`; `observed` for a resolution,
   * which GitHub does not timestamp at all. [`OutcomeSaid.observed`] carries it to a
   * screen, so a page does not print "resolved 3 min ago" about something that may have
   * happened last week.
   */
  time_basis?: unknown
}

/**
 * A stored detail's fields, or `null` when it holds no JSON object.
 *
 * Private, and the only parser. `JSON.parse` accepts a bare number, a bare string and
 * `null` as well as an object, so the result is tested for being an object — without
 * which `"65eff63"` would parse happily and a sentence would be composed from a detail
 * that is a plain word.
 */
function facts(detail: string | null | undefined): HarvestFacts | null {
  const parsed = structured(detail)
  if (parsed === null || Array.isArray(parsed)) return null
  return parsed as HarvestFacts
}

/**
 * A detail parsed, when it parses as an object or an array — and `null` for everything
 * else, **including a bare value that `JSON.parse` happens to accept.**
 *
 * `JSON.parse('"65eff63"')` and `JSON.parse('7')` both succeed, and neither result is a
 * record of a read: a developer who typed a commit id into `outcome add --detail` typed
 * prose, and prose is what [`freeDetail`] hands back. What this answers non-null for is a
 * structure, which is the one thing that must never reach a screen.
 *
 * An array is here rather than in [`facts`] because the two questions are different. "Is
 * this a structure?" decides whether it may be printed, and an array printed raw is the
 * same defect in different brackets. "Are these the harvester's fields?" decides whether a
 * sentence can be composed, and an array has none of them.
 */
function structured(detail: string | null | undefined): object | null {
  if (!detail) return null
  let parsed: unknown
  try {
    parsed = JSON.parse(detail)
  }
  catch {
    return null
  }
  if (parsed === null || typeof parsed !== 'object') return null
  return parsed
}

/** One of a detail's fields as a non-empty string, or `null`. A number reads as its digits. */
function text(value: unknown): string | null {
  if (typeof value === 'string') return value.trim().length > 0 ? value : null
  if (typeof value === 'number' && Number.isFinite(value)) return String(value)
  return null
}

/** One of a detail's fields as a line number, or `null`. */
function lineNumber(value: unknown): number | null {
  if (typeof value === 'number' && Number.isInteger(value)) return value
  return null
}

/**
 * A stored detail, when it is something a person wrote rather than a record of a read.
 *
 * **This is the rule the module exists for, and it is one function so it can be tested as
 * one.** A detail that parses as a JSON object answers `null`: the harvester's shapes are
 * unprintable, whatever fields they come to hold. Anything else is prose — a rejection
 * reason, a note typed into `outcome add --detail` — and is handed back as written.
 *
 * `test/outcomes.spec.ts` holds the three details the harvester really writes against this,
 * and reads the field names out of `crates/harvest/src/run.rs` rather than listing them —
 * so a field added to a detail in Rust fails a test in TypeScript rather than arriving on
 * a screen as a key nobody worded.
 */
export function freeDetail(detail: string | null | undefined): string | null {
  if (!detail) return null
  if (structured(detail) != null) return null
  const trimmed = detail.trim()
  return trimmed.length > 0 ? trimmed : null
}

/**
 * A commit as this app quotes one: seven characters.
 *
 * The same seven `TickedText` draws a commit id at, and the same seven GitHub shows. The
 * whole forty are in the record and nothing here loses them; a sentence quotes the short
 * form because a sentence is read, and `Already decided` on the Understand page is the
 * measurement — a 40-character id printed twice in a line made the line unreadable.
 *
 * `unknown` rather than a string, because what it is given is a field off a stored detail
 * and a stored detail can hold anything. A caller that has a string loses nothing.
 */
export function shortCommit(sha: unknown): string | null {
  const id = text(sha)
  return id == null ? null : id.slice(0, 7)
}

/**
 * One outcome, as a screen draws it.
 *
 * Structured rather than one string, because two screens want different parts of it. The
 * queue has one ellipsised line and takes [`outcomeLine`], which composes these. The
 * step-7 page draws the quote as a quote, the commit in mono and the basis of the time as
 * its own words, so it takes the parts.
 */
export interface OutcomeSaid {
  /**
   * What came back, with no finding key in front of it and no full stop after it.
   *
   * **No sentence here begins with a value out of the record**, and that is a rule rather
   * than a habit. A caller that wants a standalone sentence passes this through
   * `format.ts`'s `sentence()`, which capitalises the first character — and a login, a
   * path or a commit id capitalised is a different value: `dana-okoye` became
   * `Dana-okoye`, which is not that person's handle, and
   * `jobs/ExportTenant.php` would become `Jobs/ExportTenant.php`, which is not that file.
   * So every one of these opens with "a", "the" or "replied", and the recorded values sit
   * inside the sentence or in the fields beside it.
   *
   * `test/outcomes.spec.ts` holds it: for every kind, capitalising the sentence leaves
   * every value the detail carried unchanged.
   */
  sentence: string
  /**
   * The author's words, verbatim, or `null` for an outcome that carries none.
   *
   * **Always a record, never markup.** It is interpolated wherever it is drawn —
   * `test/step-pages.nuxt.spec.ts` holds every page of a review to that — so a reply
   * containing a backtick, a `<script>` or GitHub's own markdown arrives as the
   * characters the author typed.
   */
  quote: string | null
  /** The login that acted, as GitHub spells it, or `null` when the detail names none. */
  by: string | null
  /** The commit a change was seen in, at seven characters, or `null`. */
  commit: string | null
  /**
   * The file a change was seen in, as the record spells it, or `null`.
   *
   * Beside the sentence rather than inside it, for the reason above: a path cannot open a
   * sentence that is going to be capitalised. A page with a column for it draws it there;
   * the queue's one line does without, and the finding key it carries is how a reader
   * reaches the path.
   */
  file: string | null
  /**
   * Whether the recorded time is when this happened, or only when it was seen.
   *
   * True for a resolved thread: GitHub does not say when a thread was resolved, so the
   * harvester records the moment it looked and writes `time_basis: "observed"` to say so.
   * A page that printed that as "resolved 3 min ago" would be dating the harvest and
   * calling it the act.
   */
  observed: boolean
}

/**
 * What a developer's own decision reads as, by kind.
 *
 * The eight the developer and the agents write. **Keyed by `OutcomeKind` rather than by
 * `string`**, so a kind added to the contract is a type error here rather than a wire
 * value with its underscores swapped for spaces — which is how `thread_resolved` would
 * once have read as "thread resolved" on the one group built from exactly that kind.
 *
 * The three the harvester writes are composed from their fields below and are here only so
 * that the record is exhaustive, and so that an outcome of one of those kinds whose detail
 * holds nothing usable still has a sentence.
 */
const SAID: Record<OutcomeKind, string> = {
  cut: 'cut from this round',
  regraded: 'regraded',
  rewritten: 'rewritten',
  kept: 'kept as it was written',
  rejected: 'rejected, with a reason',
  ticketed: 'moved to a ticket',
  withdrawn: 'withdrawn — it no longer stands',
  raised_again: 'raised again',
  replied: 'replied to',
  thread_resolved: 'its thread resolved',
  code_changed_after: 'the code it points at changed afterwards',
}

/**
 * The kind's own phrase, **and an answer for a kind this build has never heard of.**
 *
 * `SAID` is a `Record<OutcomeKind, string>`, which makes a kind added to the contract a
 * type error here — at compile time, in this repository. It says nothing about run time:
 * the dashboard is generated and embedded by `cargo build`, so a binary rebuilt with a
 * newer server and an older `web/.output/public` serves kinds this bundle has no entry
 * for, and `SAID[kind]` is then `undefined`. `sentence(undefined)` throws on `.trim()`,
 * which takes out the whole render rather than one row. `editVerb` guards exactly this,
 * in as many words, with `VERBS[kind] ?? kind`.
 */
function phraseFor(kind: OutcomeKind): string {
  return SAID[kind] ?? String(kind).replace(/_/g, ' ')
}

/** Nothing but the kind's own phrase: the answer when a detail holds nothing to compose from. */
function bare(kind: OutcomeKind): OutcomeSaid {
  return {
    sentence: phraseFor(kind),
    quote: null,
    by: null,
    commit: null,
    file: null,
    observed: false,
  }
}

/**
 * Who acted, from the detail's login, else from the actor the store recorded.
 *
 * The login first, because it is the name on GitHub and the one the developer will
 * recognise. The actor is the fallback and is deliberately vague: `actor_for` maps a reply
 * from any login that is not the pull request's author to `developer`, so "the author" is
 * a claim only the login supports.
 */
function who(login: unknown, actor: Actor | null | undefined): string | null {
  const named = text(login)
  if (named != null) return named
  if (actor === 'author') return 'the author'
  if (actor === 'developer') return 'someone on your side'
  return null
}

/** A reply, as the author's words. */
function saidOfReply(held: HarvestFacts, actor: Actor | null | undefined): OutcomeSaid {
  const by = who(held.login, actor)
  const body = text(held.body)
  if (body == null) {
    /*
     * A reply whose text the record does not hold. Reachable: `outcome add --kind replied`
     * records one with no detail at all, and the row has to say that the reply itself is
     * not here rather than draw an empty quotation.
     */
    return {
      sentence: by == null
        ? 'a reply was recorded, and the record does not hold what it said'
        : `a reply from ${by}, and the record does not hold what they said`,
      quote: null,
      by,
      commit: null,
      file: null,
      observed: false,
    }
  }
  return {
    sentence: by == null ? 'a reply' : `a reply from ${by}`,
    quote: body,
    by,
    commit: null,
    file: null,
    observed: false,
  }
}

/** A resolved thread. The time is when it was seen, never when it happened. */
function saidOfResolution(held: HarvestFacts, actor: Actor | null | undefined): OutcomeSaid {
  const by = who(held.resolved_by, actor)
  return {
    sentence: by == null ? 'the thread was resolved' : `the thread was resolved by ${by}`,
    quote: null,
    by,
    commit: null,
    file: null,
    /*
     * **`=== 'observed'`, which is what the sentence beside it always said.** The harvester
     * writes `time_basis: "observed"` on every resolution; an outcome recorded by hand
     * carries no basis, and the time it was given is then the time it was given. Written as
     * `!== 'github'`, a hand-recorded resolution with any JSON detail took the harvester's
     * reading — the row said "seen 2 days ago" and the page drew the footnote explaining
     * that GitHub does not timestamp a resolution, about an outcome GitHub never saw.
     */
    observed: text(held.time_basis) === 'observed',
  }
}

/**
 * The code a finding points at, after the review read it.
 *
 * **Three sentences, because the record holds three different facts**, and the one in the
 * middle is the one the issue names: `lines_changed` is false for a finding whose *file*
 * changed while the lines it is about did not. Saying "the code at line 93 changed" there
 * would be wrong about the only thing the reader is going to check.
 *
 * The third is a finding with no line at all — a claim about a whole file, which has no
 * lines to have changed. `ChangedFile::touches_line` answers false for it by definition,
 * so without this case every file-level finding would read "but not line null".
 */
function saidOfCodeChange(held: HarvestFacts): OutcomeSaid {
  const commit = shortCommit(held.commit)
  const line = lineNumber(held.line)
  const file = text(held.file)
  const inCommit = commit == null ? '' : ` in ${commit}`
  const removed = text(held.file_status) === 'removed'

  const said = (sentence: string): OutcomeSaid => ({
    sentence,
    quote: null,
    by: null,
    commit,
    file,
    observed: false,
  })

  if (removed) return said(`the file it points at was removed${inCommit}`)
  if (line == null) return said(`the file it points at changed${inCommit}`)
  if (held.lines_changed === true) return said(`the code at line ${line} changed${inCommit}`)
  /*
   * **`=== false`, not "anything but true".** "but not line 93" is a positive claim that
   * the line is still what the review read, and a detail that carries no `lines_changed`
   * at all supports no such claim — which is reachable through
   * `outcome add --kind code_changed_after --detail '{…}'`, the only way these are
   * recorded on a review no harvest has read. Asserting a negative the record does not
   * hold is exactly what "it describes; it does not grade" forbids.
   */
  if (held.lines_changed === false) return said(`the file changed${inCommit}, but not line ${line}`)
  return said(`the file it points at changed${inCommit}`)
}

/**
 * One outcome, in words — **the only way an outcome reaches a reader.**
 *
 * Every kind answers something. A detail that holds no JSON object falls through to the
 * kind's own phrase with the prose appended where there is any, which is what a
 * hand-recorded rejection reason is; a detail that holds one is composed from its named
 * fields and never shown.
 */
export function outcomeSaid(outcome: OutcomeLike): OutcomeSaid {
  const held = facts(outcome.detail)

  if (held != null) {
    switch (outcome.kind) {
      case 'replied': return saidOfReply(held, outcome.actor)
      case 'thread_resolved': return saidOfResolution(held, outcome.actor)
      case 'code_changed_after': return saidOfCodeChange(held)
      /*
       * A kind the developer writes, whose detail somebody stored as JSON. The fields mean
       * nothing here, so the kind's phrase is the whole answer — which is the rule holding:
       * the object is not printed.
       */
      default: return bare(outcome.kind)
    }
  }

  const prose = freeDetail(outcome.detail)

  /*
   * **A reply whose detail is prose is a reply, and the prose is what was said.** Not a
   * contrived case: `review-desk outcome add --kind replied --detail "…"` records exactly
   * this, and every invented reply in `app/data/reviews.json` is one, because they were
   * written before the harvester existed. Treating it as a note *about* the reply gave
   * "replied to — The export is already chunked upstream", which puts the author's own
   * sentence after a dash as though somebody else had summarised it.
   */
  if (outcome.kind === 'replied') {
    return saidOfReply({ body: prose ?? undefined }, outcome.actor)
  }

  if (prose == null) return bare(outcome.kind)
  return { ...bare(outcome.kind), sentence: `${phraseFor(outcome.kind)} — ${prose}` }
}

/**
 * One outcome as one line, for a cell that has room for one.
 *
 * `F10 · the code at line 93 changed in 65eff63`, which is the issue's own example. A
 * reply's words are quoted into the line, with their whitespace collapsed: the queue's
 * cell is one line and a reply written as three paragraphs would otherwise arrive with its
 * newlines in it. The row that draws this clips what does not fit, and the whole string is
 * in the document either way, so a screen reader reads all of it.
 */
export function outcomeLine(outcome: OutcomeLike & { finding_key?: string }): string {
  const said = outcomeSaid(outcome)
  const quoted = said.quote == null
    ? said.sentence
    : `${said.sentence}: “${said.quote.replace(/\s+/g, ' ').trim()}”`
  return outcome.finding_key == null ? quoted : `${outcome.finding_key} · ${quoted}`
}

/**
 * How a pull request ended, or that it has not — **the fourth fact the harvest records.**
 *
 * It is not an outcome: the harvester writes it to `Review.pr_state` and `pr_state_at`, and
 * the contract serves the first and not the second. So the sentence says what happened and
 * not when, which is the honest reading of what a client can see.
 *
 * `null` is not "open": it is nothing having looked. Every review is in that state until
 * `review-desk harvest` runs over it, and a page that drew it as open would be reporting a
 * measurement nobody took — the same rule `prStateLabel` keeps for the queue's row, in the
 * words a page has room for.
 */
export function prEndLabel(state: 'open' | 'merged' | 'closed' | null | undefined): string {
  if (state === 'merged') return 'The pull request was merged'
  if (state === 'closed') return 'The pull request was closed without being merged'
  if (state === 'open') return 'The pull request is still open'
  return 'Nothing has read this pull request back from GitHub'
}

/**
 * What the queue's row heads an outcome with — "The author replied to F10".
 *
 * **It says who did what, and the group it is drawn in is what makes that answerable.**
 * `InboxReplyEvent` carries no actor, and the row used to head every one of these "The
 * author replied to F10" whatever the kind was: over a `code_changed_after` that credits
 * the author with the harvester's own observation, and over a resolution with a reply.
 * The group is built from outcomes whose actor is the author or the harvester
 * (`Store::reviews_with_author_activity`), and the harvester attributes a code change to
 * nobody, so the kind settles it — a reply or a resolution in this group is the author's,
 * and a code change is an observation.
 *
 * A function per kind rather than a string with the key spliced in, because the key sits
 * in a different place in each sentence, and `Record<OutcomeKind, …>` is what makes a kind
 * added to the contract a type error here.
 */
const HEADLINE: Record<OutcomeKind, (key: string) => string> = {
  replied: key => `The author replied to ${key}`,
  thread_resolved: key => `The author resolved ${key}’s thread`,
  code_changed_after: key => `The code under ${key} changed`,
  rejected: key => `${key} was rejected, with a reason`,
  raised_again: key => `${key} was raised again`,
  cut: key => `${key} was cut from this round`,
  regraded: key => `${key} was regraded`,
  rewritten: key => `${key} was rewritten`,
  kept: key => `${key} was kept as it was written`,
  ticketed: key => `${key} was moved to a ticket`,
  withdrawn: key => `${key} was withdrawn`,
}

/**
 * What the queue's row offers to do about one outcome.
 *
 * **Worded with the heading, because it was left behind by it.** `cameBackHeadline` tells a
 * `code_changed_after` from a reply; the row's button went on saying "Read the reply" over
 * both, offering an act that did not happen — and `events[0]` is a code change whenever the
 * lowest-numbered finding's first outcome is one, which is every author who pushed a fix
 * and never wrote anything.
 *
 * All of them go to the same place: the review, at the step it has got to. What changes is
 * what the developer is told they will find there.
 */
const ACTION: Record<OutcomeKind, string> = {
  replied: 'Read the reply',
  thread_resolved: 'Read the thread',
  code_changed_after: 'See what changed',
  rejected: 'Read their reason',
  raised_again: 'Read what came back',
  cut: 'Read what came back',
  regraded: 'Read what came back',
  rewritten: 'Read what came back',
  kept: 'Read what came back',
  ticketed: 'Read what came back',
  withdrawn: 'Read what came back',
}

/** The row's button for one outcome. Falls back for the reason the heading does. */
export function cameBackAction(kind: OutcomeKind): string {
  return ACTION[kind] ?? 'Read what came back'
}

/**
 * The row's heading for one outcome: who did what, to which finding.
 *
 * **Falls back rather than throwing**, for the reason `phraseFor` above gives: this is an
 * index-and-call, so a kind this bundle has no entry for was a
 * `TypeError: HEADLINE[event.kind] is not a function` inside the queue's `nextAction` —
 * which takes out the whole queue, where the function this replaced merely returned
 * nothing for that one row.
 */
export function cameBackHeadline(event: { kind: OutcomeKind, finding_key: string }): string {
  const said = HEADLINE[event.kind]
  return said ? said(event.finding_key) : `Something came back on ${event.finding_key}`
}

/**
 * When an outcome happened — or, for one GitHub does not timestamp, when it was seen.
 *
 * **Two words of difference, and they are the difference between a fact and an
 * observation.** GitHub does not say when a review thread was resolved, so the harvester
 * records the moment it looked; a row reading "resolved 3 min ago" over that would be
 * dating the harvest and calling it the act. The reader is told once, on the page, what
 * "seen" means; every row that carries one says it.
 */
export function outcomeWhen(
  said: OutcomeSaid,
  at: Instant | null | undefined,
  now: Date = new Date(),
): string {
  const when = sinceLabel(at, now)
  return said.observed ? `seen ${when}` : when
}
