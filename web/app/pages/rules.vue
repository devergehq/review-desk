<script setup lang="ts">
import type { Rule, RulesPage } from '~/types/desk'
import type { WriteResult } from '~/composables/useReviewDesk'
import type { DeskNotice } from '~/utils/format'

/**
 * The rules: what the reviewer has learned, and what it is proposing to learn.
 *
 * The app proposes and the developer promotes. Nothing here promotes itself, and dismissing
 * or retiring asks why — a rule turned down with no reason is proposed again next week by
 * something with no memory of having been told no.
 *
 * ## Three of the four headline figures have no source, and the board draws all four
 *
 * | Figure | Contract field | Today |
 * |---|---|---|
 * | rules in force, of at most 20 | `active_rules`, `active_rules_cap` | **measured** |
 * | findings you kept | `findings_kept` / `findings_total` | null until a posted review is harvested |
 * | same answer on a re-run | `replay_matched` / `replay_total` | null until the replay check exists |
 * | reviews that went in a circle | `raised_twice_on_one_pr` | null until a second round can be compared |
 *
 * Each is drawn where the board draws it, in the words "Not measured yet" with the reason
 * behind them, rather than as a nought. The contract was written for this: every one of
 * those fields is nullable precisely so that the absence of a measurement can be told from
 * a measurement of zero.
 *
 * The four-stage bar, which the dispatch brief listed as having no source, **does** have
 * one: `Rule.stage` is the contract's enum and `RuleRow` maps it onto the board's four
 * stages. It is drawn from the record.
 */
const desk = useReviewDesk()

useHead({ title: 'Rules · Review Desk' })

const repo = ref(ALL_REPOSITORIES)
const repos = desk.repos()
const page = desk.rules(repo)

const tabs = computed(() => [
  { value: ALL_REPOSITORIES, label: 'All repositories' },
  ...(repos.data.value ?? []).map(item => ({
    value: item.repo,
    label: item.repo,
    count: item.active_rules,
  })),
])

/*
 * The first repository, once the list has arrived, so the screen opens on one repository's
 * rules rather than on every repository's at once. Watched rather than done on mount: the
 * list is a read now, and on mount there is nothing to pick from yet.
 *
 * **It happens once, and the latch is what makes that true.** Without it the condition is
 * standing rather than initial, and every rule action re-ran it: a write refreshes every
 * open read, `listRepos` hands back a freshly mapped array each time, so the watcher fired
 * again — and a developer who had chosen "All repositories" and pressed Promote was moved
 * back to the first repository, reading a different page of rules than the one they acted on.
 */
const picked = ref(false)

watch(
  () => repos.data.value,
  (list) => {
    if (picked.value) return
    if (!list) return
    picked.value = true
    if (repo.value === ALL_REPOSITORIES && list[0]) repo.value = list[0].repo
  },
  { immediate: true },
)

const asking = ref<{ id: number, kind: 'dismiss' | 'retire' } | null>(null)
const working = ref(false)
const notice = ref<DeskNotice | null>(null)

/** Whether this repository has no rules at any stage. */
function isEmpty(data: RulesPage): boolean {
  return data.proposed.length === 0 && data.active.length === 0 && data.stale.length === 0
}

/** The four figures across the top, each with what to say when nothing measured it. */
function figures(data: RulesPage) {
  const s = data.summary
  return [
    {
      value: String(s.active_rules),
      label: s.active_rules === 1 ? 'rule in force' : 'rules in force',
      note: `of at most ${s.active_rules_cap} for a repository`,
      because: '',
      tone: 'plain' as const,
    },
    {
      value: figure(s.findings_kept, s.findings_total),
      label: 'findings you kept',
      note:
        s.findings_kept == null
          ? 'nothing has harvested a posted review yet'
          : `last ${count(s.findings_window_days ?? 30, 'day')}, draft to posted`,
      because: 'no posted review has been harvested, so no draft-to-posted pair exists yet',
      tone: 'plain' as const,
    },
    {
      value: figure(s.replay_matched, s.replay_total),
      label: 'same answer on a re-run',
      note:
        s.replay_matched == null
          ? 'the replay consistency check is a later slice'
          : 'the last replay of one review',
      because: 'Review Desk has no replay yet, so nothing has been re-run to compare',
      tone: 'plain' as const,
    },
    {
      value: figure(s.raised_twice_on_one_pr),
      label: 'reviews went in a circle',
      note:
        s.raised_twice_on_one_pr == null
          ? 'needs a second round to compare against the first'
          : 'a finding open for 3 rounds or more',
      because: 'no review here has a second round to compare against the first',
      tone: 'warn' as const,
    },
  ]
}

/** The findings a proposal came out of, as the chips the board draws. */
function citations(rule: Rule) {
  /* Not in the contract's `required` list, so a rule may arrive without the array. */
  return (rule.origin_findings ?? []).map(one => ({
    label: `F${one.seq}`,
    to: `/reviews/${one.review_id}/understand`,
  }))
}

async function act(run: () => Promise<WriteResult>) {
  working.value = true
  const result = await run()
  notice.value = noticeFor(result)
  working.value = false
  /*
   * The reason prompt stays open on a refusal, with what the developer wrote still in it.
   * A refusal here is almost always "say why", and clearing the box would be the one
   * response guaranteed to make them type it twice.
   */
  if (result.ok) asking.value = null
}

const promote = (id: number) => act(() => desk.promoteRule(id))
const dismiss = (id: number, reason: string) => act(() => desk.dismissRule(id, reason))
const retire = (id: number, reason: string) => act(() => desk.retireRule(id, reason))
const defer = (id: number) => act(() => desk.deferRule(id, 60))
</script>

<template>
  <div class="rules">
    <div class="top">
      <div class="headline">
        <h1 class="rd-display">
          What the reviewer has learned
        </h1>
        <p class="blurb rd-measure">
          Every rule names the failure it prevents. The app proposes them from what you keep
          and cut; only you promote one.
        </p>
      </div>
      <DecisionTrack
        v-model="repo"
        label="Which repository's rules to show"
        :options="tabs"
        :disabled="repos.pending.value && !repos.loaded.value"
      />
    </div>

    <DeskNotice
      v-if="notice"
      :tone="notice.tone"
    >
      {{ notice.text }}
    </DeskNotice>

    <DeskState
      :resource="page"
      :is-empty="isEmpty"
      what="the rules"
    >
      <template #default="{ data }">
        <div class="figures">
          <StatFigure
            v-for="one in figures(data)"
            :key="one.label"
            :value="one.value"
            :label="one.label"
            :note="one.note"
            :unknown-because="one.because"
            :tone="one.tone"
          />
        </div>

        <DeskSection
          v-if="data.proposed.length"
          heading="Proposed"
          tone="open"
          :count="data.proposed.length"
          note="waiting on you"
        >
          <div class="proposals">
            <RuleProposal
              v-for="rule in data.proposed"
              :key="rule.id"
              :rule="rule"
              :citations="citations(rule)"
              :working="working"
              @promote="promote(rule.id)"
              @dismiss="asking = { id: rule.id, kind: 'dismiss' }"
            >
              <template
                v-if="asking?.id === rule.id && asking.kind === 'dismiss'"
                #reason
              >
                <ReasonPrompt
                  :id="`dismiss-${rule.id}`"
                  label="Why is this not a rule?"
                  action="Dismiss the proposal"
                  @confirm="reason => dismiss(rule.id, reason)"
                  @cancel="asking = null"
                />
              </template>
            </RuleProposal>
          </div>
        </DeskSection>

        <DeskSection
          heading="In force"
          :count="data.active.length"
          :note="data.active.length === data.summary.active_rules
            ? null
            : `${data.active.length} of ${data.summary.active_rules} shown`"
        >
          <RuleRow
            v-for="rule in data.active"
            :key="rule.id"
            :rule="rule"
          />
          <p
            v-if="data.active.length === 0"
            class="rd-empty gap"
          >
            No rule applies here yet. A rule comes out of a finding you kept, not out of a
            style guide.
          </p>
          <p
            v-else
            class="rd-meta legend"
          >
            The four bars are the stages a rule moves through: noticed, repository rule,
            lint check, CI check.
          </p>
        </DeskSection>

        <DeskSection
          v-if="data.stale.length"
          heading="Going stale"
          tone="open"
          :count="data.stale.length"
        >
          <div class="stale">
            <StaleRule
              v-for="rule in data.stale"
              :key="rule.id"
              :rule="rule"
              :working="working"
              @retire="asking = { id: rule.id, kind: 'retire' }"
              @defer="defer(rule.id)"
            >
              <template
                v-if="asking?.id === rule.id && asking.kind === 'retire'"
                #reason
              >
                <ReasonPrompt
                  :id="`retire-${rule.id}`"
                  label="Why is it being retired?"
                  action="Retire the rule"
                  @confirm="reason => retire(rule.id, reason)"
                  @cancel="asking = null"
                />
              </template>
            </StaleRule>
          </div>
        </DeskSection>
      </template>

      <!--
        A repository with no rules at any stage. It reads as a desk that has not learned
        anything yet, which is true of a new database and is also the honest answer for a
        repository whose reviews have all been clean.
      -->
      <template #empty>
        <DeskCard
          tone="raised"
          class="fresh"
        >
          <h1 class="rd-display">
            No rules yet
          </h1>
          <p class="rd-note rd-measure">
            Nothing has been learned here. A rule comes out of a finding you kept — the
            harvester proposes one when the same finding keeps being raised, or the same
            kind keeps being cut, and you decide whether it becomes a rule.
          </p>
          <p class="rd-note rd-measure">
            Every rule names the failure it prevents. A preference with no consequence does
            not qualify, which is why there is no way to write one here by hand.
          </p>
        </DeskCard>
      </template>
    </DeskState>
  </div>
</template>

<style scoped>
.rules {
  display: flex;
  flex-direction: column;
  gap: 48px;
}

.top {
  display: flex;
  flex-wrap: wrap;
  gap: 20px 40px;
  align-items: flex-end;
}

.headline {
  flex: 1 1 420px;
  min-width: 0;
}

.blurb {
  margin: 10px 0 0;
  color: var(--rd-ink-dim);
}

/*
 * Four figures across, each with its own rule above it. `auto-fit` with a floor of 220px
 * so they become two and then one without anything being told a breakpoint.
 */
.figures {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(220px, 100%), 1fr));
  gap: 0 32px;
}

.proposals,
.stale {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.legend {
  padding-top: 14px;
}

.gap {
  margin-top: 16px;
}

.fresh {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 32px;
}
</style>
