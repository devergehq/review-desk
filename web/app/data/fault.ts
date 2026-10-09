/*
 * What went wrong, as the one shape every screen renders.
 *
 * **A module of its own, and the reason is the bundle rather than tidiness.** The composable
 * needs `asDeskError` on every write, and sample mode needs `DeskRequestError` to refuse the
 * way the server refuses — so both used to import `api.ts`, which put the whole HTTP client
 * in the app shell's static graph. Nuxt then emitted it as a `<link rel="prefetch">`: a
 * chunk the app cannot mount without, fetched at the lowest priority Chrome has. On a slow
 * runner the mount waited on it and the end-to-end test timed out against an empty
 * `<div id="__nuxt">`, twice.
 *
 * This file imports nothing but types, so everything that needs the error shape can have it
 * without the client that produces most of them.
 */

import type { DeskError } from '~/types/desk'

/**
 * A refusal, thrown so that a read and a write report failure the same way.
 *
 * It carries the contract's `Error` object rather than a message, because the screens branch
 * on `code` and never on wording — a `github_unavailable` is the developer's environment and
 * names a command that fixes it, while a `conflict` is about the record and names what state
 * it is in.
 */
export class DeskRequestError extends Error {
  readonly desk: DeskError

  constructor(desk: DeskError) {
    super(desk.message)
    this.name = 'DeskRequestError'
    this.desk = desk
  }
}

/**
 * One `DeskError`, built in one place.
 *
 * Four literals carried the same `field: null, expected: null` and two of them carried the
 * same sentence, so a field added to `DeskError` meant four edits and the two sentences
 * could drift apart while meaning the same thing.
 */
export function fault(
  code: DeskError['code'],
  message: string,
  status: number | null = null,
): DeskError {
  return { code, message, field: null, expected: null, status }
}

/** There is nothing on the other end. The one sentence for it, written once. */
export const NOT_LISTENING = 'Nothing is listening on this address.'

/**
 * Whatever went wrong, as the one error shape the screens render.
 *
 * Anything that is not already a refusal is `unreachable`: the server was stopped, or never
 * started, or the port moved — a state DEV-862 requires a screen to handle with a message
 * and a retry, and one no contract code describes. Calling it `store_unavailable` would tell
 * the developer their database is broken when their server is simply not running.
 */
export function asDeskError(thrown: unknown): DeskError {
  if (thrown instanceof DeskRequestError) return thrown.desk
  return fault('unreachable', NOT_LISTENING)
}
