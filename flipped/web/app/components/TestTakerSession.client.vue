<script setup lang="ts">
const props = defineProps<{ sessionId: string }>();
const store = useTestTakerSessionStore();
useSessionSocket("test_taker", props.sessionId);
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
          Test taker
        </p>
        <h1 class="text-3xl font-bold">
          {{ store.snapshot.status.replaceAll("_", " ") }}
        </h1>
      </div>
      <div
        class="badge"
        :class="
          store.snapshot.examinerConnected ? 'badge-success' : 'badge-ghost'
        "
      >
        {{
          store.snapshot.examinerConnected
            ? "Examiner connected"
            : "Waiting for examiner"
        }}
      </div>
    </section>
    <SessionCard :card="store.snapshot?.currentCard" :show-back="false" />
  </div>
</template>
