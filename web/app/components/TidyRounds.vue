<script setup lang="ts">
import type { DuplicateRoundsPage, ExtraRound } from '~/types/desk'

/**
 * **The one-off tidy for the duplicate unposted rounds already on the record.** DEV-962's
 * fourth item, and its own words: it "shows each pull request's extra unposted rounds and
 * folds them into the newest (or dismisses them); nothing is deleted without confirmation".
 *
 * ```
 *  DUPLICATE ROUNDS · 2 pull requests, 4 extra rounds
 *  acme/portal #412 — Retry failed webhook deliveries        newest: round 4 · eeb41d8
 *    Round 1 · 3f9c2ab · holds a brief and 6 findings   [ Dismiss it ]
 *    Round 2 · 9d02e71 · holds nothing                  [ Fold it into round 4 ]
 * ```
 *
 * **It shows, and the developer decides.** Every row says what the round holds before
 * anything is offered, because the alternative is asking somebody to confirm a removal they
 * cannot see the size of. The fold asks twice, for `RemoveReview`'s reason: it cannot be
 * undone, and the second press names the round rather than saying "Fold".
 *
 * **Two acts, and the row's own `act` decides which is offered.** The server works it out
 * from the question a delete asks — a round holding a brief, a finding or a draft is kept
 * and hidden — so this page draws what it was told rather than judging it again. A page
 * that judged would eventually offer a fold the API refuses.
 *
 * **There is no command line path to either act.** A session that could reach them could
 * fold away a round of its own review, which is the rule a dismissal, a restore and a
 * delete already follow.
 *
 * **Nothing new can turn up here.** A pull request cannot grow a second unposted round any
 * more: a new commit on an unposted round moves it. This is for the record as it stood when
 * that rule changed.
 */
const props = defineProps<{ page: DuplicateRoundsPage }>()

const desk = useReviewDesk()

/** Which round is being confirmed, by review id, and which act. */
const asking = ref<{ id: number, act: 'fold' | 'dismiss' } | null>(null)

/** A write is in flight, so nothing else may be pressed. */
const working = ref(false)

/** What the desk said back, in the tone its code earns. */
const said = ref<DeskNotice | null>(null)

/** The heading's own count, served rather than added up here. */
const summary = computed(() => {
  const pulls = count(props.page.pull_requests_affected, 'pull request')
  const rounds = count(props.page.extra_rounds, 'extra round')
  return `${pulls}, ${rounds}`
})

/** "Fold it into round 4" — the act named with the round it folds into. */
function foldLabel(into: number): string {
  return `Fold it into round ${into}`
}

async function fold(id: number) {
  working.value = true
  said.value = noticeFor(await desk.foldRound(id))
  working.value = false
  asking.value = null
}

async function dismiss(id: number, reason: string) {
  working.value = true
  said.value = noticeFor(await desk.dismissReview(id, reason))
  working.value = false
  /* The box closes only on success, so a refused reason is still there to correct. */
  if (said.value.tone === 'plain') asking.value = null
}

/** Whether this row is the one being confirmed, for the act named. */
function confirming(extra: ExtraRound, act: 'fold' | 'dismiss'): boolean {
  return asking.value?.id === extra.review_id && asking.value.act === act
}
</script>

<template>
  <DeskCard
    id="tidy-rounds"
    as="section"
    tone="well"
    pad="section"
    class="tidy"
  >
    <h2 class="rd-rubric">
      Duplicate rounds · {{ summary }}
    </h2>

    <p class="rd-note rd-measure">
      <strong>These are rounds that were opened and never posted.</strong>
      A new commit on an unposted round now moves that round instead of opening another, so
      nothing new turns up here — these are the pull requests the old rule recorded twice.
      Folding one records its head commit on the newest round and removes the empty row;
      a round holding a brief, a finding or a draft is dismissed instead, which keeps
      everything on it.
    </p>

    <DeskNotice
      v-if="said"
      :tone="said.tone"
    >
      {{ said.text }}
    </DeskNotice>

    <div
      v-for="pull in page.pull_requests"
      :key="`${pull.repo}#${pull.pr}`"
      class="pull"
    >
      <div class="pull-head">
        <h3 class="rd-display-card">
          {{ pull.repo }} #{{ pull.pr }}
        </h3>
        <p
          v-if="pull.title"
          class="rd-note title"
        >
          {{ pull.title }}
        </p>
        <p class="rd-meta">
          Newest: round {{ pull.newest.round }} ·
          <span class="rd-mono">{{ commitLabel(pull.newest.head_sha) }}</span>
        </p>
      </div>

      <ul class="rounds">
        <li
          v-for="extra in pull.extras"
          :key="extra.review_id"
          class="round"
          :data-review="extra.review_id"
          :data-act="extra.act"
        >
          <div class="round-what">
            <span class="round-name">Round {{ extra.round }}</span>
            <span class="rd-meta rd-mono">{{ commitLabel(extra.head_sha) }}</span>
            <!--
              **What it holds, in the words the refusal uses.** The server composes this
              sentence so that the row and the refusal cannot word the same fact
              differently, and it is what makes the choice between the two acts obvious.
            -->
            <span class="rd-meta">holds {{ extra.holds }}</span>
            <NuxtLink
              class="rd-meta read-it"
              :to="`/reviews/${extra.review_id}/understand`"
            >
              Read it
            </NuxtLink>
          </div>

          <!--
            **One slot, three states, each branch naming the state it is for** — the shape
            `RemoveReview` settled on after two states sharing a `v-else` let a keypress
            land on the wrong act.
          -->
          <div
            v-if="asking === null"
            class="controls"
          >
            <DeskButton
              v-if="extra.act === 'fold'"
              kind="secondary"
              @click="asking = { id: extra.review_id, act: 'fold' }; said = null"
            >
              {{ foldLabel(pull.newest.round) }}
            </DeskButton>
            <DeskButton
              v-else
              kind="secondary"
              @click="asking = { id: extra.review_id, act: 'dismiss' }; said = null"
            >
              Dismiss it
            </DeskButton>
          </div>

          <!--
            The fold asks twice, and the second press names the round: it cannot be undone,
            and "Fold" on its own is a word where `round 2 of acme/portal #412` is a thing.
          -->
          <div
            v-else-if="confirming(extra, 'fold')"
            class="controls"
          >
            <p class="rd-note rd-measure">
              Round {{ extra.round }} of {{ pull.repo }} #{{ pull.pr }} holds
              {{ extra.holds }}. Its head commit
              <span class="rd-mono">{{ commitLabel(extra.head_sha) }}</span>
              goes onto round {{ pull.newest.round }}'s record and the row is removed. This
              cannot be undone.
            </p>
            <DeskButton
              kind="ink"
              :working="working"
              working-label="Folding it"
              @click="fold(extra.review_id)"
            >
              Fold round {{ extra.round }} into round {{ pull.newest.round }}
            </DeskButton>
            <DeskButton
              kind="quiet"
              @click="asking = null"
            >
              Keep it
            </DeskButton>
          </div>

          <ReasonPrompt
            v-else-if="confirming(extra, 'dismiss')"
            :id="`dismiss-round-${extra.review_id}`"
            label="Why are you dismissing this round?"
            action="Dismiss it"
            :working="working"
            @confirm="reason => dismiss(extra.review_id, reason)"
            @cancel="asking = null"
          />
        </li>
      </ul>
    </div>
  </DeskCard>
</template>

<style scoped>
.tidy {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.pull {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--rd-rule);
}

.pull-head {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.title {
  margin: 0;
}

.rounds {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.round {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  background: var(--rd-surface);
  border: 1px solid var(--rd-rule);
  border-radius: var(--rd-radius-sm);
}

.round-what {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 10px;
}

.round-name {
  font-weight: 500;
}

.read-it {
  color: var(--rd-accent);
}

.controls {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
</style>
