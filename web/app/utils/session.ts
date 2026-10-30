/*
 * What the strip under a review's trail says about its Allele session.
 *
 * Nothing here reads or writes a record, as nothing in `utils/` does. The reading comes
 * from `GET /reviews/{id}/session`, which is Allele's answer, not the record's — so every
 * sentence here says what Allele said, and says nothing when there was no one to ask.
 */

import type { SessionReading } from '~/types/desk'

import { durationLabel } from './format'

/**
 * How often a review page asks again while it is visible.
 *
 * The same five seconds the draft page re-reads a signed-off review at, and for the same
 * reason: this reports on something the page does not control, so it should not add much
 * delay of its own. The server answers within about a second whatever Allele does.
 */
export const SESSION_READ_EVERY_SECONDS = 5

/** Which of the strip's looks a reading takes. */
export type SessionTone = 'working' | 'waiting' | 'finished' | 'gone' | 'unknown' | 'unreachable'

/** One line on the strip: the state in a few words, and the facts after it. */
export interface SessionLine {
  tone: SessionTone
  /** The state, which is the bold part. */
  label: string
  /** What follows it, or `null` when there is nothing to add. */
  facts: string | null
}

/**
 * The strip's line for a reading, or `null` when the page should draw no strip.
 *
 * **Null is no Allele or no session**, which is how every review looked before the strip
 * existed — absent is silent. Every other reading draws something, including the two that
 * are Allele's or the socket's fault, because "can't reach Allele" is useful and a strip
 * that vanished when Allele stopped would look like nothing was wrong.
 */
export function sessionLine(reading: SessionReading | null | undefined): SessionLine | null {
  if (reading == null) return null
  const age = durationLabel(reading.state_age_secs)

  switch (reading.status) {
    case 'running':
      return { tone: 'working', label: 'Working', facts: age ? `for ${age}` : null }
    case 'awaiting_input':
      return {
        tone: 'waiting',
        label: 'Waiting on a permission prompt',
        facts: `${age ? `${age} · ` : ''}answer it in Allele`,
      }
    case 'finished':
      return {
        tone: 'finished',
        label: 'Finished its turn',
        facts: age ? `${age} ago, waiting for a prompt` : 'waiting for a prompt',
      }
    case 'gone':
      /*
       * **Suspended is not the same as gone in Allele, and the strip says which.** A
       * suspended session can be resumed there; a done one, or one Allele has no record
       * of, cannot. Neither will pick this review up on its own, which is why both read
       * as the same state.
       */
      if (reading.allele_state === 'suspended') {
        return {
          tone: 'gone',
          label: 'Session not running',
          facts: 'suspended in Allele · nothing will pick this review up until it is resumed',
        }
      }
      return {
        tone: 'gone',
        label: 'Session gone',
        facts: reading.allele_state === 'done'
          ? 'done in Allele · nothing will pick this review up'
          : 'Allele has no session with this id · nothing will pick this review up',
      }
    case 'unreachable':
      return {
        tone: 'unreachable',
        label: 'Can\'t reach Allele',
        facts: 'nothing on this page waits on it',
      }
    case 'unknown':
    default:
      return {
        tone: 'unknown',
        label: 'Unknown',
        facts: reading.detail ?? 'Allele\'s answer could not be read, so this is not a guess',
      }
  }
}

/**
 * Whether a reading is an answer about the session, as opposed to no answer.
 *
 * **What the draft page's forty-second sentence defers to.** That sentence guesses that
 * "the session may not be running" from silence on the record. Allele saying what the
 * session is doing makes the guess redundant, and wrong whenever the answer is "working".
 * Unreachable and unknown are not answers, so the guess stays for them, as it does with no
 * Allele at all.
 */
export function sessionAnswered(reading: SessionReading | null | undefined): boolean {
  return reading != null
    && (reading.status === 'running'
      || reading.status === 'awaiting_input'
      || reading.status === 'finished'
      || reading.status === 'gone')
}
