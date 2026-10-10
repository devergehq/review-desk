/*
 * What the end-to-end test needs: a seeded database, a binary serving it, and a fresh one
 * beside it.
 *
 * **Everything here goes through the command line and the binary.** Nothing writes SQL,
 * nothing imports the store crate, and nothing stubs a request — the point of this test is
 * that the whole path works, so a shortcut anywhere in the setup would be a shortcut
 * through the thing being tested. The seed is the commands an agent session would run, in
 * the order it would run them.
 */

import { execFileSync, spawn, spawnSync, type ChildProcess } from 'node:child_process'
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

/** The contract's invented brief at real length, which the three step pages are seeded from. */
const REAL_LENGTH_BRIEF = resolve('../contract/examples/confirmBrief.response.real-length.json')

/**
 * A diagram field holding something that is not a diagram.
 *
 * **This is not invented.** It is what a review session wrote into a brief on 10 October
 * 2026 — a flow drawn by hand in text, in the field the dashboard hands to mermaid. mermaid
 * rejected it and left its own error graphic at the bottom of every page the developer
 * opened afterwards, which is DEV-880. A session will write one of these again, so the
 * seed carries one and `review-desk.spec.ts` asserts what the page does with it.
 *
 * It is on the *waived* review rather than the main one, so that the main review's valid
 * `flowchart` keeps proving that a diagram still draws.
 */
export const TEXT_SKETCH = `BEFORE (shared surface, implicit flow record)

consumer app --> GET /oauth/authorize --(guest)--> redirect()->guest() writes url.intended
    |
    v
GET /login  (SHARED staff page)
    |
    FortifyServiceProvider reads url.intended`

/** The binary, as `cargo build` leaves it. */
export const BINARY = resolve(
  process.env.REVIEW_DESK_BIN ?? '../target/debug/review-desk',
)

/** Where the two servers' addresses are left for the specs to read. */
export const ADDRESSES = resolve('e2e/.addresses.json')

/**
 * What `global-setup.ts` leaves for the specs.
 *
 * Declared once and used by the writer and both readers, which is what keeps the three in
 * step: each spec runs in its own process, so the file is the only thing they share, and
 * each spec used to describe its shape again with a different subset of the fields.
 */
export interface Addresses {
  seeded: string
  seededDatabase: string
  empty: string
  reviewId: number
  /**
   * A third review carrying the **structured** brief: the twelve rows DEV-882 added, at the
   * length the approved boards draw. It is what the Understand, Alternatives and Solution
   * pages are seeded from, and what their screenshots are taken of.
   *
   * Deliberately in nobody's queue, like the waived review below: its brief is confirmed
   * and it has no draft, so it changes none of the Inbox's counts and is reachable only by
   * going to it. A third row in "Needs you" would rewrite every other assertion in the
   * suite for a review that is here to be looked at rather than acted on.
   */
  structuredReviewId: number
  /**
   * A second review on the same repository whose brief was recorded as needing no
   * confirmation. It is deliberately not in anybody's queue: no brief to confirm and no
   * draft to approve, so it changes none of the Inbox's counts and is only reachable by
   * going to it.
   */
  waivedReviewId: number
}

/** The addresses, as the setup wrote them. */
export function readAddresses(): Addresses {
  return JSON.parse(readFileSync(ADDRESSES, 'utf8')) as Addresses
}

/** One running server, and the database it is serving. */
export interface Serving {
  url: string
  database: string
  process: ChildProcess
  /** Everything it has printed, so a death has its last words rather than none. */
  said: () => string
}

/** Run one `review-desk` command against a database, and hand back what it printed. */
export function cli(database: string, args: string[]): string {
  return execFileSync(BINARY, args, {
    env: { ...process.env, REVIEW_DESK_DB: database },
    encoding: 'utf8',
  })
}

/** Run one command and read its `--json` answer. */
export function cliJson(database: string, args: string[]): Record<string, never> {
  return JSON.parse(cli(database, [...args, '--json']))
}

/** A directory that dies with the test run. */
export function scratch(): string {
  return mkdtempSync(join(tmpdir(), 'review-desk-e2e-'))
}

/**
 * The record an agent session would have written by the time the developer is needed.
 *
 * Layer 1 passed, layer 2 wrote a brief and is waiting on the developer, layers 3 to 5 ran,
 * and layer 6 wrote a draft. That is the state DEV-862's end-to-end criterion starts from:
 * the inbox listing a review with an unconfirmed brief, and a draft behind it waiting to be
 * approved once the brief is confirmed.
 *
 * **Every fact here is written by the command line, including the draft.** The developer's
 * two acts — confirming the brief and approving the draft — are deliberately absent,
 * because there is no command for either. That is the boundary this test exists to exercise.
 */
/**
 * The invented brief, with the nine fields the server derives taken back off it.
 *
 * `contract/examples/confirmBrief.response.real-length.json` is a **response**, and
 * `NewBrief` and every one of its sub-structures is `serde(deny_unknown_fields)` — so
 * feeding the example to `brief put` as it stands is refused, naming the first extra field
 * and nothing about the other eight. What is removed is exactly what the server works out
 * for itself: the review it belongs to, whether it is confirmed and when, the blind pass's
 * derived state, when it was last written, a part's line totals (summed from its files so
 * the card cannot disagree with the list) and the `F`-prefixed keys (built from a sequence
 * number, because the dashboard builds no ids).
 *
 * **Stripping rather than hand-writing a second copy** is the point: the pages, their unit
 * tests and these screenshots are then all drawn from one invented example, and an example
 * that changes changes all three.
 */
function structuredBrief(): Record<string, unknown> {
  const served = JSON.parse(readFileSync(REAL_LENGTH_BRIEF, 'utf8')) as Record<string, unknown>

  const {
    review_id: _reviewId,
    confirmed: _confirmed,
    confirmed_at: _confirmedAt,
    blind_pass_state: _blindPassState,
    updated_at: _updatedAt,
    ...written
  } = served

  written.parts = (written.parts as Array<Record<string, unknown>>).map(
    ({ lines_added: _added, lines_removed: _removed, ...part }) => part,
  )
  written.open_choices = (written.open_choices as Array<Record<string, unknown>>).map(
    ({ finding_key: _key, ...choice }) => choice,
  )
  written.disagreements = (written.disagreements as Array<Record<string, unknown>>).map(
    ({ finding_key: _key, ...clash }) => clash,
  )

  /*
   * **Recorded as needing no confirmation, and the reason is in the record.**
   *
   * There is no `review-desk brief confirm`: the binary's own help says "Confirming one is
   * in the dashboard", because confirming is the developer's act and the command line
   * cannot perform it — which is the boundary this whole suite exists to exercise. So an
   * unconfirmed brief seeded here would put a third row in the Inbox's "Needs you" and
   * rewrite every count the other specs assert.
   *
   * This is the third of layer 2's three states rather than a way round the second: a brief
   * an agent recorded as needing no confirmation is settled, unsigned and in nobody's
   * queue. None of the three step pages draws the confirmation state at all — DEV-878's
   * pages do — so what is seeded here is exactly what they read either way.
   */
  written.confirmation_required = false
  written.confirmation_not_required_because
    = 'the brief gate is off for this repository (brief_gate: never)'

  return written
}

/**
 * The thirteen findings the structured review's parts account for.
 *
 * **Every path is one the invented brief's own `parts[].files[].path` names**, because the
 * Solution page joins the review's findings to a part by file — `contract/README.md`: "the
 * review's findings whose `file` is one of `brief.parts[].files[].path`" — and a seed whose
 * findings sat elsewhere would leave every part reading "nothing found here" while the
 * page looked like it was working.
 *
 * Four of the sequence numbers are load-bearing. `brief_open_choices` and
 * `brief_disagreements` each carry a foreign key to `findings (review_id, seq)`, and the
 * invented example points at 3, 7, 9 and 13 — so those findings have to exist **before**
 * `brief put` runs, or the write is refused by the database rather than by a validator.
 */
const STRUCTURED_FINDINGS: Array<{
  lens: string
  severity: string
  claim: string
  file: string
  line: number
  disposition: string
  disposition_reason?: string
  evidence?: string
}> = [
  {
    lens: 'sibling diff',
    severity: 'should',
    claim: 'The context is built with an empty allowed-columns list when the report '
      + 'definition names none, and an empty list means every column downstream.',
    file: 'app/Reporting/ExportContext.php',
    line: 44,
    disposition: 'in_draft',
    evidence: 'PROVED by a throwaway test: a definition with allowed_columns null '
      + 'exported all 31 columns.',
  },
  {
    lens: 'repetition',
    severity: 'nit',
    claim: 'The superseded accessor is still public with nothing marking it superseded.',
    file: 'app/Reporting/TenantContext.php',
    line: 12,
    disposition: 'held_back',
    disposition_reason: 'a nit; it goes in the ticket',
  },
  {
    lens: 'boundary',
    severity: 'should',
    claim: 'A schedule whose tenant row has gone is dispatched anyway, and the job fails '
      + 'after the context is built rather than before.',
    file: 'app/Console/Commands/DispatchScheduledExports.php',
    line: 61,
    disposition: 'in_draft',
  },
  {
    lens: 'naming',
    severity: 'nit',
    claim: 'The class converts a schedule to a context and is named for the direction '
      + 'rather than for the thing it makes.',
    file: 'app/Reporting/ScheduleToContext.php',
    line: 18,
    disposition: 'in_draft',
  },
  {
    lens: 'boundary',
    severity: 'question',
    claim: 'Two schedules for one tenant running at once would write the audit entry '
      + 'twice; nothing says whether that can happen.',
    file: 'app/Reporting/ExportPipeline.php',
    line: 73,
    disposition: 'in_draft',
  },
  {
    lens: 'sibling diff',
    severity: 'should',
    claim: 'The policy is applied after the query is built, so a hidden column is still '
      + 'selected from the database before being dropped.',
    file: 'app/Reporting/ColumnPolicy.php',
    line: 31,
    disposition: 'in_draft',
  },
  {
    lens: 'boundary',
    severity: 'question',
    claim: 'Nothing in the change says what should happen to a schedule whose tenant has '
      + 'been deleted.',
    file: 'app/Jobs/RunScheduledExport.php',
    line: 96,
    disposition: 'in_draft',
  },
  {
    lens: 'repetition',
    severity: 'nit',
    claim: 'The redaction pattern list repeats the one in the web exporter.',
    file: 'app/Reporting/Redaction.php',
    line: 15,
    disposition: 'suppressed',
    disposition_reason: 'the web copy goes in the next change; raising it twice is noise',
  },
  {
    lens: 'contract',
    severity: 'blocker',
    claim: 'The column policy applies only where a report definition names its allowed '
      + 'columns, and the backfill leaves older definitions naming none.',
    file: 'app/Reporting/ColumnPolicy.php',
    line: 52,
    disposition: 'in_draft',
    evidence: 'PROVED by running the backfill against a copy of the development '
      + 'database: 14 of 22 definitions were left with allowed_columns null.',
  },
  {
    lens: 'naming',
    severity: 'nit',
    claim: 'The removed branch leaves a comment describing it.',
    file: 'app/Reporting/ReportBuilder.php',
    line: 28,
    disposition: 'in_draft',
  },
  {
    lens: 'boundary',
    severity: 'should',
    claim: 'The web path now constructs its context from the request without checking '
      + 'that the signed-in person may export that report.',
    file: 'app/Http/Controllers/ExportController.php',
    line: 41,
    disposition: 'in_draft',
  },
  {
    /*
     * In `app/Audit/RecordsAnExport.php`, which the brief's fourth part claims with
     * `in_diff: false`. It is the finding that makes a part's claim on a file outside the
     * diff worth anything: without it that file belongs to no part and so does this.
     */
    lens: 'coverage',
    severity: 'question',
    claim: 'The audit trait is in no diff, so whether its entry shape fits a scheduled '
      + 'export was not checked by reading the change alone.',
    file: 'app/Audit/RecordsAnExport.php',
    line: 22,
    disposition: 'in_draft',
  },
  {
    lens: 'contract',
    severity: 'should',
    claim: 'An export is skipped only when the tenant row is gone; a soft-deleted tenant '
      + 'still exports, which the description says it does not.',
    file: 'app/Jobs/RunScheduledExport.php',
    line: 118,
    disposition: 'in_draft',
  },
]

/**
 * A third review, with the structured brief the three step pages draw.
 *
 * **In the order the record requires**: the review, then its findings — because the brief's
 * open choices and disagreements point at four of them by sequence number through a foreign
 * key — then the brief, then the layers so the trail has an outcome under every step.
 */
function seedStructured(directory: string, database: string): number {
  const opened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '514',
    '--head',
    'd41c8e2',
    '--title',
    'Give a scheduled export its own context',
    '--ticket',
    'ABC-402',
    '--changed-lines',
    '1442',
    '--not-checked',
    'The reporting front end, which reads the same definitions but was not changed.',
  ])
  const reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) {
    throw new Error(`review open printed no id: ${opened}`)
  }

  STRUCTURED_FINDINGS.forEach((finding, index) => {
    const path = join(directory, `structured-finding-${index}.json`)
    writeFileSync(path, JSON.stringify(finding))
    cli(database, ['finding', 'add', '--review', String(reviewId), '--file', path])
  })

  const brief = join(directory, 'structured-brief.json')
  writeFileSync(brief, JSON.stringify(structuredBrief()))
  cli(database, ['brief', 'put', '--review', String(reviewId), '--file', brief])

  const layers: Array<[number, string, string]> = [
    [1, 'done', 'passes, 2 warnings'],
    [2, 'done', 'brief settled'],
    [3, 'done', '8 options, C built'],
    [4, 'done', 'right shape'],
    [5, 'done', '13 findings, 1 blocker'],
    [6, 'waiting_on_you', 'waiting on you'],
    [7, 'pending', 'after posting'],
  ]
  for (const [number, state, detail] of layers) {
    cli(database, [
      'layer',
      'set',
      '--review',
      String(reviewId),
      '--layer',
      String(number),
      '--state',
      state,
      '--detail',
      detail,
    ])
  }

  return reviewId
}

export function seed(
  directory: string,
): {
  database: string
  reviewId: number
  waivedReviewId: number
  structuredReviewId: number
} {
  const database = join(directory, 'seeded.db')

  const opened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '412',
    '--head',
    '3f9c2ab',
    '--title',
    'Retry failed webhook deliveries',
    '--ticket',
    'ABC-231',
    '--changed-lines',
    '214',
    '--not-checked',
    'The admin screens, which this change does not touch.',
  ])
  const reviewId = Number(opened.match(/\d+/)?.[0])
  if (!Number.isInteger(reviewId)) {
    throw new Error(`review open printed no id: ${opened}`)
  }

  /*
   * **With its options, which `brief put` does take.** `NewBrief` carries
   * `options: Vec<BriefOption>` and `option_map` is marked superseded; the command's own
   * `--help` still lists the four older fields and says anything else is refused, which is
   * what this seed believed until it was checked. Layer 3 is where a review writes the
   * alternatives it considered before it read the diff, and the One review screen's "Ways
   * to solve it" is the only place they are shown — so a seed without them leaves a panel
   * untested and a screenshot that does not show the screen.
   */
  const brief = join(directory, 'brief.json')
  writeFileSync(
    brief,
    JSON.stringify({
      problem:
        'A webhook delivery that fails once is never tried again, so a customer whose '
        + 'endpoint was briefly down loses the event entirely.',
      diagram: 'flowchart LR\n  A[Event] --> B[Deliver]\n  B -->|fails| C[Dropped]',
      options: [
        {
          key: 'A',
          title: 'Retry inside the request',
          argument_for: 'The smallest change, and no new table.',
          argument_against: 'Holds the request open for as long as the endpoint is down.',
          chosen: false,
        },
        {
          key: 'B',
          title: 'Queue with backoff',
          argument_for: 'Meets the 24-hour window the ticket asks for.',
          argument_against: 'Needs an idempotency key, which is finding F1.',
          chosen: true,
        },
        {
          key: 'C',
          title: 'Dead-letter and replay by hand',
          argument_for: 'Nothing is ever delivered twice.',
          argument_against: 'Somebody has to notice, which is the failure being fixed.',
          chosen: false,
        },
      ],
      approach_verdict: 'B is the right shape for this ticket.',
    }),
  )
  cli(database, ['brief', 'put', '--review', String(reviewId), '--file', brief])

  const findings = [
    {
      lens: 'sibling diff',
      severity: 'blocker',
      claim: 'A retried delivery is sent without an idempotency key.',
      file: 'jobs/DeliverWebhook.php',
      line: 58,
      evidence: 'throwaway test, 2 deliveries for 1 event',
      disposition: 'in_draft',
    },
    {
      lens: 'boundary',
      severity: 'should',
      claim: 'The backoff schedule has no ceiling, so a dead endpoint is retried forever.',
      file: 'jobs/DeliverWebhook.php',
      line: 91,
      disposition: 'in_draft',
    },
    {
      lens: 'repetition',
      severity: 'nit',
      claim: 'Backoff schedule is repeated in two files.',
      disposition: 'held_back',
      disposition_reason: 'a nit; it goes in the ticket',
    },
  ]

  findings.forEach((finding, index) => {
    const path = join(directory, `finding-${index}.json`)
    writeFileSync(path, JSON.stringify(finding))
    cli(database, ['finding', 'add', '--review', String(reviewId), '--file', path])
  })

  const layers: Array<[number, string, string]> = [
    [1, 'done', 'passed'],
    [2, 'waiting_on_you', 'waiting on you'],
    [3, 'done', '3 options'],
    [4, 'done', 'queue with backoff'],
    [5, 'done', '3 findings'],
    [6, 'waiting_on_you', 'waiting on you'],
    [7, 'pending', 'once the session posts it'],
  ]
  for (const [number, state, detail] of layers) {
    cli(database, [
      'layer',
      'set',
      '--review',
      String(reviewId),
      '--layer',
      String(number),
      '--state',
      state,
      '--detail',
      detail,
    ])
  }

  /*
   * The draft, as the session writes it. The body raises the two findings that are in the
   * draft and holds the third back, which is what the lint below reports — and what the
   * Draft preview screen shows as "2 of 2".
   */
  const body = join(directory, 'draft.md')
  writeFileSync(
    body,
    `The queue-with-backoff approach fits the ticket, and the two findings below are
about the retry path rather than the choice.

### Blocker · F1 · A retried delivery is sent without an idempotency key

A second delivery of the same event is indistinguishable from a new one, so a customer
who was briefly down receives it twice.

### Should · F2 · The backoff schedule has no ceiling

A permanently dead endpoint is retried forever, which keeps the queue busy with
deliveries nobody will receive.

<details>
<summary>How I checked</summary>

Ran the job twice against a local endpoint that 500s once, and counted the deliveries.

</details>
`,
  )

  const lint = join(directory, 'lint.json')
  writeFileSync(
    lint,
    JSON.stringify({
      passes: true,
      visible_words: 96,
      word_budget: 150,
      findings_in_body: 2,
      findings_expected: 2,
      raised_before_on_pr: 0,
      checked_at: Math.floor(Date.now() / 1000),
    }),
  )
  cli(database, [
    'draft',
    'put',
    '--review',
    String(reviewId),
    '--file',
    body,
    '--lint',
    lint,
  ])

  /*
   * **A second review whose brief needs no confirmation**, written by the same command
   * line an agent uses. DEV-864 asks for this state end to end, and it is the one place
   * an agent decides a developer step was unnecessary — so the test that it is absent
   * from the Inbox needs a real record to be absent from, not a stub.
   *
   * 61 changed lines, which is why the reason reads the way it does: the 200-line
   * threshold belongs to the caller and Review Desk does not hold it.
   */
  const waivedOpened = cli(database, [
    'review',
    'open',
    '--repo',
    'acme/portal',
    '--pr',
    '418',
    '--head',
    'bb41e07',
    '--title',
    'Say "not measured" rather than null in the delivery log',
    '--changed-lines',
    '61',
  ])
  const waivedReviewId = Number(waivedOpened.match(/\d+/)?.[0])
  if (!Number.isInteger(waivedReviewId)) {
    throw new Error(`review open printed no id: ${waivedOpened}`)
  }

  const waivedBrief = join(directory, 'waived-brief.json')
  writeFileSync(
    waivedBrief,
    JSON.stringify({
      problem:
        'The delivery log prints `null` for a latency it never measured, which reads as '
        + 'a measurement of zero.',
      diagram: TEXT_SKETCH,
      confirmation_required: false,
      confirmation_not_required_because:
        '61 changed lines, under this caller\'s 200-line threshold',
    }),
  )
  cli(database, [
    'brief',
    'put',
    '--review',
    String(waivedReviewId),
    '--file',
    waivedBrief,
  ])

  return {
    database,
    reviewId,
    waivedReviewId,
    structuredReviewId: seedStructured(directory, database),
  }
}

/** A database with nothing in it: what the first `review-desk serve` serves. */
export function fresh(directory: string): string {
  const database = join(directory, 'fresh.db')
  /* Created and migrated without writing a record, which is what `db version` is for. */
  cli(database, ['db', 'version'])
  return database
}

/**
 * Start the binary on a free port and wait until it answers.
 *
 * Port 0, so two servers and two test runs cannot collide, and the URL comes from the
 * binary's own `--json` rather than from a guess.
 */
export async function serve(database: string): Promise<Serving> {
  const child = spawn(BINARY, ['serve', '--port', '0', '--json'], {
    env: { ...process.env, REVIEW_DESK_DB: database },
    stdio: ['ignore', 'pipe', 'pipe'],
  })

  /*
   * **Both pipes are drained and kept, which they were not.**
   *
   * A CI run of this suite failed with three asset requests dying at the transport level
   * and no API request made at all: the server had stopped answering partway through a
   * page load, and there was nothing to say why, because its stderr went into a pipe
   * nothing read. A test harness that discards the one artefact that would explain a death
   * turns a diagnosable failure into a ten-second timeout on a blank page.
   *
   * Draining matters on its own, too. An unread pipe fills, and a child that blocks writing
   * to it stops serving — which is the shape of the bug `fdb0b40` fixed in the serve test
   * for DEV-861, where the test killed the server it had started.
   */
  let printed = ''
  child.stdout?.on('data', (chunk: Buffer) => {
    printed += chunk.toString()
  })
  child.stderr?.on('data', (chunk: Buffer) => {
    printed += chunk.toString()
  })

  const said = () => printed.trim()

  /*
   * **An exit is noticed and remembered**, so that a server which dies mid-run fails the
   * next thing that touches it with its own words rather than with a timeout.
   */
  let died: string | null = null
  child.on('exit', (code, signal) => {
    died = `the server exited (code ${code}, signal ${signal}). It said:\n${said()}`
  })

  const url = await new Promise<string>((done, failed) => {
    /*
     * Polled rather than matched on every chunk. The regular expression runs over
     * everything printed so far, and the server prints for the life of the run — matching
     * per chunk would re-scan an ever-growing string for an address found in its first line.
     */
    const looking = setInterval(() => {
      const match = printed.match(/"url":"([^"]+)"/)
      if (match) {
        clearInterval(looking)
        clearTimeout(giveUp)
        done(match[1]!)
      }
      else if (died) {
        clearInterval(looking)
        clearTimeout(giveUp)
        failed(new Error(`${died}`))
      }
    }, 50)

    const giveUp = setTimeout(() => {
      clearInterval(looking)
      failed(new Error(`the server printed no address in 20s. It said:\n${said()}`))
    }, 20_000)

    child.on('error', failed)
  })

  /*
   * Answering, not merely started. `/health` is the server saying its database is open and
   * migrated, which is what the first screen read needs to be true.
   */
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (died) throw new Error(died)
    try {
      const answer = await fetch(`${url}api/v1/health`)
      if (answer.ok) return { url, database, process: child, said }
    }
    catch {
      /* Not up yet. */
    }
    await new Promise(wait => setTimeout(wait, 100))
  }

  child.kill()
  throw new Error(`${url} never answered its health endpoint. It said:\n${said()}`)
}

/**
 * The servers this process started, for the checks below.
 *
 * Each spec file runs in its own process and the servers were started in `globalSetup`'s,
 * so a spec cannot hold their handles. What it can do is ask whether the addresses still
 * answer, which is what `stillServing` does when it has no handle to inspect.
 */
export function serversUnderTest(): Array<{ url: string }> {
  const addresses = readAddresses()
  return [{ url: addresses.seeded }, { url: addresses.empty }]
}

/**
 * Fail now, with the server's own words, if either server has died.
 *
 * Called at the top of each spec. A dead server otherwise shows up as a browser timing out
 * on a blank page ten seconds later, which says nothing about what happened and points at
 * the assertion rather than at the cause.
 */
export function stillServing(servers: Array<{ url: string }>): void {
  for (const server of servers) {
    const answered = spawnSync(
      'curl',
      ['-s', '-o', '/dev/null', '-m', '5', '-w', '%{http_code}', `${server.url}api/v1/health`],
      { encoding: 'utf8' },
    )
    if (answered.stdout.trim() !== '200') {
      throw new Error(
        `${server.url} is not answering its health endpoint (curl said `
        + `${answered.stdout.trim() || 'nothing'}). A server under test has died; the run's `
        + 'log carries what it printed.',
      )
    }
  }
}

/** Stop a server, and wait for it to be gone. */
export async function stop(serving: Serving): Promise<void> {
  if (serving.process.exitCode != null) return
  const ended = new Promise<void>(done => serving.process.once('exit', () => done()))
  serving.process.kill('SIGTERM')
  await Promise.race([ended, new Promise(wait => setTimeout(wait, 5_000))])
  if (serving.process.exitCode == null) serving.process.kill('SIGKILL')
}
