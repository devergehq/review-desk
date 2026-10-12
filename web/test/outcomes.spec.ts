import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

import {
  cameBackHeadline,
  freeDetail,
  outcomeLine,
  outcomeSaid,
  outcomeWhen,
  prEndLabel,
  shortCommit,
} from '../app/utils/outcomes'
import { sentence } from '../app/utils/format'

import type { OutcomeKind } from '../app/types/desk'

/**
 * Every kind the contract declares.
 *
 * `satisfies` rather than a cast, so a kind leaving the contract stops this file
 * type-checking; the length is asserted beside it so a kind *added* to the contract and
 * not to this list fails at run time as well. Between them, "every outcome kind has a
 * sentence" is a claim with a mechanism behind it rather than a list somebody kept up to
 * date.
 */
const KINDS = [
  'cut',
  'regraded',
  'rewritten',
  'kept',
  'rejected',
  'ticketed',
  'withdrawn',
  'raised_again',
  'replied',
  'thread_resolved',
  'code_changed_after',
] satisfies OutcomeKind[]

/* A kind added to the contract and not to the list above fails here. */
const KINDS_IN_CONTRACT = 11

/*
 * **The four facts the harvest records, each as a sentence** — DEV-958's second acceptance
 * criterion, and the one the first criterion rests on: "no queue row, page or tooltip
 * prints an outcome's stored detail as JSON" is only reachable if there is something else
 * to print.
 *
 * The details here are the shapes `crates/harvest/src/run.rs` really writes. Their field
 * names are read out of that file rather than typed out — see "the detail the harvester
 * writes" below — so a field added in Rust fails here instead of arriving on a screen as a
 * key nobody worded.
 */

/** `crates/harvest/src/run.rs`, which is the only place a harvested detail is composed. */
const RUN = resolve('../crates/harvest/src/run.rs')

/**
 * A `replied` detail, as `harvest_thread` writes it.
 *
 * The reply is 108 words, which is DEV-958's "one reply over 80 words". It is invented, and
 * it is written at the length an author really answers a blocker at — a short reply is a
 * length every one of these screens survives, so it measures nothing.
 */
const LONG_REPLY
  = 'I pushed a fix for this one but I want to flag that it is not the fix you suggested. '
    + 'Taking the column policy out of the query builder means the scheduled export and the '
    + 'interactive export stop sharing it, and the interactive one is the path with the test '
    + 'coverage, so I would rather keep them together and gate the scheduled path on the '
    + 'same policy object. That is what the new commit does. If you still want them split I '
    + 'can do it, but it should be its own change with its own tests rather than something I '
    + 'slip into a fix for a reporting bug.'

const REPLIED = JSON.stringify({
  comment_id: 2_315_004_411,
  in_reply_to: 2_314_990_002,
  login: 'dana-okoye',
  body: LONG_REPLY,
  time_basis: 'github',
})

const RESOLVED = JSON.stringify({
  thread: 2_314_990_002,
  resolved_by: 'patrick-d',
  time_basis: 'observed',
})

const CODE_CHANGED = JSON.stringify({
  file: 'app/Reporting/ColumnPolicy.php',
  line: 93,
  lines_changed: true,
  file_status: 'modified',
  commit: '65eff6321a04f7b9c0d8e5a1b2c3d4e5f6071829',
  basis: 'compare 633e04a3f...65eff6321',
})

/* ------------------------- the detail is never printed ------------------------- */

describe('a stored detail that is a record is never printed', () => {
  it('is checked against every kind the contract declares', () => {
    expect(KINDS).toHaveLength(KINDS_IN_CONTRACT)
    expect(new Set(KINDS).size).toBe(KINDS_IN_CONTRACT)
  })

  it.each([
    ['a reply', REPLIED],
    ['a resolved thread', RESOLVED],
    ['code that changed', CODE_CHANGED],
  ])('%s holds back its JSON', (_name, detail) => {
    expect(freeDetail(detail)).toBeNull()
  })

  it('hands back a detail a person typed, because that is prose', () => {
    expect(freeDetail('the allowed list is empty on purpose for internal reports')).toBe(
      'the allowed list is empty on purpose for internal reports',
    )
  })

  it('holds back a JSON array as well as an object', () => {
    /*
     * Not reachable from the harvester, which writes objects. It is reachable from
     * `outcome add --detail '[1,2]'`, and an array printed raw is the same defect wearing
     * different brackets.
     */
    expect(freeDetail('[1,2]')).toBeNull()
  })

  it('hands back a bare value, which JSON.parse also accepts', () => {
    /*
     * `JSON.parse('"65eff63"')` succeeds and `JSON.parse('7')` succeeds. Neither is a
     * record of a read, so neither is held back — and testing the parse for being an
     * object is what tells them apart.
     */
    expect(freeDetail('65eff63')).toBe('65eff63')
    expect(freeDetail('7')).toBe('7')
  })

  it('treats an empty or blank detail as no detail', () => {
    expect(freeDetail('')).toBeNull()
    expect(freeDetail('   ')).toBeNull()
    expect(freeDetail(null)).toBeNull()
    expect(freeDetail(undefined)).toBeNull()
  })

  /**
   * **The guard across the language boundary.** The fields are read out of the Rust that
   * writes them, so a harvester that starts recording a fifth field fails this test until
   * `HarvestFacts` in `app/utils/outcomes.ts` names it too.
   *
   * It reads the file rather than a fixture because a fixture would be the copy that
   * drifts. `test/architecture.spec.ts` reads `app/` the same way, for the same reason.
   */
  it('names every field the harvester writes into a detail', () => {
    const rust = readFileSync(RUN, 'utf8')
    const details = [...rust.matchAll(/let detail = json!\(\{([^}]*)\}\)/g)]
    expect(
      details.length,
      'crates/harvest/src/run.rs no longer composes three details with json!',
    ).toBe(3)

    const fields = new Set(
      details.flatMap(block => [...block[1]!.matchAll(/"([a-z_]+)":/g)].map(hit => hit[1]!)),
    )
    /* The three shapes between them, which is what the module's own table claims. */
    expect([...fields].sort()).toEqual([
      'basis',
      'body',
      'comment_id',
      'commit',
      'file',
      'file_status',
      'in_reply_to',
      'line',
      'lines_changed',
      'login',
      'resolved_by',
      'thread',
      'time_basis',
    ])

    const module = readFileSync(resolve('app/utils/outcomes.ts'), 'utf8')
    const declared = module.slice(
      module.indexOf('interface HarvestFacts'),
      module.indexOf('function facts('),
    )
    for (const field of fields) {
      expect(declared, `HarvestFacts does not name ${field}`).toContain(`${field}?:`)
    }
  })

  /**
   * And the rule stated over the one path a screen has.
   *
   * Every kind in the contract, against the harvester's three details and against a detail
   * somebody stored as JSON on a kind that has none — which is the case the `default` arm
   * of `outcomeSaid` exists for.
   */
  it.each(KINDS)('%s composes a sentence with no brace in it', (kind) => {
    for (const detail of [REPLIED, RESOLVED, CODE_CHANGED, '{"anything":"at all"}']) {
      const said = outcomeSaid({ kind, detail, actor: 'author' })
      expect(said.sentence.length, `${kind} said nothing`).toBeGreaterThan(0)
      expect(said.sentence, `${kind} printed its detail`).not.toContain('{')
      expect(said.sentence).not.toContain('"')
      expect(outcomeLine({ kind, detail, finding_key: 'F10' })).not.toContain('{')
    }
  })
})

/* --------------------------- fact one · the reply ----------------------------- */

describe('a reply reads as the author’s words, quoted', () => {
  it('names who replied and carries the reply verbatim', () => {
    const said = outcomeSaid({ kind: 'replied', detail: REPLIED, actor: 'author' })
    expect(said.sentence).toBe('a reply from dana-okoye')
    expect(said.quote).toBe(LONG_REPLY)
    expect(said.by).toBe('dana-okoye')
    expect(said.observed).toBe(false)
  })

  it('is the reply that DEV-958 asks for at over 80 words', () => {
    expect(LONG_REPLY.trim().split(/\s+/).length).toBeGreaterThan(80)
    expect(LONG_REPLY.trim().split(/\s+/)).toHaveLength(108)
  })

  it('quotes the reply into a one-line cell with its whitespace collapsed', () => {
    const detail = JSON.stringify({
      login: 'dana-okoye',
      body: 'Fixed.\n\nSee the new test.',
      time_basis: 'github',
    })
    expect(outcomeLine({ kind: 'replied', detail, finding_key: 'F3' })).toBe(
      'F3 · a reply from dana-okoye: “Fixed. See the new test.”',
    )
  })

  it('falls back to the actor when the detail names no login', () => {
    const detail = JSON.stringify({ body: 'Fixed.', time_basis: 'github' })
    expect(outcomeSaid({ kind: 'replied', detail, actor: 'author' }).sentence)
      .toBe('a reply from the author')
    expect(outcomeSaid({ kind: 'replied', detail, actor: 'developer' }).sentence)
      .toBe('a reply from someone on your side')
    expect(outcomeSaid({ kind: 'replied', detail }).sentence).toBe('a reply')
  })

  it('says the reply itself is not on the record, rather than quoting nothing', () => {
    const said = outcomeSaid({
      kind: 'replied',
      detail: JSON.stringify({ login: 'dana-okoye' }),
      actor: 'author',
    })
    expect(said.sentence).toBe(
      'a reply from dana-okoye, and the record does not hold what they said',
    )
    expect(said.quote).toBeNull()
  })
})

/* ---------------------- fact two · the thread was resolved -------------------- */

describe('a resolved thread says who, and that the time is when it was seen', () => {
  it('names who resolved it', () => {
    const said = outcomeSaid({ kind: 'thread_resolved', detail: RESOLVED, actor: 'developer' })
    expect(said.sentence).toBe('the thread was resolved by patrick-d')
    expect(said.quote).toBeNull()
  })

  /*
   * GitHub does not timestamp a resolution, so the harvester records when it looked and
   * writes `time_basis: "observed"`. A page that printed that as the act would be dating
   * the harvest.
   */
  it('marks the recorded time as an observation', () => {
    expect(outcomeSaid({ kind: 'thread_resolved', detail: RESOLVED }).observed).toBe(true)
  })

  it('does not mark it an observation when the detail says the time is GitHub’s', () => {
    const detail = JSON.stringify({ resolved_by: 'patrick-d', time_basis: 'github' })
    expect(outcomeSaid({ kind: 'thread_resolved', detail }).observed).toBe(false)
  })

  it('says the thread was resolved when nobody is named', () => {
    const detail = JSON.stringify({ thread: 1, time_basis: 'observed' })
    expect(outcomeSaid({ kind: 'thread_resolved', detail }).sentence)
      .toBe('the thread was resolved')
  })
})

/* ------------------- fact three · the code changed afterwards ----------------- */

describe('code that changed after the review reads as what changed, and where', () => {
  it('is the issue’s own sentence', () => {
    const said = outcomeSaid({ kind: 'code_changed_after', detail: CODE_CHANGED })
    expect(said.sentence).toBe('the code at line 93 changed in 65eff63')
    expect(said.commit).toBe('65eff63')
    expect(outcomeLine({ kind: 'code_changed_after', detail: CODE_CHANGED, finding_key: 'F10' }))
      .toBe('F10 · the code at line 93 changed in 65eff63')
  })

  /*
   * **The case DEV-958 names in the same breath.** `lines_changed` is false for a finding
   * whose file changed while the lines it is about did not, and "the code at line 93
   * changed" would be wrong about the one thing the reader is going to check.
   */
  it('says the file changed and the lines did not, where that is what happened', () => {
    const detail = JSON.stringify({ ...JSON.parse(CODE_CHANGED), lines_changed: false })
    const said = outcomeSaid({ kind: 'code_changed_after', detail })
    expect(said.sentence).toBe('the file changed in 65eff63, but not line 93')
    /* The path is beside the sentence, because a path cannot open one. See `OutcomeSaid`. */
    expect(said.file).toBe('app/Reporting/ColumnPolicy.php')
  })

  /*
   * A finding that names a file and no line — a claim about the whole file.
   * `ChangedFile::touches_line` answers false for it by definition, so without its own
   * sentence it would read "but not line null".
   */
  it('says the file changed, for a finding with no line', () => {
    const detail = JSON.stringify({
      file: 'app/Reporting/ColumnPolicy.php',
      line: null,
      lines_changed: false,
      file_status: 'modified',
      commit: '65eff6321a04',
    })
    expect(outcomeSaid({ kind: 'code_changed_after', detail }).sentence).toBe(
      'the file it points at changed in 65eff63',
    )
  })

  it('says a file was removed, where GitHub said that is what happened to it', () => {
    const detail = JSON.stringify({
      file: 'app/Reporting/LegacyExport.php',
      line: 12,
      lines_changed: true,
      file_status: 'removed',
      commit: '65eff6321a04',
    })
    expect(outcomeSaid({ kind: 'code_changed_after', detail }).sentence).toBe(
      'the file it points at was removed in 65eff63',
    )
  })

  it('says what changed even where the detail holds no commit', () => {
    const detail = JSON.stringify({ file: 'a.php', line: 3, lines_changed: true })
    const said = outcomeSaid({ kind: 'code_changed_after', detail })
    expect(said.sentence).toBe('the code at line 3 changed')
    expect(said.commit).toBeNull()
  })

  /**
   * **It does not say the finding was fixed, and that is a correction.**
   *
   * `replyLabel` worded this kind as "F10 fixed in a later commit" — a judgement the
   * record does not support and `crates/harvest` refuses to make. The lines a finding
   * points at are no longer the lines that were reviewed; they may have been fixed,
   * rewritten, moved, or deleted with the feature.
   */
  it('grades nothing', () => {
    for (const lines_changed of [true, false]) {
      const detail = JSON.stringify({ ...JSON.parse(CODE_CHANGED), lines_changed })
      const said = outcomeSaid({ kind: 'code_changed_after', detail })
      expect(said.sentence).not.toContain('fixed')
      expect(said.sentence).not.toContain('still')
    }
  })
})

/* -------------------- fact four · how the pull request ended ------------------ */

describe('how the pull request ended', () => {
  it.each([
    ['merged', 'The pull request was merged'],
    ['closed', 'The pull request was closed without being merged'],
    ['open', 'The pull request is still open'],
  ] as const)('%s', (state, said) => {
    expect(prEndLabel(state)).toBe(said)
  })

  /*
   * **Null is not open.** It is nothing having looked, which is every review until
   * `review-desk harvest` has run over it — the rule `prStateLabel` keeps for the queue.
   */
  it('says nothing has looked, rather than drawing an unharvested review as open', () => {
    expect(prEndLabel(null)).toBe('Nothing has read this pull request back from GitHub')
    expect(prEndLabel(undefined)).toBe('Nothing has read this pull request back from GitHub')
    expect(prEndLabel(null)).not.toContain('open')
  })
})

/* ------------------------------ the rest of it -------------------------------- */

describe('a developer’s own decision', () => {
  it('reads as the decision, with the reason they typed', () => {
    expect(outcomeSaid({ kind: 'rejected', detail: 'the empty list is deliberate' }).sentence)
      .toBe('rejected, with a reason — the empty list is deliberate')
  })

  it('reads as the decision alone where no reason was recorded', () => {
    expect(outcomeSaid({ kind: 'cut' }).sentence).toBe('cut from this round')
    expect(outcomeSaid({ kind: 'ticketed', detail: null }).sentence).toBe('moved to a ticket')
  })
})

describe('a commit is quoted at seven characters', () => {
  it('shortens a full sha and leaves a short one alone', () => {
    expect(shortCommit('65eff6321a04f7b9c0d8e5a1b2c3d4e5f6071829')).toBe('65eff63')
    expect(shortCommit('65eff63')).toBe('65eff63')
  })

  it('answers null for nothing at all', () => {
    expect(shortCommit(null)).toBeNull()
    expect(shortCommit('')).toBeNull()
  })
})

describe('a reply recorded by hand', () => {
  /*
   * `review-desk outcome add --kind replied --detail "…"` records a reply with no JSON
   * around it, and every invented reply in `app/data/reviews.json` is one. The prose is
   * what was said, not a note about what was said: worded the other way it read "replied
   * to — The export is already chunked upstream", which puts the author's own sentence
   * after a dash as though somebody had summarised it.
   */
  it('reads as the reply, not as a note about one', () => {
    const said = outcomeSaid({
      kind: 'replied',
      detail: 'The export is already chunked upstream.',
      actor: 'author',
    })
    expect(said.sentence).toBe('a reply from the author')
    expect(said.quote).toBe('The export is already chunked upstream.')
  })

  it('still says a reply was recorded when it carries no words at all', () => {
    expect(outcomeSaid({ kind: 'replied', actor: 'author' }).sentence).toBe(
      'a reply from the author, and the record does not hold what they said',
    )
  })
})

describe('when an outcome happened', () => {
  const at = '2026-10-13T08:00:00Z'
  const now = new Date('2026-10-13T08:03:00Z')

  it('is the time itself for a reply, which GitHub timestamps', () => {
    const said = outcomeSaid({ kind: 'replied', detail: REPLIED })
    expect(outcomeWhen(said, at, now)).toBe('3 min ago')
  })

  /*
   * A resolution is not timestamped by GitHub, so the harvester records when it looked.
   * "resolved 3 min ago" over that would be dating the harvest and calling it the act.
   */
  it('is when it was seen for a resolution, which GitHub does not', () => {
    const said = outcomeSaid({ kind: 'thread_resolved', detail: RESOLVED })
    expect(outcomeWhen(said, at, now)).toBe('seen 3 min ago')
  })

  it('says the time is unrecorded rather than inventing one', () => {
    const said = outcomeSaid({ kind: 'replied', detail: REPLIED })
    expect(outcomeWhen(said, null, now)).toBe('at an unrecorded time')
  })
})

describe('the queue’s heading for what came back', () => {
  it.each([
    ['replied', 'The author replied to F10'],
    ['thread_resolved', 'The author resolved F10’s thread'],
    ['code_changed_after', 'The code under F10 changed'],
  ] as const)('%s', (kind, said) => {
    expect(cameBackHeadline({ kind, finding_key: 'F10' })).toBe(said)
  })

  /*
   * **The defect it closes.** The row headed every one of these "The author replied to
   * F10" whatever the kind was — crediting the author with the harvester's own
   * observation about the code, and calling a resolution a reply.
   */
  it('does not call the harvester’s observation a reply by the author', () => {
    const said = cameBackHeadline({ kind: 'code_changed_after', finding_key: 'F10' })
    expect(said).not.toContain('replied')
    expect(said).not.toContain('author')
  })

  it.each(KINDS)('%s has a heading', (kind) => {
    expect(cameBackHeadline({ kind, finding_key: 'F1' })).toContain('F1')
  })
})

describe('capitalising a sentence changes no value the record holds', () => {
  /**
   * **The rule, and the defect that produced it.** A caller that wants a standalone
   * sentence passes one through `sentence()`, which capitalises the first character — and
   * `dana-okoye replied` became `Dana-okoye replied`, which is not that person's handle.
   * The code review page did exactly that, and the same rule would have turned
   * `jobs/ExportTenant.php changed` into `Jobs/ExportTenant.php changed`.
   *
   * So no sentence opens with a value out of the record, and this is the measurement
   * rather than the comment: for every kind, over every detail the harvester writes, the
   * capitalised sentence still holds every value the detail carried.
   */
  const VALUES = ['dana-okoye', 'patrick-d', '65eff63', 'app/Reporting/ColumnPolicy.php']

  it.each(KINDS)('%s', (kind) => {
    for (const detail of [REPLIED, RESOLVED, CODE_CHANGED]) {
      const said = outcomeSaid({ kind, detail, actor: 'author' })
      const capitalised = sentence(said.sentence)
      for (const value of VALUES) {
        if (said.sentence.includes(value)) {
          expect(capitalised, `${kind} mangled ${value}`).toContain(value)
        }
      }
      /* And the first character really is capitalised, so the rule is doing some work. */
      expect(capitalised[0]).toBe(said.sentence[0]!.toUpperCase())
    }
  })

  it('opens every sentence with a word, never with a path or a login', () => {
    for (const kind of KINDS) {
      for (const detail of [REPLIED, RESOLVED, CODE_CHANGED, null]) {
        const said = outcomeSaid({ kind, detail, actor: 'author' }).sentence
        expect(said, `${kind} opens with "${said.split(' ')[0]}"`).toMatch(/^[a-z]+([ ,]|$)/)
      }
    }
  })
})

describe('no screen reads a stored detail for itself', () => {
  /**
   * **The rule, held over the source rather than over three renderings of it.**
   *
   * DEV-958 found the same defect in three places at once — the queue's author-reply row,
   * the code review page's "What came back", and the draft page's edits panel — and each
   * was one expression reaching for `Outcome.detail` or the `DraftEdit.summary` the server
   * copies it into. A test per page would have caught the three that existed; this catches
   * the fourth, in a component somebody writes six months from now.
   *
   * Named identifiers rather than the field, because the field name is not the problem: a
   * layer's `detail` is the store's own four words and is drawn on the trail on purpose.
   * What must not happen is an *outcome's* detail reaching a screen without being worded.
   */
  const SOURCES = ['app/components', 'app/pages', 'app/pages/reviews/[id]', 'app/utils']

  /** `outcomes.ts` is where a detail is read; `draft.ts` hands a summary to it. */
  const ALLOWED = ['app/utils/outcomes.ts', 'app/utils/draft.ts']

  /* The reach this forbids: an outcome-shaped thing's own stored field, used directly. */
  const REACHING = /\b(outcome|edit|event|came|first|latest)\s*\.\s*(detail|summary)\b/g

  /**
   * **Handing one over is allowed; reading it is not** — and the difference is the two
   * characters before the match.
   *
   * `detail: first.detail` is the queue's row passing the stored field *into*
   * `outcomeLine`, which is the whole fix. `first.detail ?? …` is the defect. An earlier
   * version of this check exempted any line that mentioned `outcomeLine(` anywhere, which
   * exempted the defect as well the moment it was written on the same line — measured by
   * putting the real defect back and watching this stay green.
   */
  function handedOver(before: string): boolean {
    return /(detail|summary):\s*$/.test(before)
  }

  it('finds the files it is about to check', () => {
    const found = SOURCES.flatMap(dir =>
      readdirSync(resolve(dir)).filter(name => /\.(ts|vue)$/.test(name)))
    expect(found.length).toBeGreaterThan(20)
  })

  /** Every reach this scan can see, as `path: line`. Empty is the rule holding. */
  function reaches(over: string[]): string[] {
    const offenders: string[] = []
    for (const dir of SOURCES) {
      for (const name of readdirSync(resolve(dir))) {
        if (!/\.(ts|vue)$/.test(name)) continue
        const path = `${dir}/${name}`
        if (over.includes(path)) continue
        const text = readFileSync(resolve(path), 'utf8')
        for (const line of text.split('\n')) {
          /* A comment arguing about the defect is not the defect. */
          if (/^\s*(\*|\/\/|<!--)/.test(line)) continue
          for (const hit of line.matchAll(REACHING)) {
            if (handedOver(line.slice(0, hit.index))) continue
            offenders.push(`${path}: ${line.trim()}`)
          }
        }
      }
    }
    return offenders
  }

  it('reaches for one only inside the two modules that word it', () => {
    expect(reaches(ALLOWED)).toEqual([])
  })

  /**
   * **And the scan would see one if it were there.**
   *
   * A check that cannot fail is a comment. `outcomes.ts` itself reads a detail — that is
   * its job — so running the scan with nothing exempted has to report it; if it reports
   * nothing, the regex has stopped matching and the test above is green about nothing.
   */
  it('is a scan that can fail', () => {
    expect(reaches([]).join(' ')).toContain('app/utils/outcomes.ts')
  })
})
