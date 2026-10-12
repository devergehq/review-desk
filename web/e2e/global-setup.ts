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

import { ADDRESSES, fresh, scratch, seed, serve, stop, type Addresses, type Serving } from './harness'

export default async function globalSetup() {
  const directory = scratch()

  const {
    database,
    reviewId,
    waivedReviewId,
    structuredReviewId,
    heldBackReviewId,
    postedReviewId,
    requestedReviewId,
  } = seed(directory)
  /* The two share nothing, so they boot together rather than one after the other. */
  const [seeded, empty] = await Promise.all([serve(database), serve(fresh(directory))])

  /*
   * Typed as `Addresses`, which both specs read it back as — so a field renamed here stops
   * compiling there rather than becoming `undefined` at run time.
   */
  const addresses: Addresses = {
    seeded: seeded.url,
    seededDatabase: seeded.database,
    empty: empty.url,
    reviewId,
    waivedReviewId,
    structuredReviewId,
    heldBackReviewId,
    postedReviewId,
    requestedReviewId,
  }
  writeFileSync(ADDRESSES, JSON.stringify(addresses, null, 2))

  const running: Serving[] = [seeded!, empty!]
  return async () => {
    await Promise.all(running.map(stop))
  }
}
