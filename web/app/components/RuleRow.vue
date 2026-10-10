<script setup lang="ts">
import type { Rule, RuleStage } from '~/types/desk'

/**
 * A rule in force: what it says, the failure it prevents, where it was decided, its counts
 * and how far it has graduated.
 *
 * ## The four-stage bar
 *
 * The board's own note explains it: "the four bars are the stages a rule moves through:
 * noticed, repository rule, lint check, CI check". `Rule.stage` is the contract's enum and
 * maps straight onto them, so this bar has a source and is drawn from the record:
 *
 * | `stage` | Segments filled | Drawn |
 * |---|---|---|
 * | `proposed` | 1 | ink — it has been noticed |
 * | `active` | 2 | ink — a repository or global rule |
 * | `lint` | 3 | accent — checked without a model |
 * | `ci` | 4 | accent — checked in CI |
 *
 * **Accent from `lint` onwards, and that is the board's drawing, not an invention**: its CI
 * rule has all four segments in the accent while both repository rules have theirs in ink.
 * The difference the colour is carrying is whether a model still has to think about it.
 *
 * `retired` draws no bar. A retired rule is not in force and does not appear in this group.
 */
const props = defineProps<{ rule: Rule }>()

/** How far along, as the bar's segments. */
const STAGE_INDEX: Record<RuleStage, number> = {
  proposed: 1,
  active: 2,
  lint: 3,
  ci: 4,
  retired: 0,
}

const segments = computed(() => {
  const reached = STAGE_INDEX[props.rule.stage]
  /* From `lint` on, the filled segments are the accent: no model is spending tokens here. */
  const fill = props.rule.stage === 'lint' || props.rule.stage === 'ci' ? 'now' : 'done'
  return Array.from({ length: 4 }, (_, at) => (at < reached ? fill : 'todo')) as Array<
    'done' | 'now' | 'todo'
  >
})

/** The stage as the screen words it. The server sends one; this is the fallback. */
const STAGE_WORDS: Record<RuleStage, string> = {
  proposed: 'Noticed',
  active: 'Repository rule',
  lint: 'Lint check',
  ci: 'CI check',
  retired: 'Retired',
}

const stageLabel = computed(() => props.rule.stage_label ?? STAGE_WORDS[props.rule.stage])

/**
 * The counts this stage shows, in the board's order.
 *
 * A rule checked without a model shows what the check caught; one a review applies shows
 * raised, kept and rejected. A count the store cannot answer is left out — not printed as
 * a nought, which would be a measurement nobody made.
 */
const counts = computed(() => {
  const c = props.rule.counts
  const shown: Array<{ value: number, label: string }> = []
  const add = (value: number | null | undefined, label: string) => {
    if (value != null) shown.push({ value, label })
  }

  if (isCheck.value) {
    add(c.caught_by_check, 'caught by the check')
    add(c.raised_in_a_review_since, 'raised in a review since')
  }
  else {
    add(c.raised, 'raised')
    add(c.kept, 'kept by you')
    add(c.rejected_with_reason, 'rejected')
  }
  return shown
})

/** Whether this rule is checked without a model, which changes what is worth saying. */
const isCheck = computed(() => props.rule.stage === 'lint' || props.rule.stage === 'ci')

/**
 * The provenance line, as one string.
 *
 * Assembled here rather than from a chain of `<template v-if>` fragments, which the linter
 * puts on separate lines and the renderer then separates with spaces: "Decided on  #214"
 * and "20 days ago ·" with the gap before the separator. A line of prose is one string.
 *
 * The board's own R-03 reads "Checked without a model · last said by a human reviewer 19
 * days ago", so that phrase leads for a rule that has graduated — unless the server has
 * written a note that says it already, which the fixtures' own note does.
 */
const facts = computed(() => {
  const rule = props.rule
  const said: string[] = []

  if (isCheck.value && !rule.note?.toLowerCase().includes('without a model')) {
    said.push('Checked without a model')
  }
  if (rule.counts.cited_by_findings != null) {
    said.push(`cited by ${count(rule.counts.cited_by_findings, 'later finding')}`)
  }
  if (rule.counts.last_human_mention_at) {
    said.push(`last said by a human reviewer ${sinceLabel(rule.counts.last_human_mention_at)}`)
  }
  else if (rule.last_cited_at) {
    said.push(`last cited ${sinceLabel(rule.last_cited_at)}`)
  }
  if (rule.note) said.push(rule.note)
  return said.join(' · ')
})

/** Whether the line leads with the pull request it was decided on. */
const decidedOn = computed(() => (isCheck.value ? null : props.rule.origin_pr))

/** Whether the check has graduated and a model is still raising it anyway. */
const checkNotDoingItsJob = computed(
  () => isCheck.value && (props.rule.counts.raised_in_a_review_since ?? 0) > 0,
)
</script>

<template>
  <div class="rule">
    <div class="statement">
      <h3 class="rd-display-statement">
        <span class="key rd-mono rd-meta">{{ rule.key }}</span>{{ rule.statement }}
      </h3>
      <div>
        <strong>Prevents.</strong> {{ rule.prevents }}
      </div>
      <p class="rd-meta provenance">
        <template v-if="decidedOn">
          Decided on <NuxtLink
            v-if="rule.origin_review_id"
            :to="`/reviews/${rule.origin_review_id}/understand`"
          >#{{ decidedOn }}</NuxtLink><span v-else>#{{ decidedOn }}</span><template v-if="facts">
            ·
          </template>
        </template>{{ facts }}
      </p>
    </div>

    <div class="measures">
      <div
        v-if="counts.length"
        class="counts"
      >
        <span
          v-for="one in counts"
          :key="one.label"
        >
          <span class="n">{{ one.value }}</span> {{ one.label }}
        </span>
      </div>
      <p
        v-else
        class="rd-meta nothing"
      >
        <NotMeasured because="nothing has counted this rule's findings yet" />
      </p>
      <div class="stage">
        <SegmentBar
          size="wide"
          :segments="segments"
          :label="`${stageLabel}: stage ${STAGE_INDEX[rule.stage]} of 4 — noticed, repository rule, lint check, CI check`"
        />
        <span
          class="stage-label"
          :class="{ 'stage-label--check': isCheck }"
        >{{ stageLabel }}</span>
      </div>
      <p
        v-if="checkNotDoingItsJob"
        class="warn"
      >
        A review has raised this since it became a check, so a model is still spending
        tokens on it.
      </p>
    </div>
  </div>
</template>

<style scoped>
.rule {
  display: flex;
  flex-wrap: wrap;
  gap: 14px 40px;
  padding: 24px 0;
  border-bottom: 1px solid var(--rd-line);
}

.statement {
  display: flex;
  flex: 3 1 420px;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.key {
  margin-right: 10px;
}

strong {
  font-weight: 600;
}

.provenance {
  margin: 0;
}

.measures {
  display: flex;
  flex: 1 1 260px;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.counts {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 20px;
  font-size: 14px;
}

.n {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.nothing {
  margin: 0;
}

.stage {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  font-size: 13px;
}

.stage-label {
  margin-left: 6px;
  font-weight: 500;
}

.stage-label--check {
  color: var(--rd-accent);
  font-weight: 600;
}

.warn {
  margin: 0;
  font-size: 13px;
  font-weight: 500;
  color: var(--rd-warn-ink);
}
</style>
