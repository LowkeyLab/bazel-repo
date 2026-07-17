<template>
  <div class="min-h-screen bg-base-200 text-base-content">
    <header class="border-b border-base-300 bg-base-100">
      <div class="navbar mx-auto max-w-6xl px-4">
        <NuxtLink to="/" class="btn btn-ghost text-xl font-bold"
          >Flipped</NuxtLink
        >
        <span class="ml-auto text-sm text-base-content/70"
          >Examiner-led flashcards</span
        >
      </div>
    </header>
    <main class="mx-auto max-w-6xl px-4 py-10">
      <NuxtPage />
    </main>
  </div>
</template>
