<script setup lang="ts">
import type { RulesPage } from '~/types/desk'
import type { WriteResult } from '~/composables/useReviewDesk'
import type { DeskNotice } from '~/utils/format'

/**
 * The rules: what the desk has learned, and what it is proposing to learn.
 *
 * The app proposes and the developer promotes. Nothing here promotes itself, and
 * dismissing or retiring asks why — a rule turned down with no reason is proposed again
 * next week by something with no memory of having been told no.
 */
const desk = useReviewDesk()

useHead({ title: 'Rules · Review Desk' })

const repo = ref(ALL_REPOSITORIES)
const repos = desk.repos()
const page = desk.rules(repo)

const options = computed(() => [
  ...(repos.data.value ?? []).map(item => ({ value: item.repo, label: item.repo })),
  { value: ALL_REPOSITORIES, label: 'All repositories' },
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

function stats(data: RulesPage) {
  const s = data.summary
  return [
    {
      label: 'Active rules',
      value: String(s.active_rules),
      note: `cap of ${s.active_rules_cap} per repository`,
    },
    {
      label: 'Same answer on re-run',
      value: figure(s.replay_matched, s.replay_total) ?? 'Not measured yet',
      note:
        s.replay_matched == null
          ? 'the replay check is a later slice'
          : 'findings matched on the last replay',
    },
    {
      label: 'Findings you kept',
      value: figure(s.findings_kept, s.findings_total) ?? 'Not measured yet',
      note:
        s.findings_kept == null
          ? 'nothing has harvested a posted review yet'
          : `last ${count(s.findings_window_days ?? 30, 'day')}, draft to posted`,
    },
    {
      label: 'Raised twice on one PR',
      value: figure(s.raised_twice_on_one_pr) ?? 'Not measured yet',
      note:
        s.raised_twice_on_one_pr == null
          ? 'needs a second round to compare'
          : 'reviews that went in a circle',
    },
  ]
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
    <div class="head">
      <div class="heading">
        <h1>Rules</h1>
        <p class="rd-meta blurb">
          Every rule names the failure it prevents. A preference with no consequence does
          not qualify.
        </p>
      </div>
      <div class="filter">
        <label
          class="rd-meta"
          for="repo"
        >Repository</label>
        <select
          id="repo"
          v-model="repo"
          class="rd-field"
          :disabled="repos.pending.value && !repos.loaded.value"
        >
          <option
            v-for="option in options"
            :key="option.value"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </select>
      </div>
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
        <div class="stats">
          <StatTile
            v-for="stat in stats(data)"
            :key="stat.label"
            :label="stat.label"
            :value="stat.value"
            :note="stat.note"
          />
        </div>

        <DeskSection
          v-if="data.proposed.length"
          heading="Proposed"
          :count="data.proposed.length"
        >
          <DeskCard
            v-for="rule in data.proposed"
            :key="rule.id"
            tone="chosen"
            class="proposal"
          >
            <div class="proposal-body">
              <div class="head-line">
                <span class="rd-mono rd-meta">{{ rule.key }}</span>
                <h3 class="statement">
                  {{ rule.statement }}
                </h3>
              </div>
              <p
                v-if="rule.why_proposed"
                class="rd-note why"
              >
                <strong>Why proposed:</strong> {{ rule.why_proposed }}
              </p>
              <p class="rd-note why">
                <strong>Prevents:</strong> {{ rule.prevents }}
              </p>
              <p
                v-if="rule.note"
                class="rd-meta note"
              >
                {{ rule.note }}
              </p>
            </div>
            <div class="actions">
              <button
                type="button"
                class="rd-btn rd-btn--primary"
                :disabled="working"
                @click="promote(rule.id)"
              >
                Promote
              </button>
              <button
                type="button"
                class="rd-btn"
                :disabled="working"
                @click="asking = { id: rule.id, kind: 'dismiss' }"
              >
                Dismiss
              </button>
            </div>
            <ReasonPrompt
              v-if="asking?.id === rule.id && asking.kind === 'dismiss'"
              :id="`dismiss-${rule.id}`"
              label="Why is this not a rule?"
              action="Dismiss the proposal"
              @confirm="reason => dismiss(rule.id, reason)"
              @cancel="asking = null"
            />
          </DeskCard>
        </DeskSection>

        <DeskSection
          heading="Active"
          :count="data.active.length"
        >
          <RuleCard
            v-for="rule in data.active"
            :key="rule.id"
            :rule="rule"
          />
          <p
            v-if="data.active.length === 0"
            class="rd-empty"
          >
            No rule applies here yet. A rule comes out of a finding you kept, not out of a
            style guide.
          </p>
        </DeskSection>

        <DeskSection
          v-if="data.stale.length"
          heading="Going stale"
          :count="data.stale.length"
        >
          <ActionRow
            v-for="rule in data.stale"
            :key="rule.id"
            tone="pending"
          >
            <template #title>
              <span class="rd-mono rd-meta">{{ rule.key }}</span>
              {{ rule.statement }}
            </template>
            <template #meta>
              {{ rule.note ?? staleLabel(rule) }}
            </template>
            <template #extra>
              <ReasonPrompt
                v-if="asking?.id === rule.id && asking.kind === 'retire'"
                :id="`retire-${rule.id}`"
                label="Why is it being retired?"
                action="Retire the rule"
                @confirm="reason => retire(rule.id, reason)"
                @cancel="asking = null"
              />
            </template>
            <template #actions>
              <button
                type="button"
                class="rd-btn"
                :disabled="working"
                @click="asking = { id: rule.id, kind: 'retire' }"
              >
                Retire
              </button>
              <button
                type="button"
                class="rd-btn"
                :disabled="working"
                @click="defer(rule.id)"
              >
                Keep 60 more days
              </button>
            </template>
          </ActionRow>
        </DeskSection>
      </template>

      <!--
        A repository with no rules at any stage. It reads as a desk that has not learned
        anything yet, which is true of a new database and is also the honest answer for a
        repository whose reviews have all been clean.
      -->
      <template #empty>
        <DeskCard class="fresh">
          <h1>No rules yet</h1>
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
  gap: 24px;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  align-items: flex-end;
}

.heading {
  flex: 1 1 360px;
  min-width: 0;
}

.blurb {
  margin: 4px 0 0;
}

.filter {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.filter .rd-field {
  width: auto;
  min-width: 200px;
}

.stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 12px;
}

.proposal {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  align-items: center;
}

.proposal-body {
  flex: 1 1 420px;
  min-width: 0;
}

.head-line {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.statement {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.why {
  margin: 4px 0 0;
}

.why strong {
  font-weight: 600;
}

.note {
  margin: 4px 0 0;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
</style>
