<script setup lang="ts">
import type { ConnectionStatus } from "#shared/session";
defineProps<{ status: ConnectionStatus }>();
</script>

<template>
  <div v-if="status !== 'connected'" class="alert alert-warning" role="status">
    <span>{{
      status === "reconnecting"
        ? "Connection lost. Restoring the authoritative session…"
        : "Connecting to the session…"
    }}</span>
  </div>
</template>
