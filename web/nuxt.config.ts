import { readFileSync } from 'node:fs'

/*
 * Review Desk's dashboard.
 *
 * Server-side rendering is off and the build is static: the output is embedded in the
 * Rust binary by a later issue, and the JSON API is that binary's, never Nuxt's. There
 * are no server routes here and there is no `server/` directory.
 *
 * Fonts are bundled rather than fetched, so the generated site asks nothing of the
 * network — which is what lets the whole app run from a file server with no internet.
 */

interface ReviewFixture {
  review: { id: number }
  draft_status?: string | null
}

const reviews: ReviewFixture[] = JSON.parse(
  readFileSync(new URL('./app/data/reviews.json', import.meta.url), 'utf8'),
)

/*
 * Every route a static file server has to answer. Crawling cannot find a parameterised
 * route, so each one is named: a deep link to a review has to resolve from a plain
 * directory of files.
 */
const prerenderRoutes = [
  '/',
  '/rules',
  ...reviews.map(item => `/reviews/${item.review.id}`),
  ...reviews.map(item => `/reviews/${item.review.id}/draft`),
]

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

  compatibilityDate: '2026-10-08',

  nitro: {
    prerender: {
      routes: prerenderRoutes,
      crawlLinks: false,
      failOnError: true,
    },
  },

  typescript: { strict: true },

  eslint: {
    config: { stylistic: true },
  },
})
