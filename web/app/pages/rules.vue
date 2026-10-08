<script setup lang="ts">
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
const page = desk.rules(repo)

const options = computed(() => [
  ...desk.repos.value.map(item => ({ value: item.repo, label: item.repo })),
  { value: ALL_REPOSITORIES, label: 'All repositories' },
])

onMounted(() => {
  if (repo.value === ALL_REPOSITORIES && desk.repos.value[0]) {
    repo.value = desk.repos.value[0].repo
  }
})

const asking = ref<{ id: number, kind: 'dismiss' | 'retire' } | null>(null)
const notice = ref<{ text: string, ok: boolean } | null>(null)

const stats = computed(() => {
  const s = page.value.summary
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
})

function promote(id: number, key: string) {
  desk.promoteRule(id)
  notice.value = { text: `${key} promoted. It applies from the next review.`, ok: true }
  asking.value = null
}

function dismiss(id: number, reason: string) {
  const result = desk.dismissRule(id, reason)
  notice.value = { text: result.message, ok: result.ok }
  if (result.ok) asking.value = null
}

function retire(id: number, reason: string) {
  const result = desk.retireRule(id, reason)
  notice.value = { text: result.message, ok: result.ok }
  if (result.ok) asking.value = null
}

function defer(id: number) {
  notice.value = { text: desk.deferRule(id, 60), ok: true }
  asking.value = null
}
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
      :tone="notice.ok ? 'plain' : 'problem'"
    >
      {{ notice.text }}
    </DeskNotice>

    <div class="stats">
      <StatTile
        v-for="stat in stats"
        :key="stat.label"
        :label="stat.label"
        :value="stat.value"
        :note="stat.note"
      />
    </div>

    <DeskSection
      v-if="page.proposed.length"
      heading="Proposed"
      :count="page.proposed.length"
    >
      <DeskCard
        v-for="rule in page.proposed"
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
          <p class="rd-note why">
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
            @click="promote(rule.id, rule.key)"
          >
            Promote
          </button>
          <button
            type="button"
            class="rd-btn"
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
      :count="page.active.length"
    >
      <RuleCard
        v-for="rule in page.active"
        :key="rule.id"
        :rule="rule"
      />
      <p
        v-if="page.active.length === 0"
        class="rd-empty"
      >
        No rule applies here yet. A rule comes out of a finding you kept, not out of a
        style guide.
      </p>
    </DeskSection>

    <DeskSection
      v-if="page.stale.length"
      heading="Going stale"
      :count="page.stale.length"
    >
      <ActionRow
        v-for="rule in page.stale"
        :key="rule.id"
        tone="pending"
      >
        <template #title>
          <span class="rd-mono rd-meta">{{ rule.key }}</span>
          {{ rule.statement }}
        </template>
        <template #meta>
          {{ rule.note }}
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
            @click="asking = { id: rule.id, kind: 'retire' }"
          >
            Retire
          </button>
          <button
            type="button"
            class="rd-btn"
            @click="defer(rule.id)"
          >
            Keep 60 more days
          </button>
        </template>
      </ActionRow>
    </DeskSection>
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
