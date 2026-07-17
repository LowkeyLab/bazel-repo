<script setup lang="ts">
const props = defineProps<{ sessionId: string }>();
const store = useExaminerSessionStore();
const commands = useSessionSocket("examiner", props.sessionId);
</script>

<template>
  <div class="space-y-6">
    <ConnectionBanner :status="store.connection" />
    <div v-if="store.errorCode" class="alert alert-error" role="alert">
      {{ store.errorCode }}
    </div>
    <section
      v-if="store.snapshot"
      class="flex items-center justify-between gap-4"
    >
      <div>
        <p class="text-sm uppercase tracking-wider text-base-content/60">
          Examiner
        </p>
        <h1 class="text-3xl font-bold">
          {{ store.snapshot.status.replaceAll("_", " ") }}
        </h1>
      </div>
      <div
        class="badge"
        :class="
          store.snapshot.testTakerConnected ? 'badge-success' : 'badge-ghost'
        "
      >
        {{
          store.snapshot.testTakerConnected
            ? "Test taker connected"
            : "Waiting for test taker"
        }}
      </div>
    </section>
    <SessionCard :card="store.snapshot?.currentCard" :show-back="true" />
    <ExaminerControls
      :status="store.snapshot?.status"
      :disabled="
        store.connection !== 'connected' || Boolean(store.pendingCommandId)
      "
      @start="commands.start"
      @advance="commands.advance"
      @end="commands.end"
    />
  </div>
</template>
