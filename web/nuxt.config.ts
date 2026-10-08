/*
 * Review Desk's dashboard.
 *
 * Server-side rendering is off and the build is static: `cargo build` embeds the output in
 * the Rust binary, and the JSON API is that binary's, never Nuxt's. There are no server
 * routes here and there is no `server/` directory.
 *
 * Fonts are bundled rather than fetched, so the generated site asks nothing of the
 * network — which is what lets the whole app run from a file server with no internet.
 */

/**
 * The server `nuxt dev` forwards the API to.
 *
 * `review-desk serve` takes `--port`, so the address is an environment variable rather
 * than a constant. It is read at development time only; a built dashboard calls
 * `/api/v1` on whatever origin served it, which is the binary.
 */
const devApi = process.env.REVIEW_DESK_API ?? 'http://127.0.0.1:7777'

/**
 * Which records the dashboard reads in development: `api` or `sample`.
 *
 * Only honoured by `nuxt dev` and the component tests. A built dashboard always reads the
 * API — `app/data/source.ts` ignores this in a production build, and the fixtures are not
 * in the bundle to read.
 */
const devSource = process.env.REVIEW_DESK_SOURCE === 'sample' ? 'sample' : 'api'

/*
 * Every route a static file server has to answer.
 *
 * Two, and no parameterised ones. A review's route is resolved by the client router: the
 * Rust server answers any path that cannot be a file with the index, which
 * `crates/server/src/assets.rs` does deliberately and `crates/server/tests/assets.rs`
 * holds. Naming `/reviews/17` here would mean reading a fixture at build time to find out
 * which ids exist, which is the fixture data a production build must not contain.
 */
const prerenderRoutes = ['/', '/rules']

export default defineNuxtConfig({

  modules: ['@nuxt/eslint', '@nuxt/test-utils/module'],
  ssr: false,

  devtools: { enabled: false },

  app: {
    head: {
      htmlAttrs: { lang: 'en' },
      title: 'Review Desk',
      meta: [
        { charset: 'utf-8' },
        { name: 'viewport', content: 'width=device-width, initial-scale=1' },
        {
          name: 'description',
          content: 'The record of an agent-assisted code review, and what is waiting on you.',
        },
      ],
    },
  },

  css: [
    '@fontsource/ibm-plex-sans/400.css',
    '@fontsource/ibm-plex-sans/500.css',
    '@fontsource/ibm-plex-sans/600.css',
    '@fontsource/ibm-plex-mono/400.css',
    '@fontsource/ibm-plex-mono/500.css',
    '~/assets/css/tokens.css',
  ],

  /*
   * Read by `app/data/source.ts`. Baked into the bundle at build time, which is what makes
   * `sample` unreachable in a built dashboard: `nuxt generate` writes `api` here and the
   * branch that would have loaded a fixture is removed with it.
   */
  runtimeConfig: {
    public: {
      deskSource: devSource,
    },
  },

  compatibilityDate: '2026-10-08',

  nitro: {
    prerender: {
      routes: prerenderRoutes,
      crawlLinks: false,
      failOnError: true,
    },

    /*
     * The development loop: this dev server on :3000 forwards `/api/v1` to a running
     * `review-desk serve`.
     *
     * **The two header lines are the whole reason this block is not three lines.** That
     * server refuses a request whose `Host` is not the address it bound, and whose
     * `Origin`, when sent, is not its own — `crates/server/src/guard.rs`, which closes DNS
     * rebinding and a page on another origin driving `fetch`. A browser on
     * `localhost:3000` sends `Host: localhost:3000` and `Origin: http://localhost:3000`,
     * and the guard is right to refuse both.
     *
     * So the proxy states what it is really talking to. `changeOrigin` rewrites `Host` to
     * the target's, and `headers.origin` replaces the browser's `Origin` with the server's
     * own. The guard is unchanged, knows nothing about a proxy, and would refuse this dev
     * server exactly as it refuses any other origin — what reaches it is a request that
     * genuinely names it.
     *
     * The alternative was a development bypass in the guard. It is not here on purpose: a
     * flag, an allowed-origins list or an environment switch that relaxes the check is one
     * mistake away from leaving it relaxed in a build that signs off reviews.
     */
    devProxy: {
      '/api/v1': {
        target: `${devApi}/api/v1`,
        changeOrigin: true,
        headers: { origin: devApi },
      },
    },
  },

  typescript: { strict: true },

  eslint: {
    config: { stylistic: true },
  },
})
