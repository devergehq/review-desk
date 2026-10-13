import { defineConfig, devices } from '@playwright/test'

/*
 * The end-to-end test: a real browser, against the real binary, over a database the
 * command line seeded.
 *
 * **It needs a binary with the dashboard in it.** `cargo build` embeds whatever
 * `web/.output/public` held when it ran, so the sequence is `pnpm run generate` and then
 * `cargo build`, in that order — which is what `pnpm run e2e` does and what
 * `.github/workflows/e2e.yml` runs. A binary built without the front end serves a page
 * saying so, and the first assertion fails on it with that page's own words, which is a
 * clearer failure than a missing element.
 *
 * One browser. The dashboard is served from loopback to one developer who chose their own
 * browser; running the suite three times over would cost three times as much to learn
 * nothing about this app. Chromium is the one Playwright installs smallest.
 */
export default defineConfig({
  testDir: '.',
  /* The two servers and the seed, once for the whole run. */
  globalSetup: './global-setup.ts',

  /*
   * Serial. Both projects drive the same two servers over the same two databases, and the
   * acceptance path is a sequence — confirm a brief, then approve the draft behind it — so
   * a parallel run would race itself.
   */
  workers: 1,
  fullyParallel: false,

  /* A failing end-to-end test that passes on the second run is not a pass. */
  retries: 0,
  forbidOnly: !!process.env.CI,
  timeout: 60_000,

  /*
   * **Ten seconds, which is as long as anything here should ever take.** Every read is a
   * handful of SQLite reads over loopback and the assets come out of the binary's own
   * memory; the browser's work is rendering four screens.
   *
   * It was raised to thirty once, for CI failures where `#__nuxt` was empty, and put back.
   * Those failures were real and were not slowness: the server could accept a connection and
   * never read it, so two of the page's assets were never answered at all and `load` never
   * fired. The trace showed them pending sixty seconds later. That is fixed in the server —
   * it serves on `axum` now, and `crates/server/tests/concurrency.rs` is the fault written
   * down — so there is nothing here for a longer wait to paper over. Raising a timeout to
   * cover a hanging request only buys a slower red.
   */
  expect: { timeout: 10_000 },

  reporter: process.env.CI ? [['github'], ['list']] : [['list']],

  use: {
    ...devices['Desktop Chrome'],
    /* Wide enough for the two-column screens, which is what the mockups were drawn at. */
    viewport: { width: 1440, height: 900 },
    /* Kept only for a failure: a passing run should leave nothing behind. */
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'off',
  },

  projects: [
    {
      /*
       * The acceptance path, and DEV-936's measurement of the header beside it. Both are
       * assertions about the built binary in a browser, and `pnpm run e2e` runs this
       * project — so a header that overflows at 390 fails the same run the acceptance path
       * does rather than waiting for somebody to remember a second command.
       *
       * `header-links.spec.ts`, `what-happened.spec.ts` and `rounds.spec.ts` each start a
       * server of their own in `beforeAll` rather than using the two this config's global
       * setup starts: the seeded database is asserted against by name and by count, and a
       * review added to it for one measurement would rewrite assertions with nothing to do
       * with that measurement. Each carries the argument in its own header.
       */
      name: 'desk',
      testMatch: /(review-desk|header-links|what-happened|rounds)\.spec\.ts/,
    },
    {
      /*
       * The screenshots the pull request shows. Its own project, so that a CI run of the
       * acceptance test does not rewrite nine committed files on every build —
       * `pnpm run e2e:screenshots` is what produces them.
       */
      name: 'screenshots',
      testMatch: /screenshots\.spec\.ts/,
    },
  ],
})
