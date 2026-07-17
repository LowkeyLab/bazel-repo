<script setup lang="ts">
const route = useRoute();
const sessionId = computed(() => String(route.params.id));
</script>

<template>
  <TestTakerSession :session-id="sessionId" />
</template>
