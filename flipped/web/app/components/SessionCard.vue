<script setup lang="ts">
import type { CardFront, CardFull } from "#shared/session";
defineProps<{ card?: CardFront | CardFull; showBack: boolean }>();
</script>

<template>
  <article
    class="card min-h-80 border border-base-300 bg-base-100 shadow-lg"
    aria-live="polite"
  >
    <div class="card-body justify-center gap-8 text-center">
      <template v-if="card">
        <div>
          <p
            class="mb-2 text-xs font-semibold uppercase tracking-widest text-primary"
          >
            Prompt
          </p>
          <div class="prose mx-auto max-w-none text-2xl" v-text="card.front" />
        </div>
        <div
          v-if="showBack && 'back' in card"
          class="border-t border-base-300 pt-8"
        >
          <p
            class="mb-2 text-xs font-semibold uppercase tracking-widest text-secondary"
          >
            Answer
          </p>
          <div class="prose mx-auto max-w-none text-xl" v-text="card.back" />
        </div>
        <p class="text-sm text-base-content/60">
          Card {{ card.position }} of {{ card.total }}
        </p>
      </template>
      <p v-else class="text-lg text-base-content/70">
        Waiting for the examiner to start.
      </p>
    </div>
  </article>
</template>
