/*
 * **Who is answering this review, and what they are doing.** DEV-1333 drew the header from
 * it; DEV-1335 added one way and the gate every send passes.
 *
 * `docs/design.md`, *The application shell*: who answers is the developer's choice, and
 * Review Desk never starts, resumes or drives Claude on its own (S11):
 *
 * | Mode | When | The panel |
 * | -- | -- | -- |
 * | `two_way` | Allele has a reading of the review's session | ask here; Allele delivers it |
 * | `one_way` | no reading, and a session has written with `review-desk message reply` | what the session wrote; ask in your session; Copy a reference |
 * | `none` | neither | read only; what is kept still reads |
 *
 * **The mode is read off two things and nothing else.** Allele's reading of the review's
 * session, `GET /reviews/{id}/session`, which the server takes from Allele's socket
 * (`crates/server/src/allele.rs`, where an absent socket is silent and serves no reading);
 * and the record's own messages, where a session's reply is a session having written. No
 * setting chooses it, and nothing here, or behind it on the server, looks for a process,
 * reads a file of the developer's agent, or touches a login.
 *
 * Nothing here reads or writes a record.
 */

import type { ReviewMessage, SessionReading } from '~/types/desk'

/** How the developer reaches the session that answers. */
export type AnsweringMode = 'two_way' | 'one_way' | 'none'

/** The dot beside the header's line. `quiet` is S11's grey: nothing live to report. */
export type AnsweringTone = 'live' | 'busy' | 'stopped' | 'quiet'

/** The conversation's header: what it is called, who answers, and how they are right now. */
export interface Answering {
  mode: AnsweringMode
  /** The panel's heading: *Ask this review*, or S13's *From your session* in one way. */
  title: string
  /** Who answers, e.g. *The session that wrote this review*. */
  who: string
  /** Their state in a few words, after the dot, e.g. *idle · via Allele*. */
  state: string
  tone: AnsweringTone
}

/** What the mode needs of a message: whether a session wrote it. */
type Written = Pick<ReviewMessage, 'author'>

/**
 * **The mode, from Allele's reading and the record.**
 *
 * Any reading is Allele answering for the session, so two way — including *unreachable*,
 * where the socket is there and slow: a question asked then is kept, and the line under it
 * says why it has not been read (`deliveryLine`). With no reading, a reply in the record is a
 * session the developer runs having written here, so one way. Otherwise nobody.
 */
export function answeringMode(
  reading: SessionReading | null | undefined,
  messages: readonly Written[] = [],
): AnsweringMode {
  if (reading != null) return 'two_way'
  return messages.some(message => message.author === 'session') ? 'one_way' : 'none'
}

/**
 * The header for one Allele reading and the conversation so far.
 *
 * Two way's state words are S6's — *idle* and *working* — and, for the readings S6 does not
 * draw, the strip's own (`sessionLine` in `utils/session.ts`) said shorter, so the two cannot
 * come to describe one reading differently; S11 adds *via Allele*. One way's and none's are
 * S13's and S11's.
 */
export function answering(
  reading: SessionReading | null | undefined,
  messages: readonly Written[] = [],
): Answering {
  const mode = answeringMode(reading, messages)
  if (mode === 'one_way') {
    return {
      mode,
      title: 'From your session',
      who: 'Your own Claude Code session',
      state: 'one way · no Allele',
      tone: 'quiet',
    }
  }
  if (reading == null) {
    return {
      mode: 'none',
      title: 'Ask this review',
      who: 'No agent connected',
      state: 'read only',
      tone: 'quiet',
    }
  }
  const title = 'Ask this review'
  const who = 'The session that wrote this review'
  const via = (state: string, tone: AnsweringTone): Answering =>
    ({ mode: 'two_way', title, who, state: `${state} · via Allele`, tone })
  switch (reading.status) {
    case 'running':
      return via('working', 'busy')
    case 'finished':
      return via('idle', 'live')
    case 'awaiting_input':
      return via('waiting on a permission prompt', 'busy')
    case 'gone':
      return via(reading.allele_state === 'suspended' ? 'suspended in Allele' : 'gone', 'stopped')
    case 'unreachable':
      return { mode: 'two_way', title, who, state: 'can’t reach Allele', tone: 'stopped' }
    case 'unknown':
    default:
      return via('state unknown', 'stopped')
  }
}

/**
 * **Whether a question can leave this panel**: the one gate every send passes
 * (`ConversationPanel`'s `send`), the composer's and *Ask it again, in the format* alike.
 *
 * Only two way. One way has nobody Review Desk can reach — the developer asks in their own
 * session, and *Copy a reference* is how they point it here — and none has nobody at all.
 *
 * **A gone or unreachable session still takes a question**, deliberately, because the
 * question is kept with the review either way. A gone one: the line under it says *Not
 * delivered · the session is gone* (`deliveryLine`), and the fresh session the strip offers is
 * what reads it. An unreachable one: the line says *Not read yet*, and when the session
 * last checked for messages, which the record knows without Allele.
 * Refusing it would lose the developer's words to say what that line already says.
 */
export function canAsk(mode: AnsweringMode): boolean {
  return mode === 'two_way'
}
