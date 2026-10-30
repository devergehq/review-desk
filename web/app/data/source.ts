/*
 * Which records the dashboard reads, and **the only place a fixture can be reached from.**
 *
 * The dashboard reads the API that `review-desk serve` serves. Sample mode is the
 * development and component-test alternative: invented records in memory, with a "Sample
 * data" marker on every screen so that nothing on it can be mistaken for a real review.
 *
 * ## A production build cannot show fixtures, and not because anyone remembered to check
 *
 * `import.meta.env.DEV` is a compile-time constant: Vite replaces it with `false` when the
 * dashboard is built for production, so [`sampleWanted`] returns `false` before any runtime
 * value is consulted, and the `await import('./sample')` below becomes unreachable. An
 * unreachable dynamic import is dropped along with everything it pulls in — the four JSON
 * files, the in-memory write logic, all of it — so a built `web/.output/public` holds no
 * invented record to show. `test/build-output.spec.ts` measures that against a real build
 * rather than trusting the reasoning.
 *
 * **`import.meta.env.DEV` rather than Nuxt's `import.meta.dev`**, and the difference is not
 * cosmetic: `@nuxt/test-utils` sets `import.meta.dev` to `false` in the plain test
 * environment, which would make sample mode unreachable in the component tests that exist
 * to use it. The predicate wanted here is "this is not a production build", and that is
 * what Vite's own flag means.
 *
 * The switch is deliberately *not* a runtime value. A `runtimeConfig` entry, or anything
 * read from the environment when the page loads, would keep the branch alive and the
 * fixtures in the bundle — and "a production build never shows sample data" would then be
 * a property of a configuration file that ships next to the thing it governs.
 *
 * ## How it is selected in development
 *
 * Three ways, all of them inert in a built dashboard:
 *
 * | How | For |
 * |---|---|
 * | `?sample` on any URL, and `?api` to come back | Looking at a screen without starting the binary |
 * | `localStorage['review-desk:source']` | Staying in sample mode across reloads; the "Use sample records" control on the "not answering" screen sets it |
 * | `chooseSource('sample')` | The component tests, which have no URL and no server |
 */

import type { DeskBackend } from './backend'

/** Where the choice is remembered across reloads. */
const SOURCE_KEY = 'review-desk:source'

/** Which records to read. */
export type DeskSourceName = 'api' | 'sample'

/**
 * An explicit choice, when something has made one.
 *
 * `null` means "work it out from the URL and what was remembered", which is what a browser
 * does. The component tests set it, because they have neither.
 */
let chosen: DeskSourceName | null = null

/**
 * Choose where the records come from, or go back to working it out.
 *
 * Remembered across reloads when a browser is there to remember it, so that a developer who
 * turned sample mode on from the "not answering" screen is still in it after a reload.
 */
export function chooseSource(source: DeskSourceName | null): void {
  chosen = source
  try {
    if (source == null) localStorage.removeItem(SOURCE_KEY)
    else localStorage.setItem(SOURCE_KEY, source)
  }
  catch {
    /*
     * A private window, blocked site data, or no browser at all. The choice still holds
     * for this page; it is only not remembered, which is worth nothing to report.
     */
  }
}

/** One stored or query value, or `null` when there is no browser to ask. */
function fromTheBrowser(): DeskSourceName | null {
  try {
    const query = new URLSearchParams(window.location.search)
    /*
     * `?api` as well as `?sample`, because the choice is remembered: without a way to say
     * "the real records" in a URL, getting back out of sample mode would mean clearing the
     * site's storage, which is a thing nobody should have to be told.
     */
    if (query.has('api')) return 'api'
    if (query.has('sample')) return 'sample'
  }
  catch {
    /* No window. */
  }
  try {
    const held = localStorage.getItem(SOURCE_KEY)
    if (held === 'sample' || held === 'api') return held
  }
  catch {
    /* No storage, or it threw. */
  }
  return null
}

/**
 * Whether this page is reading invented records.
 *
 * Answered without awaiting anything, because the "Sample data" marker is in the header
 * and a marker that appeared a frame late would be a marker nobody could rely on.
 */
export function sampleWanted(): boolean {
  // Replaced with `false` in a production build, which is what removes the branch below.
  if (!import.meta.env.DEV) return false
  return (chosen ?? fromTheBrowser()) === 'sample'
}

/**
 * The records themselves.
 *
 * `sample.ts` is reached by a dynamic `import()` inside a branch that a production build
 * has already proved dead — see the top of this file.
 */
export async function loadBackend(): Promise<DeskBackend> {
  /*
   * **`import.meta.env.DEV` is tested here, not only inside `sampleWanted`**, and that is
   * the difference between the fixtures leaving the bundle and not. Measured, after they
   * did not: a call to a function that happens to return `false` is not a dead branch to
   * Rollup, so `if (sampleWanted())` kept the dynamic import alive and `nuxt generate`
   * emitted a 44 kB chunk of invented reviews beside the app. Written as a literal the
   * bundler replaces, this is `if (false && …)`, the block goes, and the chunk goes with
   * it. `test/build-output.spec.ts` is the measurement, not this comment.
   */
  if (import.meta.env.DEV && sampleWanted()) {
    const { sampleBackend } = await import('./sample')
    return sampleBackend()
  }

  /*
   * **The HTTP client is imported here and not at the top of this file**, so that it is not
   * in the app shell's static graph. Loaded here, it is fetched when the first read is made,
   * which is after the shell has rendered its loading state — and a build that runs in
   * sample mode never fetches it at all.
   *
   * This was done while chasing an end-to-end failure that turned out to be the HTTP
   * transport losing requests, not anything about chunk ordering. It is kept for the reason
   * above and not as a fix.
   *
   * Named one by one rather than spread from the module. Twenty-two lines buy a compile error
   * when an operation is added to `api.ts` and not to `DeskBackend`, or named differently in
   * the two — which a spread would have accepted silently, leaving sample mode and the API
   * with different surfaces.
   */
  const api = await import('./api')

  return {
    isSampleData: false,
    getInbox: api.getInbox,
    listReviews: api.listReviews,
    getReview: api.getReview,
    getDraft: api.getDraft,
    getReviewSession: api.getReviewSession,
    listRules: api.listRules,
    listRepos: api.listRepos,
    startReview: api.startReview,
    dismissReview: api.dismissReview,
    listDuplicateRounds: api.listDuplicateRounds,
    foldRound: api.foldRound,
    restoreReview: api.restoreReview,
    deleteReview: api.deleteReview,
    confirmBrief: api.confirmBrief,
    setFindingDisposition: api.setFindingDisposition,
    decideFinding: api.decideFinding,
    saveDraft: api.saveDraft,
    approveDraft: api.approveDraft,
    sendBackDraft: api.sendBackDraft,
    ticketRestAndApprove: api.ticketRestAndApprove,
    requestNitTicket: api.requestNitTicket,
    promoteRule: api.promoteRule,
    dismissRule: api.dismissRule,
    retireRule: api.retireRule,
    deferRule: api.deferRule,
    health: api.health,
  }
}
