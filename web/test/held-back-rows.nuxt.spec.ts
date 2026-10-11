// @vitest-environment nuxt
import { describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'

import HeldBackRows from '../app/components/HeldBackRows.vue'
import { heldBackRows } from '../app/utils/draft'

import type { Finding, HeldBackFinding } from '../app/types/desk'

/*
 * **The panel at the volume the complaint was about.**
 *
 * DEV-928 was raised on eight held-back findings in a column about 290 pixels wide, with
 * claims of 41 to 63 words. One or two rows prove nothing about that: the things that went
 * wrong — a code name running out of the card, 63 words printed where a title belongs, eight
 * paragraphs where a reader wanted a list — are all properties of the panel at that size.
 *
 * So this mounts the panel on its own with eight rows, which is the level the question is
 * really at: `test/draft.spec.ts` holds the arithmetic, `test/screens.nuxt.spec.ts` holds the
 * panel in its page at the fixtures' own three rows, and the real browser measurement at
 * 1440 and 390 is `e2e/review-desk.spec.ts` against the seeded binary. A fixture review in
 * `app/data/` would have been the fourth place, and it was tried: adding one changed the
 * queue's counts and the inbox's totals, which are six assertions in two files about a
 * screen this issue does not touch.
 *
 * **The records are built from the real derivation**, not from hand-written rows: a test that
 * invented `HeldBackRow`s directly would pass with `heldBackRows` broken.
 */

/**
 * A claim of exactly `n` words, opening with something readable about the finding.
 *
 * Padded to the length rather than guessed at it: the point of these records is that they
 * are the lengths the complaint was about, and "about sixty words" measured nothing.
 */
function claimOf(n: number, about: string): string {
  const opening = `The ${about}`.split(/\s+/)
  const pad = Array.from({ length: n - opening.length }, (_, at) => `word${at + 1}`)
  const claim = [...opening, ...pad].join(' ')
  if (claim.split(/\s+/).length !== n) throw new Error(`claimOf(${n}) built the wrong length`)
  return claim
}

function held(seq: number, severity: HeldBackFinding['severity'], claim: string): HeldBackFinding {
  return { finding_seq: seq, finding_key: `F${seq}`, claim, severity } as HeldBackFinding
}

function raised(seq: number, title: string | null, file: string, line: number): Finding {
  return {
    review_id: 42,
    seq,
    key: `F${seq}`,
    lens: 'repetition',
    severity: 'nit',
    file,
    line,
    claim: 'the claim is on the entry, not here',
    title,
    disposition: 'held_back',
    created_at: '2026-10-11T07:40:00Z',
  } as Finding
}

/**
 * Eight held back: six nits and two questions, every claim between 41 and 63 words.
 *
 * One of them carries **no title at all**, so the twelve-word stand-in is on the page, and
 * one carries a **long code name in backticks**, which is the pair the issue's eighth
 * criterion asks for.
 */
const LONG_NAME = '`report_definitions.allowed_columns` is read twice, in two orders'

const ENTRIES: HeldBackFinding[] = [
  held(4, 'nit', claimOf(57, 'list in `report_definitions.allowed_columns` is read twice')),
  held(5, 'nit', claimOf(44, 'class is named for the smaller half of what it does')),
  held(6, 'nit', claimOf(61, 'address pattern is written twice and the two have drifted')),
  held(7, 'nit', claimOf(50, 'comment describes the code as it used to be')),
  held(8, 'nit', claimOf(41, 'builder method this change replaced has no caller left')),
  held(9, 'nit', claimOf(63, 'default is a string where every reader wants a boolean')),
  held(10, 'question', claimOf(54, 'existing schedule with no policy is not established')),
  held(11, 'question', claimOf(59, 'per-row policy read may be paid once or per row')),
]

const FINDINGS: Finding[] = [
  raised(4, LONG_NAME, 'app/Reporting/ColumnPolicy.php', 44),
  raised(5, 'The class is named for the smaller half of what it does', 'app/Reporting/Redaction.php', 12),
  raised(6, 'The address pattern is written twice', 'app/Reporting/ExportPipeline.php', 96),
  /* No title: the row shows the first twelve words of the claim and says so. */
  raised(7, null, 'app/Reporting/ReportBuilder.php', 140),
  raised(8, 'The builder method this change replaced has no caller left', 'app/Reporting/ExportContext.php', 71),
  raised(9, 'The redaction default is a string', 'config/exports.php', 18),
  raised(10, 'What an existing schedule with no policy does is not established', 'app/Jobs/RunScheduledExport.php', 118),
  raised(11, 'Whether the per-row policy read is paid once is unsaid', 'app/Reporting/ColumnPolicy.php', 88),
]

const ROWS = heldBackRows(ENTRIES, FINDINGS)

function panel() {
  return mountSuspended(HeldBackRows, { props: { rows: ROWS } })
}

describe('eight findings held back, which is what the complaint was about', () => {
  it('is the record the issue describes: six nits, two questions, 41 to 63 words', () => {
    expect(ROWS).toHaveLength(8)
    expect(ROWS.filter(row => row.severity === 'nit')).toHaveLength(6)
    expect(ROWS.filter(row => row.severity === 'question')).toHaveLength(8 - 6)
    for (const entry of ENTRIES) {
      const words = entry.claim.split(/\s+/).length
      expect(words, `${entry.finding_key}'s claim`).toBeGreaterThanOrEqual(41)
      expect(words, `${entry.finding_key}'s claim`).toBeLessThanOrEqual(63)
    }
    expect(ENTRIES.some(entry => entry.claim.split(/\s+/).length > 60)).toBe(true)
  })

  it('draws eight rows and no claim at all until one is opened', async () => {
    const page = await panel()
    expect(page.findAll('.held-row')).toHaveLength(8)
    expect(page.findAll('.held-opened')).toHaveLength(0)
    /* The 57-word claim is nowhere on the closed panel. */
    expect(page.text()).not.toContain(ENTRIES[0]!.claim)
  })

  it('heads the panel with the count, and each group with its own', async () => {
    const page = await panel()
    expect(page.text()).toContain('Held back · 8 findings')
    expect(page.findAll('.held-group').map(group => group.text()))
      .toEqual(['2 questions', '6 nits'])
  })

  it('draws every row’s key, severity and line, and no severity tag', async () => {
    const page = await panel()
    const rows = page.findAll('.held-row')
    for (const row of rows) {
      expect(row.find('.cell--key').text()).toMatch(/^F\d+$/)
      expect(row.find('.cell--severity').text()).toMatch(/nit|question/)
      expect(row.find('.cell--line').text().length).toBeGreaterThan(0)
    }
    /* Nothing grades a finding beyond the severity word the record holds. */
    expect(page.findAll('.tag')).toHaveLength(0)
    expect(page.findAll('.pill')).toHaveLength(0)
  })

  /*
   * The pair the issue's eighth criterion asks for, on the page rather than in a derivation:
   * a long code name drawn as code, and a title the page had to cut.
   */
  it('draws a backtick pair in a title as code, not as backticks', async () => {
    const page = await panel()
    const row = page.findAll('.held-row').find(one => one.text().includes('read twice'))!
    expect(row.find('code').text()).toBe('report_definitions.allowed_columns')
    expect(row.text()).not.toContain('`')
  })

  it('says a line it cut is shortened, and shows the whole claim when opened', async () => {
    const page = await panel()
    const row = page.findAll('.held-row').find(one => one.text().includes('shortened'))!
    expect(row.find('.cell--line').text()).toContain('…')
    /* Twelve words and the ellipsis, which is what the row carries for F7. */
    expect(row.find('.cell--line').text().replace('shortened', '').trim().split(/\s+/))
      .toHaveLength(12)

    await row.trigger('click')
    const opened = page.find('.held-opened')
    expect(opened.text()).toContain('No title was written for this finding')
    expect(opened.text()).toContain('first 12 words')
    expect(opened.text()).toContain(ENTRIES[3]!.claim)
  })

  it('opens one row at a time, and says so to a screen reader', async () => {
    const page = await panel()
    const rows = () => page.findAll('.held-row')

    await rows()[0]!.trigger('click')
    expect(rows()[0]!.attributes('aria-expanded')).toBe('true')
    expect(rows()[0]!.attributes('aria-controls')).toBe(page.find('.held-opened').attributes('id'))
    /* Named for a screen reader, so "Open" is not the whole announcement. */
    expect(rows()[0]!.text()).toContain('Fold F10')

    await rows()[4]!.trigger('click')
    expect(page.findAll('.held-opened')).toHaveLength(1)
    expect(rows().filter(row => row.attributes('aria-expanded') === 'true')).toHaveLength(1)
  })

  it('shows the claim and where it is in the code, and nothing else', async () => {
    const page = await panel()
    await page.findAll('.held-row')[0]!.trigger('click')
    const opened = page.find('.held-opened')
    expect(opened.text()).toContain('In the code')
    expect(opened.text()).toContain('app/Jobs/RunScheduledExport.php:118')
    /* The code review page owns the hunk, the evidence and the comment as it will post. */
    expect(opened.findAll('pre')).toHaveLength(0)
  })

  it('says so rather than inventing a file for a finding that names none', async () => {
    const rows = heldBackRows(
      [held(12, 'nit', claimOf(45, 'description does not say what a skipped export does'))],
      [],
    )
    const page = await mountSuspended(HeldBackRows, { props: { rows } })
    await page.find('.held-row').trigger('click')
    expect(page.find('.held-opened').text()).toContain('The record names no file')
  })
})
