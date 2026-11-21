/*
 * **Who is answering this review, and what they are doing.** DEV-1333; the one helper the
 * conversation's header reads, which DEV-1335 owns from here and extends.
 *
 * `docs/design.md`, *The application shell*: who answers is the developer's choice, and
 * Review Desk never starts, resumes or drives Claude on its own. With Allele the
 * conversation is two way; with the developer's own session it is one way; with neither it
 * is read only (S11).
 *
 * **Today there are two modes, read off one thing: Allele's reading of the review's
 * session**, `GET /reviews/{id}/session`, which the server takes from the Allele socket
 * (`crates/server/src/allele.rs`). A reading means Allele is there and knows the session:
 * two way. No reading means no Allele or no session: nobody can be asked from here, so the
 * conversation is read only. Nothing here looks for a process or reads a file; one way
 * (DEV-1335) will be read off the record's replies, never off the machine.
 *
 * Nothing here reads or writes a record.
 */

import type { SessionReading } from '~/types/desk'

/** How the developer reaches the session that answers. One way arrives with DEV-1335. */
export type AnsweringMode = 'two_way' | 'none'

/** The dot beside the header's line. */
export type AnsweringTone = 'live' | 'busy' | 'stopped'

/** The conversation's header, as S6 draws it: who, and how they are right now. */
export interface Answering {
  mode: AnsweringMode
  /** Who answers, e.g. *The session that wrote this review*. */
  who: string
  /** Their state in a word or two, after the dot, e.g. *idle*. */
  state: string
  tone: AnsweringTone
}

/**
 * The header for one Allele reading.
 *
 * The state words are S6's — *idle* and *working* — and, for the readings S6 does not draw,
 * the strip's own (`sessionLine` in `utils/session.ts`) said shorter, so the two cannot come
 * to describe one reading differently.
 */
export function answering(reading: SessionReading | null | undefined): Answering {
  if (reading == null) {
    return {
      mode: 'none',
      who: 'No agent connected',
      state: 'you can read, not ask',
      tone: 'stopped',
    }
  }
  const who = 'The session that wrote this review'
  switch (reading.status) {
    case 'running':
      return { mode: 'two_way', who, state: 'working', tone: 'busy' }
    case 'finished':
      return { mode: 'two_way', who, state: 'idle', tone: 'live' }
    case 'awaiting_input':
      return { mode: 'two_way', who, state: 'waiting on a permission prompt', tone: 'busy' }
    case 'gone':
      return {
        mode: 'two_way',
        who,
        state: reading.allele_state === 'suspended' ? 'suspended in Allele' : 'gone',
        tone: 'stopped',
      }
    case 'unreachable':
      return { mode: 'two_way', who, state: 'can’t reach Allele', tone: 'stopped' }
    case 'unknown':
    default:
      return { mode: 'two_way', who, state: 'state unknown', tone: 'stopped' }
  }
}

/**
 * **A gone or unreachable session still takes a question**, deliberately: the question is
 * kept with the review, the line under it says *Not delivered · the session is gone*
 * (`deliveryLine`), and the fresh session the strip offers is what reads it. Refusing it here
 * would lose the developer's words to say what that line already says. DEV-1335 owns this
 * gate and may decide otherwise.
 *
 * Whether the composer takes a question in this mode. The one gate every send passes,
 * which DEV-1335 replaces with `canAsk(mode)` once one way exists.
 */
export function asksFromHere(mode: AnsweringMode): boolean {
  return mode === 'two_way'
}
