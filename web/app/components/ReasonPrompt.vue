<script setup lang="ts">
/**
 * Asks why before doing something the desk will not undo. The API refuses a dismissal or
 * a retirement without a reason, because a rule dismissed silently is proposed again next
 * week with no memory of having been told no — so the screen has to collect one.
 */
/*
 * `working` is the write being in flight, and it is what stops a second press. Without it
 * two presses — or two Enters, which is easier — send the act twice, and on an act the
 * record refuses to repeat the second answer is a refusal shown over a first that
 * succeeded. The same split `DeskButton` makes: the pointer's states are CSS, and a write
 * in flight is a fact about the record and so a prop.
 */
withDefaults(
  defineProps<{ id: string, label: string, action: string, working?: boolean }>(),
  { working: false },
)
const emit = defineEmits<{ confirm: [reason: string], cancel: [] }>()

const reason = ref('')
const field = useTemplateRef<HTMLInputElement>('field')
const problem = ref('')

onMounted(() => field.value?.focus())

function confirm() {
  /*
   * **Cleared first, because the last complaint is about the last press.** It was set on
   * a blank confirm and never cleared, so "A reason is required." survived the developer
   * typing one — and on a box that stays open when the desk itself refuses, the card then
   * showed that line underneath the real refusal: two contradictory messages about one
   * field.
   */
  problem.value = ''
  if (reason.value.trim().length === 0) {
    problem.value = 'A reason is required.'
    field.value?.focus()
    return
  }
  emit('confirm', reason.value)
}
</script>

<template>
  <div class="prompt">
    <label
      class="rd-meta"
      :for="id"
    >{{ label }}</label>
    <div class="line">
      <input
        :id="id"
        ref="field"
        v-model="reason"
        class="rd-field"
        type="text"
        :aria-describedby="problem ? `${id}-problem` : undefined"
        :disabled="working"
        @keyup.enter="confirm"
        @keyup.escape="emit('cancel')"
      >
      <button
        type="button"
        class="rd-btn rd-btn--primary"
        :disabled="working"
        @click="confirm"
      >
        {{ action }}
      </button>
      <button
        type="button"
        class="rd-btn"
        :disabled="working"
        @click="emit('cancel')"
      >
        Cancel
      </button>
    </div>
    <DeskNotice
      v-if="problem"
      :id="`${id}-problem`"
      tone="problem"
    >
      {{ problem }}
    </DeskNotice>
  </div>
</template>

<style scoped>
.prompt {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1 1 100%;
}

.line {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.line .rd-field {
  flex: 1 1 220px;
  width: auto;
}
</style>
