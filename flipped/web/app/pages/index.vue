<script setup lang="ts">
import type { CreateSessionApiResponse } from "#shared/api";
const uploading = ref(false);
const error = ref<string>();
const created = ref<CreateSessionApiResponse>();
const testTaker = useTestTakerSessionStore();

async function upload(file: File) {
  error.value = undefined;
  if (!file.name.toLowerCase().endsWith(".apkg")) {
    error.value = "Choose an .apkg file.";
    return;
  }
  uploading.value = true;
  try {
    const response = await fetch("/api/sessions", {
      method: "POST",
      credentials: "same-origin",
      headers: {
        "content-type": "application/octet-stream",
        "x-package-extension": ".apkg",
        "x-declared-size": String(file.size),
      },
      body: file,
    });
    if (!response.ok)
      throw new Error(
        (await response.json().catch(() => undefined))?.statusMessage ??
          "Upload failed",
      );
    created.value = (await response.json()) as CreateSessionApiResponse;
    testTaker.applySnapshot(created.value.initialSnapshot);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : "Upload failed";
  } finally {
    uploading.value = false;
  }
}
</script>

<template>
  <div class="mx-auto max-w-3xl space-y-8">
    <section class="space-y-3 text-center">
      <div class="badge badge-primary badge-outline">
        Live flashcard examination
      </div>
      <h1 class="text-4xl font-black sm:text-5xl">
        Share the prompt. Keep the answer with the examiner.
      </h1>
      <p class="mx-auto max-w-2xl text-lg text-base-content/70">
        Upload a basic text-only Anki deck, invite an examiner, and run a
        synchronized session without exposing card backs to the test taker.
      </p>
    </section>
    <div v-if="error" class="alert alert-error" role="alert">{{ error }}</div>
    <div v-if="uploading" class="card bg-base-100">
      <div class="card-body items-center">
        <span class="loading loading-spinner loading-lg" />
        <p>Validating and importing the deck…</p>
      </div>
    </div>
    <template v-else-if="created">
      <InvitationPanel :path="created.invitationPath" />
      <NuxtLink
        class="btn btn-primary w-full"
        :to="`/session/${created.sessionId}`"
        >Enter test-taker session</NuxtLink
      >
    </template>
    <UploadCard v-else @selected="upload" />
    <div class="alert" role="note">
      <span
        >Supported: ordinary front/back text cards. Cloze notes, media, custom
        templates, scheduling data, and <code>.colpkg</code> are rejected.</span
      >
    </div>
  </div>
</template>
