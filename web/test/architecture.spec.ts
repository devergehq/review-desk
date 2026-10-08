import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

/*
 * The rule DEV-855 asks for: "a search of `web/` shows data access only inside the data
 * layer". A reviewer should not have to run that search by hand, and discipline is not a
 * mechanism, so it is a test.
 *
 * The data layer is two files: the composable every screen calls, and the source it reads
 * from. Pointing the dashboard at the Rust server is a change to the second one.
 */

/* Vitest runs from `web/`, which `package.json` next door confirms. */
const ROOT = resolve('app')

const DATA_LAYER = ['composables/useReviewDesk.ts', 'data/snapshot.ts']

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

  it('no file outside the data layer imports a fixture', () => {
    const offenders = sources
      .filter(file => !DATA_LAYER.includes(file.path))
      .filter(file => /from\s+['"][^'"]*data\/(reviews|drafts|rules|desk)/.test(file.text))
      .map(file => file.path)
    expect(offenders).toEqual([])
  })

  it('no file outside the data layer makes an HTTP request', () => {
    const offenders = sources
      .filter(file => !DATA_LAYER.includes(file.path))
      .filter(file =>
        /\$fetch\s*\(|\buseFetch\s*\(|\buseAsyncData\s*\(|\bfetch\s*\(|\baxios\b|XMLHttpRequest/.test(
          file.text,
        ),
      )
      .map(file => file.path)
    expect(offenders).toEqual([])
  })

  it('no screen reaches past the composable into the reactive source', () => {
    const offenders = sources
      .filter(file => !DATA_LAYER.includes(file.path))
      .filter(file => /\bloadSource\b|\bisSampleData\b.*from/.test(file.text))
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

  it('the fonts are bundled, not fetched', () => {
    const config = readFileSync(resolve('nuxt.config.ts'), 'utf8')
    expect(config).toContain('@fontsource/ibm-plex-sans')
    expect(config).not.toContain('fonts.googleapis.com')
  })
})
