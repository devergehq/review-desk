import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

/*
 * The rule DEV-855 asks for: "a search of `web/` shows data access only inside the data
 * layer". A reviewer should not have to run that search by hand, and discipline is not a
 * mechanism, so it is a test.
 *
 * **It is what made DEV-862 a change to six files rather than to every screen.** The data
 * layer is the composable every screen calls, the two sources behind it — the API and the
 * fixtures — the type that keeps them interchangeable, the error shape they share, and the
 * file that chooses. Nothing outside them makes a request, names a fixture, or knows which
 * of the two it is reading.
 */

/* Vitest runs from `web/`, which `package.json` next door confirms. */
const ROOT = resolve('app')

const DATA_LAYER = [
  'composables/useReviewDesk.ts',
  'data/api.ts',
  'data/backend.ts',
  'data/fault.ts',
  'data/sample.ts',
  'data/source.ts',
]

/** The only file that may name a fixture. */
const FIXTURE_READER = 'data/sample.ts'

/** The only file that may make an HTTP request. */
const HTTP_CALLER = 'data/api.ts'

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry)
    return statSync(full).isDirectory() ? walk(full) : [full]
  })
}

const sources = walk(ROOT)
  .filter(path => /\.(ts|vue)$/.test(path))
  .map(path => ({ path: relative(ROOT, path), text: readFileSync(path, 'utf8') }))

describe('the data layer is the only way to the records', () => {
  it('finds the files it is about to check', () => {
    expect(sources.length).toBeGreaterThan(20)
  })

  it.each(DATA_LAYER)('%s exists', (path) => {
    expect(sources.map(file => file.path)).toContain(path)
  })

  it('only one file names a fixture', () => {
    const offenders = sources
      .filter(file => file.path !== FIXTURE_READER)
      .filter(file => /from\s+['"][^'"]*data\/(reviews|drafts|rules|desk)/.test(file.text))
      .map(file => file.path)
    /*
     * One, not five. The fixtures are reachable from exactly the module a production build
     * drops, which is what makes "a production build shows no sample data" checkable by
     * reading one import list — see `test/build-output.spec.ts` for the measurement.
     */
    expect(offenders).toEqual([])
  })

  it('only one file makes an HTTP request', () => {
    const offenders = sources
      .filter(file => file.path !== HTTP_CALLER)
      .filter(file =>
        /\$fetch\s*\(|\buseFetch\s*\(|\buseAsyncData\s*\(|\bfetch\s*\(|\baxios\b|XMLHttpRequest/.test(
          file.text,
        ),
      )
      .map(file => file.path)
    expect(offenders).toEqual([])
  })

  it('no screen reaches past the composable into either source', () => {
    const offenders = sources
      .filter(file => !DATA_LAYER.includes(file.path))
      .filter(file =>
        /\bloadSource\b|\bresetSource\b|\bloadBackend\b|\bsampleBackend\b|\bcurrentSource\b/
          .test(file.text),
      )
      .map(file => file.path)
    expect(offenders).toEqual([])
  })

  it('no screen decides where the API is', () => {
    /*
     * The dashboard calls `/api/v1` on whatever origin served it, and that one fact lives
     * in `data/api.ts`. A screen that built a URL would be a screen that could be pointed
     * somewhere else — and a cross-origin request is one the server's guard refuses, so the
     * failure would look like a broken screen rather than a misplaced constant.
     */
    const offenders = sources
      .filter(file => file.path !== HTTP_CALLER)
      .filter(file => /['"`]\/api\/v1|127\.0\.0\.1|localhost:/.test(file.text))
      .map(file => file.path)
    expect(offenders).toEqual([])
  })

  it('no screen reads the development source switch', () => {
    /*
     * Which records are being read is the data layer's decision. A screen that branched on
     * it would behave differently on fixtures than on the record, which is the one thing
     * sample mode must not do — a component test would then be testing a path the binary
     * never takes.
     */
    const offenders = sources
      .filter(file => !DATA_LAYER.includes(file.path))
      .filter(file => /import\.meta\.(dev|env)|REVIEW_DESK_SOURCE/.test(file.text))
      .map(file => file.path)
    expect(offenders).toEqual([])
  })
})

describe('the app asks nothing of the network', () => {
  it('no stylesheet or script is loaded from another origin', () => {
    const offenders = sources
      .filter(file => /<link[^>]+href=["']https?:|<script[^>]+src=["']https?:/.test(file.text))
      .map(file => file.path)
    expect(offenders).toEqual([])
  })

  /*
   * The three typefaces the approved boards are drawn in. The boards themselves ask Google
   * Fonts for them in a `<link>`; this app cannot, and the whole point of naming all three
   * here is that adding a fourth family by `<link>` fails rather than works on the one
   * machine with internet and breaks on the one without.
   */
  it.each([
    ['@fontsource/newsreader', 'Newsreader, the display face'],
    ['@fontsource/instrument-sans', 'Instrument Sans, everything read'],
    ['@fontsource/jetbrains-mono', 'JetBrains Mono, a number or a key'],
  ])('%s is bundled rather than fetched', (packageName) => {
    const config = readFileSync(resolve('nuxt.config.ts'), 'utf8')
    expect(config).toContain(packageName)
    expect(config).not.toContain('fonts.googleapis.com')
  })

  it('every family the stylesheet names is one of the three that are bundled', () => {
    const css = readFileSync(resolve('app/assets/css/tokens.css'), 'utf8')
    const families = [...css.matchAll(/--rd-font-[a-z]+:\s*'([^']+)'/g)].map(hit => hit[1])
    expect(families).toEqual(['Newsreader', 'Instrument Sans', 'JetBrains Mono'])
  })
})
