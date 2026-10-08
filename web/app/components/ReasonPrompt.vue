<script setup lang="ts">
/**
 * Asks why before doing something the desk will not undo. The API refuses a dismissal or
 * a retirement without a reason, because a rule dismissed silently is proposed again next
 * week with no memory of having been told no — so the screen has to collect one.
 */
defineProps<{ id: string, label: string, action: string }>()
const emit = defineEmits<{ confirm: [reason: string], cancel: [] }>()

const reason = ref('')
const field = useTemplateRef<HTMLInputElement>('field')
const problem = ref('')

onMounted(() => field.value?.focus())

function confirm() {
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
        @keyup.enter="confirm"
        @keyup.escape="emit('cancel')"
      >
      <button
        type="button"
        class="rd-btn rd-btn--primary"
        @click="confirm"
      >
        {{ action }}
      </button>
      <button
        type="button"
        class="rd-btn"
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
