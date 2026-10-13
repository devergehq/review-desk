<script setup lang="ts">
/**
 * **What has happened to this round's head commit, as a notice.** DEV-962.
 *
 * Since a new commit on an unposted round moves that round instead of opening the next one,
 * everything a review page draws may have been written against a commit the pull request has
 * moved past. This is the sentence that says so — and the quieter one that says a push
 * brought the base branch in and changed nothing, which matters for the opposite reason: a
 * reader who watched the pull request move is owed the reason nothing here did.
 *
 * **A component rather than two lines in a page**, because Understand draws it in three
 * branches and the code review's timeline says the same thing: the words live once, in
 * `utils/rounds`, and the drawing lives once here.
 */
import type { ReviewDetail } from '~/types/desk'

import { baseOnlyLine, nothingMoved, updatedLine } from '~/utils/rounds'

const props = defineProps<{ detail: ReviewDetail }>()

/**
 * `head_updates` is absent from a response written before DEV-962 — it is not in the
 * contract's `required` list, for the reason `rounds` is not — and `nothingMoved` is the one
 * reading of that absence.
 */
const updates = computed(() => props.detail.head_updates ?? nothingMoved())

const moved = computed(() => updatedLine(updates.value))
const baseOnly = computed(() => baseOnlyLine(updates.value))
</script>

<template>
  <!--
    **The move first, and the base merges only when nothing moved.** Both at once reads as
    two things to deal with where there is one: a round whose head moved is a round the
    session is re-reading, and that is the sentence that matters. The timeline on the code
    review draws every line, which is where a reader goes for the whole history.
  -->
  <DeskNotice
    v-if="moved"
    tone="problem"
  >
    {{ moved }}. The review session re-reads the change and updates what is below.
  </DeskNotice>
  <DeskNotice
    v-else-if="baseOnly"
    tone="plain"
  >
    {{ baseOnly }}. The change itself is the one below.
  </DeskNotice>
</template>
