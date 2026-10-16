/*
 * Every screen of a running desk, at two widths, into a directory you name.
 *
 * **This is the design loop's measuring instrument, and it is read-only by construction.**
 * A design change is judged by a before-and-after pair rather than by a diff, so the two
 * halves of that pair have to come off the same database through the same browser at the
 * same widths — otherwise the comparison carries the fixture's differences as well as the
 * change's. One directory per side:
 *
 *     pnpm run shoot -- --out ../.design-loop/before
 *     # … make the change, rebuild, serve again …
 *     pnpm run shoot -- --out ../.design-loop/after
 *
 * It is **not** `e2e/screenshots.spec.ts` and does not replace it. That file produces the
 * nine pictures committed in `docs/screenshots/`, from databases the command line seeds, so
 * that a pull request shows what a screen looks like; it asserts as it goes and is a
 * Playwright project. This walks a desk somebody else has already started, over whatever
 * database that desk was given, and asserts nothing. Two jobs, and a script that tried to
 * be both would have to seed a fixture it is meant to be comparing against.
 *
 * ## It refuses to shoot the desk the developer is using
 *
 * Review Desk's own desk runs on `127.0.0.1:7777` against `~/.review-desk/review-desk.db`,
 * and on 12 October 2026 a build session wrote a review into that database. So the port is
 * refused here rather than left to a flag somebody remembers: a tool whose whole purpose is
 * to visit every screen of a desk is the one most likely to be pointed at the wrong one.
 *
 * Two guards, because they fail differently:
 *
 * 1. **The address.** Port 7777 is refused outright, before a browser is launched.
 * 2. **The requests.** Every method but `GET` is aborted at the browser. The dashboard
 *    writes on sign-off, on a save and on a ticket request, and a stray click — or a page
 *    that fires something on mount — would otherwise reach the API. Nothing here clicks,
 *    which is exactly why the abort is worth having: it is the guard against the thing
 *    nobody predicted.
 *
 * The desk's own `guard.rs` would not help. It refuses a foreign origin, and these requests
 * come from the page it served.
 *
 * ## Which reviews, and why those three
 *
 * A screen is only as hard as the record behind it, so the ids below are the fixture's
 * stress cases rather than a sample. From
 * `~/.locus/data/review-desk/design-loop/fixture.db` (35 reviews, 464 findings):
 *
 * | Id | Pull request | What it stresses |
 * |---|---|---|
 * | 36 | #9639, round 1 of 1 | 16 findings, 12 draft sections and 6 threads on the code, under the index the house style writes. The design pass's own example, so its screens read beside that artifact — and one of the two ids in `baseline-shots-2026-10-15/`, so a shot here can be checked against the live desk's own baseline. |
 * | 1 | #9584, round 1 of 4 | 17 findings over a 19,750-character draft, the longest body with a real finding count — the shape the Draft page's height is about. **And it carries no index**, having been written before the house style grew one, so it is the control: a change that touches the index must leave this page alone. |
 * | 35 | #9541, round 5 of 5 | The only draft in the fixture that draws the whole sign-off rail. 19 findings, 5 held back, a pull request still open and a draft already signed off, so the rail carries the event, the sign-off, the take-back and the posting watch at once — and five rounds behind it for the switcher and the timeline. |
 *
 * **Every other draft in the fixture is `posted`, `drafting`, or on a pull request that has
 * finished.** Three are `ready` and all three are refused — two merged, one dismissed — so
 * they draw "Nothing to sign off" rather than the panel. That is why 35 is in the set and
 * why a change to the sign-off rail cannot be judged on 36 or 1: neither draws one.
 *
 * Review 29 (#9541 round 3) was in the first cut for its 24 findings and is folded into 35,
 * the same pull request four rounds later. Review 26 (#9622) is the baseline's other id and
 * stresses nothing these three do not. Pass `--ids` to shoot either, or anything else,
 * without editing this.
 *
 * ## What it does not control for
 *
 * **The clock.** The desk prints "31 min ago" and "1 hr 18 min waiting on you", and nothing
 * here pins the time — so two runs an hour apart differ in those words. Heights and layout
 * are unaffected, which is what a pair is read for; `e2e/screenshots.spec.ts` pins a clock
 * because it asserts on what it draws, and this asserts on nothing. Shoot the two sides
 * close together and read the timestamps as noise.
 *
 * ## What it writes
 *
 * One PNG per screen per width, `desk_` at 1440 and `phone_` at 390, named after the route
 * so that `before/` and `after/` sort into pairs. Beside them, `heights.tsv`: the name, the
 * width and the full document height of every shot. **The height is the measurement the
 * pictures cannot carry.** "The Draft page is 4,324px tall and the rail ends at 1,100" is
 * the finding that opened this loop, and a pair of tall thin images is a poor way to see
 * whether that got better. Two files of numbers diff.
 */

import { mkdirSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { chromium } from '@playwright/test'

/** The desk the developer uses. Never this script's target — see the header. */
const THE_LIVE_DESK = 7777

/** The two widths the app is drawn for: the mockups' desktop, and the narrowest phone. */
const WIDTHS = [
  { width: 1440, tag: 'desk' },
  { width: 390, tag: 'phone' },
]

/** The seven steps of a review, in the order the trail draws them. `''` is the review itself. */
const STEPS = [
  '',
  '/understand',
  '/alternatives',
  '/solution',
  '/code-review',
  '/draft',
  '/what-happened',
]

/** The screens that are not a review: the queue, the list and the rules. */
const SCREENS = ['/', '/reviews', '/rules']

/**
 * The fixture's three stress cases. See the table in the header for what each one is for.
 *
 * Here rather than in a flag's default string, so that a reader of this file learns which
 * reviews the loop's pictures are of without running it.
 */
const IDS = ['36', '1', '35']

/**
 * Whether this address may be shot, and why not when it may not.
 *
 * **A decision rather than an `process.exit`, so that a test can fail if it is removed.**
 * That is `dev-proxy.ts`'s shape and it is here for `dev-proxy.ts`'s reason: the guard
 * exists because something already went wrong once — a build session wrote into
 * `~/.review-desk/review-desk.db` on 12 October 2026 — and a guard nothing asserts on is a
 * comment. `test/shoot.spec.ts` fails if this stops refusing 7777.
 *
 * **On the port and not on the whole address.** The desk is reached as `127.0.0.1:7777`,
 * `localhost:7777` and anything else that resolves to the loopback interface, and a check
 * against one spelling of the host is a check somebody gets around by accident. Nothing
 * else on this machine wants 7777, so the number is the fact.
 *
 * A default port counts. `http://127.0.0.1` has no port in it and is port 80, not 7777;
 * `URL` reports that as an empty string, so the comparison is made on what the URL means
 * rather than on what it says.
 */
export function mayShoot(base) {
  let url
  try {
    url = new URL(base)
  }
  catch {
    return { shoot: false, reason: `${JSON.stringify(base)} is not a URL.` }
  }

  const port = url.port === '' ? { 'http:': 80, 'https:': 443 }[url.protocol] ?? null : Number(url.port)
  if (port === THE_LIVE_DESK) {
    return {
      shoot: false,
      reason:
        `that is port ${THE_LIVE_DESK}, the desk the developer is using against\n`
        + '  ~/.review-desk/review-desk.db. This script will not visit it.\n'
        + '  Start your own: REVIEW_DESK_DB=<your copy> review-desk serve --port 7790',
    }
  }

  return { shoot: true }
}

/**
 * What a route's picture is called: the width's tag, then the route with its slashes turned
 * into underscores.
 *
 * `desk_reviews_36_draft.png`. The queue is at `/` and has no path to name it, so it is
 * `desk_home.png`.
 *
 * **Not what the reference script produced, and the difference is a bug it had.** Its
 * `route.replace(/\//g, '_') || '_home'` never reached the fallback: `'/'` replaces to
 * `'_'`, which is truthy, so the queue's picture was called `desk_.png` — which is what
 * `baseline-shots-2026-10-15/` holds. That file is a one-off cross-check against the live
 * desk rather than the other half of a pair, so the clearer name is worth the one rename
 * when the two are read side by side. `test/shoot.spec.ts` is what caught it.
 */
export function nameFor(tag, route) {
  const path = route.replace(/^\/+$/, '').replace(/\//g, '_')
  return `${tag}${path === '' ? '_home' : path}.png`
}

/**
 * Every route to visit, in the order they are shot.
 *
 * The same list at both widths. The reference script this grew from shot three screens and
 * two steps on the phone, which made the phone half of a pair unable to answer a question
 * about any other screen — and the narrow width is where the sprawl this loop is about is
 * worst.
 */
export function routesFor(ids) {
  return [...SCREENS, ...ids.flatMap(id => STEPS.map(step => `/reviews/${id}${step}`))]
}

/** Stop, with something the reader can act on, and no browser left running. */
function fail(message) {
  console.error(`\nshoot: ${message}\n`)
  process.exit(1)
}

/**
 * The flags, read by hand rather than through a parser.
 *
 * Three of them, and the dependency a parser would add is one more thing in a tree that
 * `test/architecture.spec.ts` has opinions about. `--out` has no default on purpose: the
 * directory is the half of the comparison being written, and a default would eventually
 * write `after/` over `before/`.
 */
function options(argv) {
  const flags = new Map()
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i]
    if (!flag.startsWith('--')) fail(`"${flag}" is not a flag. Expected --out, --base or --ids.`)
    const value = argv[i + 1]
    if (value == null || value.startsWith('--')) fail(`${flag} needs a value.`)
    flags.set(flag.slice(2), value)
    i += 1
  }

  for (const flag of flags.keys()) {
    if (!['out', 'base', 'ids'].includes(flag)) {
      fail(`--${flag} is not a flag this takes. Expected --out, --base or --ids.`)
    }
  }

  const out = flags.get('out')
  if (out == null) {
    fail(
      'no --out.\n'
      + '  It is the directory the pictures go in, and it names one side of a comparison:\n'
      + '    pnpm run shoot -- --out ../.design-loop/before\n'
      + '  There is no default, so that one run cannot write over the other.',
    )
  }

  const base = flags.get('base') ?? 'http://127.0.0.1:7790'
  const ids = (flags.get('ids') ?? IDS.join(',')).split(',').map(one => one.trim()).filter(Boolean)
  if (ids.length === 0) fail('--ids was given and held no ids.')

  return { out: resolve(out), base, ids }
}

async function shoot() {
  const { out, base, ids } = options(process.argv.slice(2))

  const decision = mayShoot(base)
  if (!decision.shoot) fail(`--base ${decision.reason}`)

  mkdirSync(out, { recursive: true })

  const routes = routesFor(ids)
  const browser = await chromium.launch()
  const measured = [['name', 'width', 'height'].join('\t')]

  try {
    for (const { width, tag } of WIDTHS) {
      const page = await browser.newPage({
        viewport: { width, height: 900 },
        /*
         * One device pixel per CSS pixel. The pictures are read side by side in an artifact
         * and compared by height, not zoomed into, and a retina shot is four times the
         * bytes for a comparison that does not use them.
         */
        deviceScaleFactor: 1,
      })

      /* The second of the two guards. See the header. */
      await page.route('**/*', route =>
        route.request().method() === 'GET' ? route.continue() : route.abort())

      for (const route of routes) {
        const name = nameFor(tag, route)
        /*
         * `networkidle`, then a beat. The brief's flow diagram is drawn by mermaid after
         * the page is otherwise ready and the fonts are bundled rather than fetched, so the
         * network going quiet plus 400ms is what makes two runs produce the same image —
         * the same wait, for the same reason, as `e2e/screenshots.spec.ts`.
         */
        await page.goto(base + route, { waitUntil: 'networkidle' })
        await page.waitForTimeout(400)
        await page.screenshot({ path: resolve(out, name), fullPage: true })

        const height = await page.evaluate(() => document.documentElement.scrollHeight)
        measured.push([name, String(width), String(height)].join('\t'))
        console.log(`${name}\t${height}px`)
      }

      await page.close()
    }
  }
  finally {
    await browser.close()
  }

  writeFileSync(resolve(out, 'heights.tsv'), `${measured.join('\n')}\n`)
  console.log(`\n${routes.length * WIDTHS.length} shots and heights.tsv in ${out}`)
}

/*
 * **Run only when run, so that the three decisions above can be imported and asserted on.**
 * Without this, importing the module launches a browser and starts shooting — which is why
 * the guard that matters most here had nothing holding it.
 */
if (import.meta.main) await shoot()
