/*
 * **Where an answer's cite goes, or that it goes nowhere.** DEV-1333, the one place a cite is
 * read; DEV-1334's suggestion cards read it too.
 *
 * The server checks that an answer's cites are there and are strings, and never resolves
 * them (`docs/answer-contract.md`, *Who checks it*). So this is the one place a cite meets
 * the record, and it holds one rule: **a cite is a link only when the record holds the thing
 * it names.** Anything else is drawn as the words the session wrote, never as a link that
 * lands on nothing.
 *
 * | Cite | Becomes | Only when |
 * | -- | -- | -- |
 * | `F9` | Findings with F9 selected, `findingPath` | the review has a finding keyed `F9` |
 * | `path/to/file.php:41` | that finding, as `F9` would | a finding sits at exactly that file and line |
 * | `criterion 3` | Understanding › *Ticket against what was built* | the review records a criterion at position 3 |
 * | `option C` | Understanding › *Approach* | the brief weighs an option keyed `C` |
 * | a commit SHA | the commit on GitHub | the review's repository makes a usable address |
 * | anything else | plain text | — |
 *
 * **A path is not linked to GitHub**, because a session cites `ExportController.php:41` as
 * readily as the full path, and an address built from a partial path is a broken link. It
 * links where the record says something is at that line, which is a finding, and nowhere
 * else.
 *
 * Nothing here reads or writes a record: it takes what `GET /reviews/{id}` served.
 */

import type { ReviewDetail } from '~/types/desk'

import { prUrl } from './links'
import { findingPath, tabPath } from './reviewTabs'
import { optionsOf } from './theCase'

/** The parts of a review a cite is resolved against. */
export type CiteSource = Pick<ReviewDetail, 'review' | 'findings' | 'criteria' | 'brief'>

/**
 * What one cite becomes on the page: a place in this app for `NuxtLink`, a place outside it
 * opened in a new tab, or the session's words drawn as they are.
 */
export type CiteTarget
  = | { kind: 'page', text: string, to: string }
    | { kind: 'away', text: string, href: string }
    | { kind: 'text', text: string }

const FINDING = /^F[1-9]\d*$/
const CRITERION = /^criterion\s+(\d+)$/i
const OPTION = /^option\s+([A-Za-z])$/i
const COMMIT = /^[0-9a-f]{7,40}$/
const PATH_LINE = /^(\S+):(\d+)$/

/** Where one cite goes. A cite the record cannot place is `text`. */
export function citeTarget(cite: string, source: CiteSource): CiteTarget {
  const text = cite.trim()
  const id = source.review.id

  if (FINDING.test(text)) {
    return source.findings.some(finding => finding.key === text)
      ? { kind: 'page', text, to: findingPath(id, text) }
      : { kind: 'text', text }
  }

  const criterion = CRITERION.exec(text)
  if (criterion) {
    const position = Number(criterion[1])
    return (source.criteria ?? []).some(one => one.position === position)
      ? { kind: 'page', text, to: `${tabPath(id, 'understanding')}#ticket-against-what-was-built` }
      : { kind: 'text', text }
  }

  const option = OPTION.exec(text)
  if (option) {
    const key = option[1]!.toUpperCase()
    return source.brief && optionsOf(source.brief).some(one => one.key.toUpperCase() === key)
      ? { kind: 'page', text, to: `${tabPath(id, 'understanding')}#what-was-chosen` }
      : { kind: 'text', text }
  }

  if (COMMIT.test(text)) {
    const pull = prUrl(source.review.repo, source.review.pr)
    return pull
      ? { kind: 'away', text, href: `https://github.com/${source.review.repo}/commit/${text}` }
      : { kind: 'text', text }
  }

  const place = PATH_LINE.exec(text)
  if (place) {
    const [, file, line] = place
    const there = source.findings.find(finding =>
      finding.file === file && finding.line === Number(line))
    return there
      ? { kind: 'page', text, to: findingPath(id, there.key) }
      : { kind: 'text', text }
  }

  return { kind: 'text', text }
}
