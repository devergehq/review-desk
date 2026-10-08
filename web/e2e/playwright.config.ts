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
      name: 'desk',
      testMatch: /review-desk\.spec\.ts/,
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
