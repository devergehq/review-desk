/*
 * Two binaries, two databases, started once for the whole run.
 *
 * Started here rather than per test because starting a server and seeding a database is a
 * second of work and the tests do not change either: everything that writes writes through
 * the dashboard, which is the point. The addresses are left in a file for the specs, since
 * each spec runs in its own process.
 *
 * The fresh database has a server of its own rather than being the same one emptied. A
 * database with nothing in it is a state the first run of `review-desk serve` is really in,
 * and emptying a seeded one would leave the sequence counters where the seed left them —
 * which is not what a new database looks like.
 */

import { writeFileSync } from 'node:fs'

import { ADDRESSES, fresh, scratch, seed, serve, stop, type Serving } from './harness'

export default async function globalSetup() {
  const directory = scratch()

  const { database, reviewId } = seed(directory)
  const seeded = await serve(database)
  const empty = await serve(fresh(directory))

  writeFileSync(
    ADDRESSES,
    JSON.stringify(
      {
        seeded: seeded.url,
        seededDatabase: seeded.database,
        empty: empty.url,
        reviewId,
        binary: process.env.REVIEW_DESK_BIN ?? '../target/debug/review-desk',
      },
      null,
      2,
    ),
  )

  const running: Serving[] = [seeded, empty]
  return async () => {
    for (const server of running) await stop(server)
  }
}
