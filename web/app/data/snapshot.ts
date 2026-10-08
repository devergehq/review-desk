/*
 * Where the dashboard's records come from, and the only file in `web/` that names a
 * fixture.
 *
 * Today it returns invented records from `app/data/*.json`, shaped to
 * `contract/openapi.yaml`. When the Rust server exists this file becomes four `$fetch`
 * calls to `/inbox`, `/reviews/{id}`, `/reviews/{id}/draft` and `/rules?repo=`, and
 * nothing else in `web/` changes: the composable above it keeps its shape, and no screen
 * ever knew where the records came from.
 */

import type { DeskSource, DraftDetail, ReviewDetail, Rule } from '~/types/desk'
import reviews from './reviews.json'
import drafts from './drafts.json'
import rules from './rules.json'
import desk from './desk.json'

/** True while the records are invented. The header's marker reads this. */
export const isSampleData = true

/** The whole desk, cloned so a write can never reach the fixture files. */
export function loadSource(): DeskSource {
  return structuredClone({
    reviews: reviews as unknown as ReviewDetail[],
    drafts: drafts as unknown as DraftDetail[],
    rules: rules as unknown as Rule[],
    summary: desk.summary,
    repos: desk.repos,
    own_prs: desk.own_prs,
    handled_without_you: desk.handled_without_you,
    session_liveness: desk.session_liveness as Record<string, boolean | null>,
    github_available: desk.github_available,
  })
}
