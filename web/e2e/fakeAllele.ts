/*
 * **A stand-in for Allele's control socket, so an end-to-end test can run two way.**
 * DEV-1335's test seam; DEV-1336's phone test uses it to ask from the bottom sheet.
 *
 * Every other spec serves with `--allele-socket` pointed at a path that is never there, so
 * the panel is read only (or one way, once a session has written). Two way needs Allele's
 * socket to answer, and CI has no Allele — so this answers in its place, the way
 * `crates/server/tests/common/mod.rs`'s `fake_allele` does for the Rust tests: one JSON line
 * in, one JSON line out, on a Unix socket, over Allele's own wire
 * (`crates/server/src/allele.rs`).
 *
 * It answers `sessions_status`, for whichever session it is asked about, in the state it was
 * started with. It starts nothing and runs nothing; a question asked from the page is kept in
 * the record, as it is with a real Allele, and nothing reads it.
 *
 * **Handing a question over is opt-in** (DEV-1383). Started with `{ prompt: true }`, it also
 * answers DEV-1382's `allele.ext/1` `hello`, granting `prompt`, and `sessions_prompt` with
 * `prompted`, keeping each notice it was handed in `prompts()`. Without it, `hello` is refused
 * as an Allele from before `sessions_prompt` refuses it, so every other spec's page reads as
 * it did before and the server falls back to the watcher.
 *
 * ```ts
 * const allele = await fakeAllele()            // 'idle' unless told otherwise
 * const two = await serve(database, { allele: allele.socket })
 * …
 * await stop(two); await allele.close()
 * ```
 *
 * The review asked about must have a session linked (`review link --session`): with none,
 * the server never asks Allele, and the panel is not two way however Allele answers.
 */

import { mkdtempSync, rmSync } from 'node:fs'
import { createServer } from 'node:net'
import { join } from 'node:path'

/** Allele's words for a session's state, as `sessions_status` carries them. */
export type AlleleState = 'idle' | 'running' | 'awaiting_input' | 'suspended'

export interface FakeAllele {
  /** The socket's path, for `serve(…, { allele })` or `--allele-socket`. */
  socket: string
  /** How many questions it has been asked. */
  asked: () => number
  /** Every `sessions_prompt` it was sent, oldest first. Empty unless started with `prompt`. */
  prompts: () => Array<{ session_id: string, prompt: string }>
  close: () => Promise<void>
}

export async function fakeAllele(
  state: AlleleState = 'idle',
  { prompt = false }: { prompt?: boolean } = {},
): Promise<FakeAllele> {
  /* Under `/tmp` and short: a Unix socket's path is limited to about a hundred bytes. */
  const directory = mkdtempSync('/tmp/rd-allele-')
  const socket = join(directory, 'a.sock')
  let asked = 0
  const prompts: Array<{ session_id: string, prompt: string }> = []

  /* One answer line for one request line. */
  const answer = (line: string): string => {
    let request: { op?: string, session_id?: string, prompt?: string }
    try {
      request = JSON.parse(line) as typeof request
    }
    catch {
      return JSON.stringify({ result: 'error', code: 'bad_request', message: 'not JSON' })
    }
    const sessionId = request.session_id ?? 'unknown'
    switch (request.op) {
      case 'sessions_status':
        return JSON.stringify({ result: 'status', session: { session_id: sessionId, state, state_age_secs: 5 } })
      case 'hello':
        return prompt
          ? JSON.stringify({ result: 'hello', contract_versions: ['allele.ext/1'], epoch: 'e2e', granted: ['observe', 'prompt'], dispatch_limits: null })
          : JSON.stringify({ result: 'error', code: 'bad_request', message: 'could not parse hello: unknown variant `prompt`' })
      case 'sessions_prompt':
        prompts.push({ session_id: sessionId, prompt: request.prompt ?? '' })
        return JSON.stringify({
          result: 'prompted',
          session: { session_id: sessionId, name: 'e2e', was_state: state, now_state: 'running' },
        })
      default:
        return JSON.stringify({ result: 'error', code: 'bad_request', message: `not faked: ${request.op}` })
    }
  }

  /* A handover says hello and then prompts on one connection, so every line is answered. */
  const server = createServer((connection) => {
    let buffered = ''
    connection.setEncoding('utf8')
    connection.on('data', (chunk: string) => {
      buffered += chunk
      let end = buffered.indexOf('\n')
      while (end >= 0) {
        const line = buffered.slice(0, end)
        buffered = buffered.slice(end + 1)
        asked += 1
        connection.write(`${answer(line)}\n`)
        end = buffered.indexOf('\n')
      }
    })
    connection.on('error', () => undefined)
  })

  await new Promise<void>((done, failed) => {
    server.once('error', failed)
    server.listen(socket, () => done())
  })

  return {
    socket,
    asked: () => asked,
    prompts: () => [...prompts],
    close: () => new Promise<void>((done) => {
      server.close(() => {
        rmSync(directory, { recursive: true, force: true })
        done()
      })
    }),
  }
}
