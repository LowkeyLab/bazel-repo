<script setup lang="ts">
import { computed, ref } from "vue";
import type { SessionStatus } from "#shared/session";
const props = defineProps<{ status?: SessionStatus; disabled: boolean }>();
const emit = defineEmits<{ start: []; advance: []; end: [] }>();
const confirmEnd = ref(false);
const canStart = computed(() => props.status === "ready");
const canAdvance = computed(() => props.status === "in_progress");
</script>

<template>
  <div class="flex flex-wrap gap-3" aria-label="Examiner controls">
    <button
      class="btn btn-primary"
      type="button"
      :disabled="disabled || !canStart"
      @click="emit('start')"
    >
      Start examination
    </button>
    <button
      class="btn btn-secondary"
      type="button"
      :disabled="disabled || !canAdvance"
      @click="emit('advance')"
    >
      Next card
    </button>
    <button
      class="btn btn-error btn-outline"
      type="button"
      :disabled="
        disabled || !status || ['terminated', 'expired'].includes(status)
      "
      @click="confirmEnd = true"
    >
      End session
    </button>
  </div>
  <dialog class="modal" :open="confirmEnd">
    <div class="modal-box">
      <h3 class="text-lg font-bold">End this examination?</h3>
      <p class="py-4">
        Both participants will be moved to the terminal session view.
      </p>
      <div class="modal-action">
        <button class="btn" type="button" @click="confirmEnd = false">
          Cancel
        </button>
        <button
          class="btn btn-error"
          type="button"
          @click="
            confirmEnd = false;
            emit('end');
          "
        >
          End session
        </button>
      </div>
    </div>
  </dialog>
</template>
