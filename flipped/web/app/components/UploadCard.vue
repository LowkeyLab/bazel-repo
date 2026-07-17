<script setup lang="ts">
const emit = defineEmits<{ selected: [file: File] }>();
const dragging = ref(false);
function select(files: FileList | null) {
  const file = files?.item(0);
  if (file) emit("selected", file);
}
function drop(event: DragEvent) {
  dragging.value = false;
  select(event.dataTransfer?.files ?? null);
}
</script>

<template>
  <label
    class="card cursor-pointer border-2 border-dashed bg-base-100 transition-colors"
    :class="dragging ? 'border-primary bg-primary/5' : 'border-base-300'"
    @dragenter.prevent="dragging = true"
    @dragover.prevent
    @dragleave.prevent="dragging = false"
    @drop.prevent="drop"
  >
    <div class="card-body items-center py-14 text-center">
      <span class="text-5xl" aria-hidden="true">⇧</span>
      <h2 class="card-title">Choose an Anki deck</h2>
      <p class="max-w-lg text-base-content/70">
        Drop a basic <code>.apkg</code> deck here or choose a file. Text-only
        front/back cards are supported.
      </p>
      <span class="btn btn-primary mt-3">Select deck</span>
      <input
        class="sr-only"
        type="file"
        accept=".apkg,application/octet-stream"
        @change="select(($event.target as HTMLInputElement).files)"
      />
    </div>
  </label>
</template>
