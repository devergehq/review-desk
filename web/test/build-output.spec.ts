import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

import { loadSource } from '../app/data/sample'

/*
 * **A production build contains no fixture data.**
 *
 * One of DEV-862's acceptance criteria, and it is here rather than in a comment because
 * the reasoning was wrong the first time it was written down. `app/data/source.ts` loads
 * the fixtures through a dynamic `import()` inside a branch that a production build makes
 * dead, and the first version of that branch tested a *function* — `if (sampleWanted())`.
 * A call that happens to return `false` is not a dead branch to Rollup, so `nuxt generate`
 * emitted a 44 kB chunk of invented reviews beside the app and served it. Writing the
 * condition as the literal the bundler replaces fixed it. Nothing about that is obvious
 * from reading the source, which is exactly why it is measured.
 *
 * It reads the build rather than making one: `pnpm run generate` is three and a half
 * seconds and belongs to the build step, not to every test run. `.github/workflows/web.yml`
 * builds before it tests, so this always has something to measure in CI.
 */

const BUILD = resolve('.output/public')

/** Every file in the build, as text where it is text. */
function files(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry)
    return statSync(full).isDirectory() ? files(full) : [full]
  })
}

const built = existsSync(BUILD)
const text = built
  ? files(BUILD)
      .filter(path => /\.(js|mjs|css|html|json|map)$/.test(path))
      .map(path => ({ path, body: readFileSync(path, 'utf8') }))
  : []

describe('the built dashboard', () => {
  it('has been built', () => {
    expect(
      built,
      'There is no build to check. Run `pnpm run generate` first — '
      + '`.github/workflows/web.yml` does, before it runs the tests.',
    ).toBe(true)
  })

  it('holds no sentence from a fixture', () => {
    /*
     * Taken from the fixtures themselves rather than typed out, so that editing a fixture
     * cannot quietly make this test look for a string that is no longer in one.
     */
    const source = loadSource()
    const sentences = [
      ...source.reviews.flatMap(detail => [
        detail.review.title,
        detail.brief?.problem,
        ...detail.findings.map(finding => finding.claim),
      ]),
      ...source.rules.map(rule => rule.statement),
      ...source.drafts.map(detail => detail.draft.body.split('\n')[0]),
    ].filter((line): line is string => typeof line === 'string' && line.length > 20)

    expect(sentences.length).toBeGreaterThan(10)

    const leaked = sentences.filter(sentence =>
      text.some(file => file.body.includes(sentence)),
    )
    expect(
      leaked,
      'A fixture\'s own words are in the production build. The sample records are meant to '
      + 'be removed with the branch that loads them — see app/data/source.ts.',
    ).toEqual([])
  })

  it('holds no repository or ticket a fixture invented', () => {
    /*
     * The shorter strings, checked separately: these are the ones a reader would notice on
     * screen first, and they are short enough to appear by accident in a minified bundle,
     * so a failure here deserves its own message rather than being lumped in above.
     */
    const invented = ['acme/portal', 'acme/billing-api', 'ABC-231', 'DEF-118']
    const leaked = invented.filter(name => text.some(file => file.body.includes(name)))
    expect(leaked).toEqual([])
  })

  it('names no fixture file', () => {
    const leaked = text
      .filter(file => /data\/(reviews|drafts|rules|desk)\.json/.test(file.body))
      .map(file => file.path)
    expect(leaked).toEqual([])
  })

  it('has no way to load the invented records, whatever it is asked', () => {
    /*
     * The capability, not a string. `loadBackend`'s sample branch is compiled out, so the
     * module that reads the fixtures is never imported and is not in the build — and these
     * three names are how you would know if it were.
     *
     * **The `review-desk:source` key is still in there, and that is not a hole.** The
     * function that writes it survives because the "Use sample records" control references
     * it, and the control cannot render. A bundle that can remember a preference it can
     * never act on is harmless; what would not be is a bundle that could act on it, which
     * is what this checks.
     */
    for (const name of ['sampleBackend', 'loadSource', 'resetSource']) {
      const leaked = text.filter(file => file.body.includes(name)).map(file => file.path)
      expect(leaked, `${name} is in the production build`).toEqual([])
    }
  })

  /*
   * **Whether the "Sample data" marker shows is not checked here, deliberately.** The
   * words are in the bundle — they are a string literal in a component's compiled render
   * function, and Vue's template compiler does not remove one behind a `v-if` that is
   * always false. Grepping for them would fail on a build where the marker can never
   * render, which is a test that fails for a reason nobody can act on.
   *
   * What the criterion is about is the rendered page, so it is asserted on a rendered
   * page: `web/e2e/review-desk.spec.ts` opens all five screens on a binary serving a real
   * database and fails if the marker is anywhere on them.
   */

  it('asks nothing of another origin', () => {
    /*
     * The app is embedded in a binary and run from loopback. A font or a script from a CDN
     * would stop it working on a machine with no internet, which is the machine it is
     * designed for.
     */
    const leaked = text
      .filter(file => file.path.endsWith('.html'))
      .filter(file => /(href|src)="https?:\/\//.test(file.body))
      .map(file => file.path)
    expect(leaked).toEqual([])
  })

  it('serves the two routes the binary prerenders, and relies on the fallback for the rest', () => {
    /*
     * Three, and no **parameterised** ones, which is the distinction this asserts. A
     * review's own route is resolved by the client router: the Rust server answers any
     * path that cannot be a file with the index, which `crates/server/tests/assets.rs`
     * holds. Naming `/reviews/17` here would mean reading a fixture at build time to find
     * out which ids exist — the fixture data this file is about.
     */
    expect(existsSync(join(BUILD, 'index.html'))).toBe(true)
    expect(existsSync(join(BUILD, 'rules/index.html'))).toBe(true)
    expect(existsSync(join(BUILD, 'reviews/index.html'))).toBe(true)

    /*
     * **Narrowed rather than loosened.** This read `path.includes('/reviews/')`, which was
     * right while no review route was prerendered at all and became wrong the moment
     * `/reviews` was — `reviews/index.html` contains that string and is the file that
     * should exist. Deleting the assertion would have given up on the thing it is for, so
     * it names the shape it is against: a path with a review *id* in it.
     */
    const withAnId = files(BUILD).filter(path => /\/reviews\/\d/.test(path))
    expect(withAnId).toEqual([])
  })
})
