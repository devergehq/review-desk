import { defineVitestConfig } from '@nuxt/test-utils/config'

/*
 * jsdom rather than happy-dom, for one reason: DOMPurify needs a DOM faithful enough to
 * parse `<details>`, and happy-dom silently drops tags it does not model — which would
 * make the draft preview's sanitiser look broken here and fine in a browser, or worse,
 * the other way round.
 *
 * Tests that mount a screen declare `// @vitest-environment nuxt` at the top of the file,
 * so they get auto-imports and the router; the rest are plain unit tests.
 */
export default defineVitestConfig({
  test: {
    environment: 'jsdom',
    include: ['test/**/*.spec.ts'],
    environmentOptions: {
      nuxt: { domEnvironment: 'jsdom' },
    },
  },
})
