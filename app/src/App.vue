<template>
  <div class="layout">
    <header class="header">
      <span class="header-title">CJK Dictionary</span>
      <nav class="header-nav" aria-label="Views">
        <RouterLink to="/dictionary">Words</RouterLink>
        <RouterLink to="/kanji">Characters</RouterLink>
      </nav>
      <select v-model="lang" aria-label="Search language">
        <option
          v-for="option in langSelectOptions"
          :key="option.value"
          :value="option.value"
        >
          {{ option.label }}
        </option>
      </select>
    </header>
    <RouterView />
  </div>
  <!-- Background downloads: non-modal, the app stays usable meanwhile -->
  <aside
    v-if="toBeLoaded.length || loadErrors.length || verifying"
    class="downloads"
    :class="{ collapsed: downloadsCollapsed }"
    aria-live="polite"
  >
    <header class="downloads-header">
      <span v-if="toBeLoaded.length">
        Downloading ({{ toBeLoaded.length }} left)
      </span>
      <span v-else-if="verifying">Verifying dictionaries</span>
      <span v-else>Download failed</span>
      <button
        type="button"
        class="text-button"
        @click="downloadsCollapsed = !downloadsCollapsed"
      >
        {{ downloadsCollapsed ? "Show" : "Hide" }}
      </button>
      <button
        v-if="!toBeLoaded.length && !verifying"
        type="button"
        class="text-button"
        @click="loadErrors = []"
      >
        Dismiss
      </button>
    </header>
    <div v-show="!downloadsCollapsed" class="downloads-body">
      <div v-if="verifying">
        <p>
          Verifying {{ verifying.file }}
          <span style="float: right">({{ verifyingPercent }}%)</span>
        </p>
        <div
          class="progress"
          role="progressbar"
          :aria-label="`Verifying ${verifying.file}`"
          aria-valuemin="0"
          aria-valuemax="100"
          :aria-valuenow="verifyingPercent"
        >
          <div
            class="progress-value"
            :style="{ width: `${verifyingPercent}%` }"
          />
        </div>
      </div>
      <Downloading
        v-for="d in toBeLoaded"
        :key="d.filename"
        :ref="(el) => setLoader(d.filename, el)"
        v-bind="d"
      />
      <p v-for="e in loadErrors" :key="e.filename" class="downloads-error">
        {{ e.filename }}: {{ e.error }} (will retry on next launch)
      </p>
    </div>
  </aside>
</template>

<script setup lang="ts">
import {
  type ComponentPublicInstance,
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import Downloading from "./components/Downloading.vue";
import {
  DownloadUrlProgress,
  DownloadingProps,
  SyncStatus,
  VerifyProgress,
  dictsVersion,
  loadErrors,
  syncStatus,
  toBeLoaded,
  verifying,
} from "./util/loading.ts";
import { lang } from "./util/settings.ts";
import { makeLinderaDownloadList } from "./util/tokenize";

// Download components by filename. Not a `ref` in `v-for`: that array's order
// isn't guaranteed to match `toBeLoaded`, so [0] could be another file's component.
const loaders = new Map<string, InstanceType<typeof Downloading>>();
function setLoader(
  filename: string,
  el: Element | ComponentPublicInstance | null,
) {
  if (el) loaders.set(filename, el as InstanceType<typeof Downloading>);
  else loaders.delete(filename);
}

const downloadsCollapsed = ref(false);

const langSelectOptions = ref([
  { label: "Auto-detect", value: "" },
  { label: "ja-JP", value: "ja-JP" },
  { label: "zh-CN", value: "zh-CN" },
  { label: "ko-KR", value: "ko-KR" },
]);

const verifyingPercent = computed(() => {
  const v = verifying.value;
  return v && v.size ? Math.floor((100 * v.hashed) / v.size) : 0;
});

// Checksumming the library takes minutes on a phone: show which file it's at.
// `dict_plan` and `dict_commit` in flight; an event arriving after one returned is stale.
const checksumming = ref(0);
async function checksummed<T>(run: () => Promise<T>) {
  checksumming.value++;
  try {
    return await run();
  } finally {
    checksumming.value--;
    verifying.value = null;
  }
}

// The whole sync (gigabytes) takes a while: on Android, report it to SyncService, whose
// notification keeps the app running in the background and with the screen off.
const download = ref<DownloadUrlProgress | null>(null);
const status = computed(() =>
  syncStatus({
    checking: checksumming.value > 0,
    verifying: verifying.value,
    download: download.value,
    queue: toBeLoaded.value,
  }),
);
watch(status, (s) => sendSyncStatus(s), { deep: true });

/**
 * At most one progress update a second: Android drops faster notification updates.
 * Updates in between are dropped rather than sent later: timers in a page that's in the
 * background are throttled to as little as once a minute.
 */
const SYNC_STATUS_INTERVAL_MS = 1000;
let lastSent: SyncStatus | null = null;
let lastSentAt = 0;
function sendSyncStatus(s: SyncStatus | null) {
  if (!s && !lastSent) return;
  // a new step or the end at once (MainActivity delays stopping, see setSyncStatus)
  const now = Date.now();
  if (
    s &&
    lastSent &&
    s.text === lastSent.text &&
    now - lastSentAt < SYNC_STATUS_INTERVAL_MS
  ) {
    return;
  }
  lastSent = s && { ...s };
  lastSentAt = now;
  invoke("set_sync_status", {
    text: s?.text ?? null,
    percent: s?.percent ?? null,
  }).catch((e) => console.error(e));
}

const unlisteners: UnlistenFn[] = [];
onBeforeUnmount(() => {
  unlisteners.forEach((u) => u());
});

onMounted(async () => {
  unlisteners.push(
    await listen<VerifyProgress>("dict-verify-progress", ({ payload }) => {
      if (checksumming.value) verifying.value = payload;
    }),
    await listen<DownloadUrlProgress>(
      "download-url-progress",
      ({ payload }) => {
        download.value = payload;
      },
    ),
  );
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
      const loader = loaders.get(current.filename);
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

      // last file of a dictionary sync: put everything in place at once
      if (current.dict && !toBeLoaded.value.some((d) => d.dict)) {
        await commitDicts();
      }
    }
  } finally {
    queueRunning = false;
  }
}

watch(toBeLoaded, () => {
  if (toBeLoaded.value.length) runQueue();
});

async function commitDicts() {
  try {
    await checksummed(() => invoke("dict_commit"));
    dictsVersion.value++;
  } catch (e) {
    console.error(e);
    loadErrors.value = [
      ...loadErrors.value,
      { filename: "dictionaries", error: String(e) },
    ];
  }
}

async function loadAll() {
  // Dictionaries: download what changed in the library (zips + search.db), then commit.
  try {
    const plan = await checksummed(() =>
      invoke<{ filename: string; url: string }[]>("dict_plan"),
    );
    toBeLoaded.value = [
      ...toBeLoaded.value,
      ...plan.map(
        (f): DownloadingProps => ({
          filename: f.filename,
          url: f.url,
          dict: true,
        }),
      ),
    ];
    if (!plan.length) dictsVersion.value++;
  } catch (e) {
    console.error(e);
    loadErrors.value = [
      ...loadErrors.value,
      { filename: "dictionaries", error: String(e) },
    ];
  }

  // lindera dictionaries, for tokenizing "~" search queries on the device.
  toBeLoaded.value = [
    ...toBeLoaded.value,
    ...(await makeLinderaDownloadList(["unidic", "ko-dic", "cc-cedict"])),
  ];
}
</script>

<style scoped>
.layout {
  height: 100vh;
  height: 100dvh;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  /* Not wider than the window, whatever the content's minimum width: a narrow window,
     such as an Android pop-up (floating) window, would otherwise cut off the right. */
  grid-template-columns: minmax(0, 1fr);
  /* and long words, e.g. in a search hint, wrap rather than stick out */
  overflow-wrap: break-word;
  overflow: hidden;
}

.header {
  display: flex;
  flex-direction: row;
  /* in a narrow window, the language select goes on a line of its own */
  flex-wrap: wrap;
  box-sizing: border-box;
  min-height: 4rem;
  padding: 0.5rem;
  gap: 0.5rem 0.75rem;
}

.header-title {
  /* takes up what's left of the line, without pushing the rest onto the next one */
  flex: 1 1 0;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.header-nav {
  display: flex;
  gap: 0.75rem;
}

.header-nav a {
  color: var(--app-link);
  text-decoration: none;
}

.header-nav a.router-link-active {
  color: var(--app-text);
  font-weight: 600;
}

.header select {
  min-width: 0;
  max-width: 100%;
  font: inherit;
  padding: 0.25rem 0.5rem;
}

/* too narrow to be anything but cut off */
@media (max-width: 24rem) {
  .header-title {
    display: none;
  }
}

.text-button {
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
  padding: 0.25rem 0.5rem;
}

.text-button:hover {
  text-decoration: underline;
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
  border: 1px solid var(--app-border);
  border-radius: 0.5rem;
  background: var(--app-surface);
  color: var(--app-text);
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
  color: var(--app-error);
}

/* as in Downloading.vue */
.progress {
  height: 0.5rem;
  overflow: hidden;
  border-radius: 999px;
  background: var(--app-progress-track);
}

.progress-value {
  height: 100%;
  background: var(--app-focus);
  transition: width 150ms ease;
}
/* A short window, such as an Android pop-up (floating) window: the headers would leave
   little room for results, so scroll the whole page rather than only the results.
   Keep in step with the media queries in the views. */
@media (max-height: 28rem) {
  .layout {
    display: block;
    overflow-y: auto;
  }

  .header {
    min-height: 3rem;
    padding-block: 0.25rem;
  }
}
</style>
