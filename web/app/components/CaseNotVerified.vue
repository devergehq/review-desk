<script setup lang="ts">
import type { ReviewLayer } from '~/types/desk'

/**
 * **Not verified** — what the review did not look at. S3's last band, DEV-1327.
 *
 * The review's own `not_checked`, in the session's words, which Findings also draws beside
 * its list. Here as well because Overview's *What wasn't checked?* links to this tab, as S2
 * does, and a link has to land on the answer. When the field is empty the band says what
 * the empty field means — `notVerified` chooses those words — rather than disappearing,
 * because an absent section reads as *nothing was missed*.
 *
 * **One item, not S3's list of titled items**: `not_checked` is one piece of text, and
 * splitting it into a title and a reason would be writing what the session did not.
 */
const { notChecked, layers } = defineProps<{
  notChecked: string | null | undefined
  layers: readonly Pick<ReviewLayer, 'name' | 'state'>[]
}>()

const said = computed(() => notVerified(notChecked, layers))

/** Whether these are the session's own words, drawn as recorded text. */
const written = computed(() => !!notChecked?.trim())
</script>

<template>
  <section
    id="not-verified"
    class="band"
  >
    <CaseBandHead :heading="bandLabel('not-verified')" />
    <p
      class="said"
      :class="written ? 'rd-recorded' : 'rd-note'"
    >
      <TickedText
        v-if="written"
        :text="said.says"
      />
      <template v-else>
        {{ said.says }}
      </template>
    </p>
  </section>
</template>

<style scoped>
.band {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.said {
  margin: 0;
  padding: 0 0 14px;
  border-bottom: 1px solid var(--rd-line);
}
</style>
