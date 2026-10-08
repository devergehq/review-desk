<script setup lang="ts">
/** The inbox: everything waiting on the developer, and nothing else. */
const desk = useReviewDesk()
const inbox = desk.inbox

useHead({ title: 'Inbox · Review Desk' })

const link = ref('')

/*
 * What the desk said back. `blocked` is reserved for a read Review Desk needed and could
 * not make: the developer's `gh` is missing or logged out, nothing was created, and the
 * fix is a command rather than a different link.
 */
const notice = ref<{ text: string, tone: 'plain' | 'problem' | 'blocked' } | null>(null)

function start() {
  const result = desk.startReview(link.value)
  if (result.ok) {
    notice.value = { text: result.message, tone: 'plain' }
    link.value = ''
    return
  }
  notice.value = {
    text: result.error.message,
    tone:
      result.error.code === 'github_unavailable' || result.error.code === 'store_unavailable'
        ? 'blocked'
        : 'problem',
  }
}

const handled = computed(() => {
  const h = inbox.value.handled_without_you
  const lines: string[] = []
  if (h.description_problems_caught != null) {
    lines.push(
      `${count(h.description_problems_caught, 'description problem')} caught by the CI check`,
    )
  }
  if (h.nits_filed != null && h.nit_tickets != null) {
    lines.push(`${count(h.nits_filed, 'nit')} filed as ${count(h.nit_tickets, 'ticket')} for an agent`)
  }
  return lines
})

const anythingWaiting = computed(() => inbox.value.totals.items > 0)
</script>

<template>
  <div class="inbox">
    <div class="column">
      <div>
        <h1>Waiting on you</h1>
        <p class="rd-meta summary">
          {{ count(inbox.totals.items, 'item') }} across
          {{ count(inbox.totals.repos, 'repository', 'repositories') }}
        </p>
      </div>

      <p
        v-if="!anythingWaiting"
        class="rd-empty"
      >
        Nothing is waiting on you. Anything a session produces turns up here.
      </p>

      <DeskSection
        v-if="inbox.briefs_to_confirm.length"
        heading="Confirm the problem"
        :count="inbox.briefs_to_confirm.length"
      >
        <ActionRow
          v-for="item in inbox.briefs_to_confirm"
          :key="`brief-${item.review_id}`"
        >
          <template #title>
            <PrRef :pr="item.pull_request.pr" /> {{ item.pull_request.title }}
          </template>
          <template #meta>
            {{ item.pull_request.repo }}
            <template v-if="item.ticket_id">
              · {{ item.ticket_id }}
            </template>
            · brief ready {{ sinceLabel(item.brief_ready_at) }} ·
            {{
              item.blocks_later_layers
                ? 'nothing else runs until you confirm'
                : 'nothing posts until you confirm'
            }}
          </template>
          <template #actions>
            <NuxtLink
              :to="`/reviews/${item.review_id}`"
              class="rd-btn rd-btn--primary"
            >
              Read brief
            </NuxtLink>
          </template>
        </ActionRow>
      </DeskSection>

      <DeskSection
        v-if="inbox.drafts_to_approve.length"
        heading="Approve a draft"
        :count="inbox.drafts_to_approve.length"
      >
        <ActionRow
          v-for="item in inbox.drafts_to_approve"
          :key="`draft-${item.review_id}`"
        >
          <template #title>
            <PrRef :pr="item.pull_request.pr" /> {{ item.pull_request.title }}
          </template>
          <template #meta>
            {{ item.pull_request.repo }} · round {{ item.round }}
            <template v-if="item.is_delta">
              · delta since your last review
            </template>
            <template v-if="severityTally(item.severities)">
              · {{ severityTally(item.severities) }}
            </template>
            <template v-if="item.held_back > 0">
              · {{ count(item.held_back, 'finding') }} held back for a ticket
            </template>
          </template>
          <template #actions>
            <NuxtLink
              :to="`/reviews/${item.review_id}/draft`"
              class="rd-btn rd-btn--primary"
            >
              Open draft
            </NuxtLink>
          </template>
        </ActionRow>
      </DeskSection>

      <DeskSection
        v-if="inbox.author_replies.length"
        heading="Author replied"
        :count="inbox.author_replies.length"
      >
        <ActionRow
          v-for="item in inbox.author_replies"
          :key="`reply-${item.review_id}`"
        >
          <template #title>
            <PrRef :pr="item.pull_request.pr" /> {{ item.pull_request.title }}
          </template>
          <template #meta>
            {{ item.pull_request.repo }} ·
            {{
              item.events
                .map(event =>
                  event.kind === 'rejected'
                    ? `finding ${event.finding_key} rejected with a reason`
                    : event.kind === 'code_changed_after'
                      ? `${event.finding_key} fixed in a later commit`
                      : `${event.finding_key} ${event.kind.replace(/_/g, ' ')}`,
                )
                .join(' · ')
            }}
          </template>
          <template #actions>
            <NuxtLink
              :to="`/reviews/${item.review_id}`"
              class="rd-btn"
            >See reply</NuxtLink>
          </template>
        </ActionRow>
      </DeskSection>

      <DeskSection
        v-if="inbox.own_prs_without_reviewer.length"
        heading="Your PRs with no reviewer"
        :count="inbox.own_prs_without_reviewer.length"
      >
        <ActionRow
          v-for="item in inbox.own_prs_without_reviewer"
          :key="`own-${item.pull_request.repo}-${item.pull_request.pr}`"
        >
          <template #title>
            <PrRef :pr="item.pull_request.pr" /> {{ item.pull_request.title }}
          </template>
          <template #meta>
            {{ item.pull_request.repo }} · {{ openForLabel(item.opened_at) }} ·
            {{ descriptionLintLabel(item.description_lint) }}
          </template>
          <template #actions>
            <!--
              Review Desk never writes to GitHub, so this is a link out rather than a
              button: asking for a reviewer happens on the pull request itself.
            -->
            <a
              class="rd-btn"
              :href="`https://github.com/${item.pull_request.repo}/pull/${item.pull_request.pr}`"
              target="_blank"
              rel="noreferrer noopener"
            >
              Request on GitHub<span class="rd-visually-hidden">
                for {{ item.pull_request.repo }}#{{ item.pull_request.pr }}, opens in a new tab</span>
            </a>
          </template>
        </ActionRow>
      </DeskSection>

      <DeskSection
        v-if="inbox.proposed_rules.length"
        heading="Proposed rule"
        :count="inbox.proposed_rules.length"
      >
        <ActionRow
          v-for="item in inbox.proposed_rules"
          :key="`rule-${item.rule_id}`"
        >
          <template #title>
            {{ item.statement }}
          </template>
          <template #meta>
            {{ item.why_proposed }}
          </template>
          <template #actions>
            <NuxtLink
              to="/rules"
              class="rd-btn"
            >Decide</NuxtLink>
          </template>
        </ActionRow>
      </DeskSection>
    </div>

    <aside class="aside">
      <DeskCard class="panel">
        <label
          class="rd-label"
          for="pr-link"
        >Start a review</label>
        <input
          id="pr-link"
          v-model="link"
          class="rd-field"
          type="text"
          inputmode="url"
          placeholder="Paste a pull request link"
          aria-describedby="pr-link-help"
          @keyup.enter="start"
        >
        <button
          type="button"
          class="rd-btn rd-btn--ink"
          @click="start"
        >
          Understand it first
        </button>
        <DeskNotice
          v-if="notice"
          :tone="notice.tone"
        >
          {{ notice.text }}
        </DeskNotice>
        <p
          id="pr-link-help"
          class="rd-meta help"
        >
          Starts with the problem brief. No findings are written until you confirm it.
        </p>
      </DeskCard>

      <DeskCard class="panel">
        <div class="rd-label">
          In progress · {{ inbox.in_progress.length }}
        </div>
        <p
          v-if="inbox.in_progress.length === 0"
          class="rd-meta help"
        >
          No session is running.
        </p>
        <div
          v-for="item in inbox.in_progress"
          :key="item.review_id"
          class="progress"
        >
          <NuxtLink
            class="progress-title"
            :to="`/reviews/${item.review_id}`"
          >
            <PrRef :pr="item.pull_request.pr" /> {{ item.pull_request.title }}
          </NuxtLink>
          <p class="rd-meta progress-meta">
            Layer {{ item.layer_number }} of {{ item.layers_total }} ·
            {{ layerLabel(item.layer_name).toLowerCase() }} ·
            <span :class="{ unknown: item.session_alive == null }">
              {{ sessionLabel(item.session_alive) }}
            </span>
          </p>
          <MeterBar
            :value="item.layer_number"
            :max="item.layers_total"
            :label="`Layer ${item.layer_number} of ${item.layers_total} on #${item.pull_request.pr}`"
          />
        </div>
      </DeskCard>

      <DeskCard class="panel panel--tight">
        <div class="rd-label">
          Handled without you
          <template v-if="inbox.handled_without_you.window_days === 7">
            this week
          </template>
          <template v-else>
            in {{ count(inbox.handled_without_you.window_days, 'day') }}
          </template>
        </div>
        <p
          v-for="line in handled"
          :key="line"
          class="rd-meta line"
        >
          {{ line }}
        </p>
        <p
          v-if="handled.length === 0"
          class="rd-meta line"
        >
          Nothing has reported in yet.
        </p>
      </DeskCard>
    </aside>
  </div>
</template>

<style scoped>
.inbox {
  display: flex;
  flex-wrap: wrap;
  gap: 28px;
  align-items: flex-start;
}

.column {
  flex: 999 1 560px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.summary {
  margin: 4px 0 0;
}

.aside {
  flex: 1 1 300px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.panel--tight {
  gap: 6px;
}

.help,
.line {
  margin: 0;
}

.progress {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.progress-title {
  font-weight: 500;
  color: var(--rd-ink);
  text-decoration: none;
}

.progress-title:hover {
  color: var(--rd-accent-dark);
  text-decoration: underline;
}

.progress-meta {
  margin: 0;
}

.unknown {
  border-bottom: 1px dashed var(--rd-line-strong);
}
</style>
