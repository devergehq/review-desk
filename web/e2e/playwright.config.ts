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
   * **Thirty seconds, and the ten it replaced is why.**
   *
   * Two CI runs failed with `#__nuxt` empty — the Vue app had not mounted — while every
   * blocking asset had answered 200. What had not answered were the two the generated
   * index loads as `<link rel="prefetch">`, one of which is the data layer the app shell
   * imports. Chrome runs a prefetch at the lowest priority and in no hurry, and the mount
   * waits on the same URL; on a runner four times slower than this machine, that race can
   * outlast ten seconds.
   *
   * It is calibration rather than suppression: the assertion is still that the element
   * appears, the test budget is still 60s, and `retries` is still 0 — a run that passes
   * only on the second attempt is not a pass. The coupling itself is the better fix and is
   * written up in the pull request; it is a change to how the bundle is split, which is a
   * different kind of change from this one.
   */
  expect: { timeout: 30_000 },

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
