<script setup lang="ts">
import type { Inbox } from '~/types/desk'

/** The inbox: everything waiting on the developer, and nothing else. */
const desk = useReviewDesk()
const inbox = desk.inbox()

useHead({ title: 'Inbox · Review Desk' })

const link = ref('')
const starting = ref(false)

/*
 * What the desk said back. `blocked` is for something Review Desk needed and could not
 * get — the developer's `gh` is missing or logged out, or its own database is unreadable:
 * nothing was created, and the fix is a command rather than a different link.
 */
const notice = ref<{ text: string, tone: 'plain' | 'problem' | 'blocked' } | null>(null)

async function start() {
  starting.value = true
  const result = await desk.startReview(link.value)
  starting.value = false

  if (result.ok) {
    notice.value = { text: result.message, tone: 'plain' }
    link.value = ''
    return
  }

  /* The link stays in the field: nothing is wrong with it, so retyping it is waste. */
  notice.value = { text: refusal(result.error), tone: refusalTone(result.error) }
}

/**
 * Whether the inbox has anything in it at all.
 *
 * The five waiting groups *and* the two panels. A database with one running session is not
 * an empty desk, even though nothing is waiting on the developer — "nothing is waiting on
 * you" and "there is nothing here yet" are different sentences and a new database deserves
 * the second one.
 */
function isEmpty(data: Inbox): boolean {
  return data.totals.items === 0 && data.in_progress.length === 0
}

function handledLines(data: Inbox): string[] {
  const h = data.handled_without_you
  const lines: string[] = []
  if (h.description_problems_caught != null) {
    lines.push(
      `${count(h.description_problems_caught, 'description problem')} caught by the CI check`,
    )
  }
  if (h.nits_filed != null && h.nit_tickets != null) {
    lines.push(
      `${count(h.nits_filed, 'nit')} filed as ${count(h.nit_tickets, 'ticket')} for an agent`,
    )
  }
  return lines
}
</script>

<template>
  <div class="inbox">
    <DeskState
      :resource="inbox"
      :is-empty="isEmpty"
      what="the inbox"
    >
      <template #default="{ data }">
        <div class="column">
          <div>
            <h1>Waiting on you</h1>
            <p class="rd-meta summary">
              {{ count(data.totals.items, 'item') }} across
              {{ count(data.totals.repos, 'repository', 'repositories') }}
            </p>
          </div>

          <p
            v-if="data.totals.items === 0"
            class="rd-empty"
          >
            Nothing is waiting on you. Anything a session produces turns up here.
          </p>

          <DeskSection
            v-if="data.briefs_to_confirm.length"
            heading="Confirm the problem"
            :count="data.briefs_to_confirm.length"
          >
            <ActionRow
              v-for="item in data.briefs_to_confirm"
              :key="`brief-${item.review_id}`"
            >
              <template #title>
                <PrRef :pr="item.pull_request.pr" />
                {{ item.pull_request.title ?? 'no title recorded' }}
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
            v-if="data.drafts_to_approve.length"
            heading="Approve a draft"
            :count="data.drafts_to_approve.length"
          >
            <ActionRow
              v-for="item in data.drafts_to_approve"
              :key="`draft-${item.review_id}`"
            >
              <template #title>
                <PrRef :pr="item.pull_request.pr" />
                {{ item.pull_request.title ?? 'no title recorded' }}
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
            v-if="data.author_replies.length"
            heading="Author replied"
            :count="data.author_replies.length"
          >
            <ActionRow
              v-for="item in data.author_replies"
              :key="`reply-${item.review_id}`"
            >
              <template #title>
                <PrRef :pr="item.pull_request.pr" />
                {{ item.pull_request.title ?? 'no title recorded' }}
              </template>
              <template #meta>
                {{ item.pull_request.repo }} ·
                {{ item.events.map(event => replyLabel(event)).join(' · ') }}
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
            v-if="data.own_prs_without_reviewer.length"
            heading="Your PRs with no reviewer"
            :count="data.own_prs_without_reviewer.length"
          >
            <ActionRow
              v-for="item in data.own_prs_without_reviewer"
              :key="`own-${item.pull_request.repo}-${item.pull_request.pr}`"
            >
              <template #title>
                <PrRef :pr="item.pull_request.pr" />
                {{ item.pull_request.title ?? 'no title recorded' }}
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
            v-if="data.proposed_rules.length"
            heading="Proposed rule"
            :count="data.proposed_rules.length"
          >
            <ActionRow
              v-for="item in data.proposed_rules"
              :key="`rule-${item.rule_id}`"
            >
              <template #title>
                {{ item.statement }}
              </template>
              <template #meta>
                {{ item.why_proposed ?? 'no reason recorded' }}
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
              :disabled="starting"
              @click="start"
            >
              {{ starting ? 'Reading the pull request…' : 'Understand it first' }}
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
              In progress · {{ data.in_progress.length }}
            </div>
            <p
              v-if="data.in_progress.length === 0"
              class="rd-meta help"
            >
              No session is running.
            </p>
            <div
              v-for="item in data.in_progress"
              :key="item.review_id"
              class="progress"
            >
              <NuxtLink
                class="progress-title"
                :to="`/reviews/${item.review_id}`"
              >
                <PrRef :pr="item.pull_request.pr" />
                {{ item.pull_request.title ?? 'no title recorded' }}
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
              <template v-if="data.handled_without_you.window_days === 7">
                this week
              </template>
              <template v-else>
                in {{ count(data.handled_without_you.window_days, 'day') }}
              </template>
            </div>
            <p
              v-for="line in handledLines(data)"
              :key="line"
              class="rd-meta line"
            >
              {{ line }}
            </p>
            <p
              v-if="handledLines(data).length === 0"
              class="rd-meta line"
            >
              Nothing has reported in yet.
            </p>
          </DeskCard>
        </aside>
      </template>

      <!--
        A database with nothing in it. It reads as new rather than as broken, and it says
        the one thing that fills it: a session recording a review.
      -->
      <template #empty>
        <DeskCard class="fresh">
          <h1>Nothing here yet</h1>
          <p class="rd-note rd-measure">
            This database holds no reviews. A review turns up here once an agent session has
            recorded one — it writes the brief and the findings through
            <code class="rd-mono">review-desk</code>, and what is waiting on you appears in
            this list.
          </p>
          <p class="rd-note rd-measure">
            To start one yourself, paste a pull request link below. Review Desk reads the
            pull request through your own <code class="rd-mono">gh</code> login, records the
            review at the problem brief, and writes nothing to GitHub.
          </p>
          <div class="fresh-start">
            <label
              class="rd-label"
              for="pr-link-empty"
            >Start a review</label>
            <input
              id="pr-link-empty"
              v-model="link"
              class="rd-field"
              type="text"
              inputmode="url"
              placeholder="Paste a pull request link"
              @keyup.enter="start"
            >
            <button
              type="button"
              class="rd-btn rd-btn--ink"
              :disabled="starting"
              @click="start"
            >
              {{ starting ? 'Reading the pull request…' : 'Understand it first' }}
            </button>
            <DeskNotice
              v-if="notice"
              :tone="notice.tone"
            >
              {{ notice.text }}
            </DeskNotice>
          </div>
        </DeskCard>
      </template>
    </DeskState>
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

.fresh {
  flex: 1 1 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 24px;
}

.fresh-start {
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-width: 28rem;
  padding-top: 8px;
}
</style>
