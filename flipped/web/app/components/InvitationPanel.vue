<script setup lang="ts">
const props = defineProps<{ path: string }>();
const copied = ref(false);
async function copyInvitation() {
  await navigator.clipboard.writeText(
    new URL(props.path, window.location.origin).href,
  );
  copied.value = true;
  window.setTimeout(() => {
    copied.value = false;
  }, 2_000);
}
</script>

<template>
  <section class="card border border-primary/20 bg-primary/5">
    <div class="card-body">
      <h2 class="card-title">Invite the examiner</h2>
      <p>
        Send this one-time link to the examiner. It expires with this session.
      </p>
      <button
        class="btn btn-primary w-fit"
        type="button"
        @click="copyInvitation"
      >
        {{ copied ? "Copied" : "Copy examiner link" }}
      </button>
    </div>
  </section>
</template>
