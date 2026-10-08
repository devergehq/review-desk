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

import { execFileSync, spawn, type ChildProcess } from 'node:child_process'
import { mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

/** The binary, as `cargo build` leaves it. */
export const BINARY = resolve(
  process.env.REVIEW_DESK_BIN ?? '../target/debug/review-desk',
)

/** Where the two servers' addresses are left for the specs to read. */
export const ADDRESSES = resolve('e2e/.addresses.json')

/** One running server, and the database it is serving. */
export interface Serving {
  url: string
  database: string
  process: ChildProcess
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
export function seed(directory: string): { database: string, reviewId: number } {
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

  const brief = join(directory, 'brief.json')
  writeFileSync(
    brief,
    JSON.stringify({
      problem:
        'A webhook delivery that fails once is never tried again, so a customer whose '
        + 'endpoint was briefly down loses the event entirely.',
      diagram: 'flowchart LR\n  A[Event] --> B[Deliver]\n  B -->|fails| C[Dropped]',
      approach_verdict: 'The queue-with-backoff approach fits the ticket.',
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

  return { database, reviewId }
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

  const url = await new Promise<string>((done, failed) => {
    let printed = ''
    const timer = setTimeout(
      () => failed(new Error(`the server printed no address: ${printed}`)),
      20_000,
    )

    child.stdout?.on('data', (chunk: Buffer) => {
      printed += chunk.toString()
      const match = printed.match(/"url":"([^"]+)"/)
      if (match) {
        clearTimeout(timer)
        done(match[1]!)
      }
    })
    child.on('error', failed)
    child.on('exit', code =>
      failed(new Error(`the server exited with ${code} before printing an address`)),
    )
  })

  /*
   * Answering, not merely started. `/health` is the server saying its database is open and
   * migrated, which is what the first screen read needs to be true.
   */
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const answer = await fetch(`${url}api/v1/health`)
      if (answer.ok) return { url, database, process: child }
    }
    catch {
      /* Not up yet. */
    }
    await new Promise(wait => setTimeout(wait, 100))
  }

  child.kill()
  throw new Error(`${url} never answered its health endpoint`)
}

/** Stop a server, and wait for it to be gone. */
export async function stop(serving: Serving): Promise<void> {
  if (serving.process.exitCode != null) return
  const ended = new Promise<void>(done => serving.process.once('exit', () => done()))
  serving.process.kill('SIGTERM')
  await Promise.race([ended, new Promise(wait => setTimeout(wait, 5_000))])
  if (serving.process.exitCode == null) serving.process.kill('SIGKILL')
}
