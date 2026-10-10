import { existsSync } from 'node:fs'
import { resolve } from 'node:path'

import { mayForward } from './dev-proxy'

/*
 * **The generated contract types have to be there, and saying so first is the point.**
 *
 * `app/types/contract.ts` is generated from `contract/openapi.yaml` and is not committed.
 * Every documented command generates it first — see `package.json` — so this should never
 * fire. It exists for the person who runs `nuxt typecheck` or `nuxt build` directly.
 *
 * Without it, the type checker reports the real cause *last*: measured at fifty-two errors,
 * of which fifty-one were `implicitly has an 'any' type` in `app/data/sample.ts` and the
 * missing module was number fifty-two. Nobody reads to the bottom of that. This config is
 * evaluated by every `nuxt` command, so one clear sentence arrives before any of them.
 */
const contractTypes = resolve(__dirname, 'app/types/contract.ts')
if (!existsSync(contractTypes)) {
  throw new Error(
    'app/types/contract.ts has not been generated.\n'
    + '  It comes from ../contract/openapi.yaml and is deliberately not committed.\n'
    + '  Run `pnpm install`, or `pnpm run contract:types` to generate it on its own.\n'
    + '  Every pnpm script here generates it first; a bare `nuxt` command does not.',
  )
}

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

  /*
   * The three typefaces the approved boards are drawn in, and every weight they use:
   * Newsreader for display, Instrument Sans for everything read, JetBrains Mono for a
   * pull request number, a rule key and a keyboard hint.
   *
   * **The latin subset of each, not the whole family.** The boards ask Google Fonts for
   * these at run time, which this app cannot do: it is served from a binary on loopback
   * to a developer who may have no internet, and `test/architecture.spec.ts` holds that.
   * Bundling the full families instead shipped woff2 files of Cyrillic and Vietnamese
   * that this interface has no glyphs to draw — the page is `lang="en"` and nothing in it
   * is translated.
   */
  css: [
    '@fontsource/newsreader/latin-400.css',
    '@fontsource/newsreader/latin-500.css',
    '@fontsource/instrument-sans/latin-400.css',
    '@fontsource/instrument-sans/latin-500.css',
    '@fontsource/instrument-sans/latin-600.css',
    '@fontsource/jetbrains-mono/latin-400.css',
    '@fontsource/jetbrains-mono/latin-500.css',
    '~/assets/css/tokens.css',
  ],

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
    /*
     * **Before the proxy, and that ordering is the whole point.** Nitro registers
     * `devHandlers` ahead of `devProxy`, so this runs first and the proxy never sees a
     * request it refuses.
     *
     * It is here because the proxy opened a hole: rewriting `Host` and `Origin` satisfies
     * two of the guard's three checks on behalf of whoever sent the request, and a
     * cross-origin form post from `https://example.com` was measured reaching the binary
     * with only the `Content-Type` check left between it and approving a draft. The proxy
     * now declines to speak for a caller it should not, so what it substitutes is true of
     * the request as well as of itself.
     *
     * The decision is in `dev-proxy.ts` rather than inline, so that
     * `test/dev-proxy.spec.ts` can fail if it is removed.
     */
    devHandlers: [
      {
        route: '/api/v1',
        handler: (event: {
          node: {
            req: { headers: Record<string, string | string[] | undefined> }
            res: { statusCode: number, setHeader: (name: string, value: string) => void }
          }
        }) => {
          const headers = event.node.req.headers
          const one = (value: string | string[] | undefined) =>
            Array.isArray(value) ? value[0] : value

          const decision = mayForward(one(headers.origin), one(headers.host))
          if (decision.forward) return undefined

          event.node.res.statusCode = 403
          event.node.res.setHeader('content-type', 'application/json; charset=utf-8')
          return {
            code: 'bad_request',
            message: decision.reason,
          }
        },
      },
    ],

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
