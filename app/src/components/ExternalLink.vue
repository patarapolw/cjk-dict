<template>
  <!-- opens in the system's browser, not in the app's webview -->
  <a :href="url" class="external-link" @click.prevent="open"
    ><slot />&#x2009;<span aria-hidden="true">↗</span></a
  >
</template>

<script setup lang="ts">
import { openUrl } from "@tauri-apps/plugin-opener";

const props = defineProps<{ url: string }>();

function open() {
  openUrl(props.url).catch((e) => console.warn(e));
}
</script>

<style scoped>
.external-link {
  color: var(--app-link);
}
</style>
