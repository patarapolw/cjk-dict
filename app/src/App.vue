<template>
  <div class="layout">
    <header class="header">
      <RouterLink to="/dictionary" custom v-slot="{ isActive, navigate }">
        <Button
          :severity="isActive ? undefined : 'secondary'"
          text
          size="small"
          aria-label="Dictionary"
          @click="navigate"
        >
          <SearchIcon />
        </Button>
      </RouterLink>
      <span class="header-title">{{ $route.name }}</span>
      <Select
        v-model="settingsState.state.lang"
        :options="langSelectOptions"
        option-label="label"
        option-value="value"
        size="small"
      />
      <RouterLink to="/settings" custom v-slot="{ isActive, navigate }">
        <Button
          :severity="isActive ? undefined : 'secondary'"
          text
          size="small"
          aria-label="Settings"
          @click="navigate"
        >
          <CogIcon />
        </Button>
      </RouterLink>
    </header>
    <RouterView />
  </div>
  <!-- Background downloads: non-modal, the app stays usable meanwhile -->
  <aside
    v-if="toBeLoaded.length || loadErrors.length"
    class="downloads"
    :class="{ collapsed: downloadsCollapsed }"
    aria-live="polite"
  >
    <header class="downloads-header">
      <span v-if="toBeLoaded.length">
        Downloading tokenizer ({{ toBeLoaded.length }} left)
      </span>
      <span v-else>Tokenizer download failed</span>
      <Button
        :label="downloadsCollapsed ? 'Show' : 'Hide'"
        size="small"
        text
        @click="downloadsCollapsed = !downloadsCollapsed"
      />
      <Button
        v-if="!toBeLoaded.length"
        label="Dismiss"
        size="small"
        text
        @click="loadErrors = []"
      />
    </header>
    <div v-show="!downloadsCollapsed" class="downloads-body">
      <Downloading
        v-for="d in toBeLoaded"
        :key="d.filename"
        ref="loading"
        v-bind="d"
      />
      <p v-for="e in loadErrors" :key="e.filename" class="downloads-error">
        {{ e.filename }}: {{ e.error }} (will retry on next launch)
      </p>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { nextTick, onMounted, ref, useTemplateRef, watch } from "vue";

import Button from "primevue/button";
import Select from "primevue/select";

import CogIcon from "@primeicons/vue/cog";
import SearchIcon from "@primeicons/vue/search";

import Downloading from "./components/Downloading.vue";
import { loadErrors, toBeLoaded } from "./util/loading.ts";
import { settingsState } from "./util/settings.ts";
import { makeLinderaDownloadList } from "./util/tokenize";

// array of refs, or undefined
const elLoading = useTemplateRef("loading");

const downloadsCollapsed = ref(false);

const langSelectOptions = ref([
  { label: "Auto-detect", value: "" },
  { label: "ja-JP", value: "ja-JP" },
  { label: "zh-CN", value: "zh-CN" },
  { label: "ko-KR", value: "ko-KR" },
]);

onMounted(() => {
  loadAll();
});

// Process downloads one at a time in the background. A failure is recorded and skipped,
// so the queue never stalls.
let queueRunning = false;
async function runQueue() {
  if (queueRunning) return;
  queueRunning = true;
  try {
    while (toBeLoaded.value.length) {
      await nextTick();
      const current = toBeLoaded.value[0];
      const loader = elLoading.value?.[0];
      try {
        if (!loader) throw new Error("download component not mounted");
        await loader.start();
      } catch (e) {
        console.error(e);
        loadErrors.value = [
          ...loadErrors.value,
          { filename: current.filename, error: String(e) },
        ];
      }
      toBeLoaded.value = toBeLoaded.value.filter((d) => d !== current);
    }
  } finally {
    queueRunning = false;
  }
}

watch(toBeLoaded, () => {
  if (toBeLoaded.value.length) runQueue();
});

async function loadAll() {
  // lindera dictionaries, for tokenizing "~" search queries on the device.
  // Yomitan dictionaries are served by the cjk-dict worker; nothing to import locally.
  toBeLoaded.value = [
    ...toBeLoaded.value,
    ...(await makeLinderaDownloadList(["unidic", "ko-dic", "cc-cedict"])),
  ];
}
</script>

<style scoped>
.layout {
  height: 100vh;
  display: grid;
  grid-template-rows: auto 1fr;
  overflow: hidden;
}

.header {
  display: flex;
  flex-direction: row;
  align-items: center;
  height: 3rem;
  padding: 0.5rem;
  gap: 0.5rem;
}

.header-title {
  flex-grow: 1;
}

.downloads {
  position: fixed;
  right: 1rem;
  bottom: 1rem;
  z-index: 1000;
  width: min(26rem, calc(100vw - 2rem));
  max-height: 40vh;
  overflow: auto;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--p-content-border-color, #ccc);
  border-radius: 0.5rem;
  background: var(--p-content-background, #fff);
  color: var(--p-text-color, inherit);
  box-shadow: 0 4px 16px rgb(0 0 0 / 0.15);
  font-size: 0.875rem;
}

.downloads.collapsed {
  width: auto;
}

.downloads-header {
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

.downloads-header > span {
  flex-grow: 1;
}

.downloads-error {
  color: var(--p-red-500, #c00);
}
</style>
