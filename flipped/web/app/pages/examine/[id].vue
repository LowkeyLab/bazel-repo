<script setup lang="ts">
import { v7 as uuidv7 } from "uuid";
const route = useRoute();
const sessionId = computed(() => String(route.params.id));
const ready = ref(false);
const redeeming = ref(true);
const error = ref<string>();

onMounted(async () => {
  const parameters = new URLSearchParams(window.location.hash.slice(1));
  const invitation = parameters.get("invite");
  if (!invitation) {
    ready.value = true;
    redeeming.value = false;
    return;
  }
  window.history.replaceState(null, "", window.location.pathname);
  try {
    await $fetch(
      `/api/sessions/${encodeURIComponent(sessionId.value)}/invitation/redeem`,
      {
        method: "POST",
        body: { invitation, redemptionId: uuidv7() },
      },
    );
    ready.value = true;
  } catch (cause) {
    error.value =
      cause instanceof Error
        ? cause.message
        : "Invitation could not be redeemed";
  } finally {
    redeeming.value = false;
  }
});
</script>

<template>
  <div v-if="redeeming" class="card bg-base-100">
    <div class="card-body items-center">
      <span class="loading loading-spinner loading-lg" />
      <p>Redeeming examiner invitation…</p>
    </div>
  </div>
  <div v-else-if="error" class="alert alert-error" role="alert">
    {{ error }}
  </div>
  <ExaminerSession v-else-if="ready" :session-id="sessionId" />
</template>
