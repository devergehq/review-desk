// @ts-check
import withNuxt from './.nuxt/eslint.config.mjs'

export default withNuxt(
  {
    /*
     * `app/types/contract.ts` is generated from `contract/openapi.yaml` by
     * `pnpm run types:contract`. It is committed, so that a contract change shows up as a
     * type change in the pull request's diff and so that a build needs neither the
     * generator nor the contract beside it — but it is not written by hand, and formatting
     * a generated file would mean either reformatting it on every generation or seeing two
     * thousand style errors on every lint. `test/contract-types.spec.ts` is what checks it
     * instead, by regenerating and comparing.
     */
    ignores: ['app/types/contract.ts'],
  },
)
