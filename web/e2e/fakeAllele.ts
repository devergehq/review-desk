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
 * It answers only `sessions_status`, for whichever session it is asked about, in the state
 * it was started with. It starts nothing and runs nothing; a question asked from the page is
 * kept in the record, as it is with a real Allele, and nothing reads it.
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
  close: () => Promise<void>
}

export async function fakeAllele(state: AlleleState = 'idle'): Promise<FakeAllele> {
  /* Under `/tmp` and short: a Unix socket's path is limited to about a hundred bytes. */
  const directory = mkdtempSync('/tmp/rd-allele-')
  const socket = join(directory, 'a.sock')
  let asked = 0

  const server = createServer((connection) => {
    let line = ''
    connection.setEncoding('utf8')
    connection.on('data', (chunk: string) => {
      line += chunk
      const end = line.indexOf('\n')
      if (end < 0) return
      asked += 1
      let sessionId = 'unknown'
      try {
        const request = JSON.parse(line.slice(0, end)) as { op?: string, session_id?: string }
        if (request.op !== 'sessions_status') {
          connection.end(`${JSON.stringify({ result: 'error', code: 'bad_request', message: `not faked: ${request.op}` })}\n`)
          return
        }
        sessionId = request.session_id ?? sessionId
      }
      catch {
        connection.end(`${JSON.stringify({ result: 'error', code: 'bad_request', message: 'not JSON' })}\n`)
        return
      }
      connection.end(`${JSON.stringify({
        result: 'status',
        session: { session_id: sessionId, state, state_age_secs: 5 },
      })}\n`)
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
    close: () => new Promise<void>((done) => {
      server.close(() => {
        rmSync(directory, { recursive: true, force: true })
        done()
      })
    }),
  }
}
