<template>
  <main class="container">
    <header>
      <div class="search-group">
        <input
          class="search-input"
          inputmode="search"
          name="term"
          v-model="q"
          :placeholder="searchReading ? 'Term' : ''"
          autocapitalize="none"
          autocorrect="off"
          autocomplete="off"
          spellcheck="false"
          @input="onTermInput"
        />
        <div class="search-modes" role="group" aria-label="Search mode">
          <button
            type="button"
            :aria-pressed="qState === 'fts'"
            :disabled="tokenizerPending"
            :title="
              tokenizerPending
                ? '~ (token search) is available once the tokenizer download finishes'
                : undefined
            "
            @click="toggleQueryState('fts')"
          >
            ~
          </button>
          <button
            type="button"
            :aria-pressed="qState === 'exact'"
            @click="toggleQueryState('exact')"
          >
            =
          </button>
        </div>
      </div>
      <input
        v-if="searchReading"
        class="search-input"
        inputmode="search"
        name="reading"
        v-model="reading"
        placeholder="Reading"
        autocapitalize="none"
        autocorrect="off"
        autocomplete="off"
        spellcheck="false"
        @input="onTermInput"
      />
      <nav>
        <label v-if="lang !== 'ko-KR'">
          <input type="checkbox" name="reading" v-model="searchReading" />
          <span>Search reading</span>
        </label>

        <label v-if="['', 'ja-JP'].includes(lang)">
          <input type="checkbox" name="autokana" v-model="autoKana" />
          <span>Auto-convert to Kana</span>
        </label>

        <label>
          <input type="checkbox" name="clipboard" v-model="clipboardMonitor" />
          <span>Clipboard monitor</span>
        </label>
      </nav>
    </header>
    <div class="scroller" ref="scroller" @scroll.passive="onScroll">
      <section v-for="it in items" :key="it.id" class="accordion-item">
        <button
          type="button"
          class="accordion-header"
          :aria-expanded="openedItems.includes(it.id)"
          :aria-label="it.entries[0]?.term || it.term"
          @click="toggleItem(it.id)"
        >
          <span v-if="!openedItems.includes(it.id)">
            {{ it.entries[0]?.term || it.term }}
          </span>
        </button>
        <div v-if="openedItems.includes(it.id)" class="accordion-content">
          <Term
            v-for="(ent, j) in it.entries"
            :key="j"
            :row="ent"
            :dictionary="ent.dict_path"
            :get-asset-src="getAssetSrc"
            @anchor-clicked="onAnchorClicked"
          />
        </div>
      </section>
    </div>
  </main>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useTemplateRef,
  watch,
} from "vue";
import { toKana } from "wanakana";

import { useRoute, useRouter } from "vue-router";
import Term from "../components/yomitan/Term.vue";
import { ISearchTerm, ITerm, searchDB } from "../db/search";
import { dictsVersion, toBeLoaded } from "../util/loading";
import { lang } from "../util/settings";

const q = ref("");
const reading = ref("");
const items = ref<(ISearchTerm & { entries: ITerm[] })[]>([]);
const openedItems = ref<number[]>([]);

// "~" needs the lindera tokenizer, which downloads in the background on first launch
const tokenizerPending = computed(() => toBeLoaded.value.some((d) => !d.dict));
const qState = ref<string | null>(null);

function toggleQueryState(value: string) {
  qState.value = qState.value === value ? null : value;
}

function toggleItem(id: number) {
  openedItems.value = openedItems.value.includes(id)
    ? openedItems.value.filter((itemId) => itemId !== id)
    : [...openedItems.value, id];
}

const searchReading = ref(false);
const autoKana = ref(lang.value === "ja-JP");

const loading = ref(false);
const nextOffset = ref<number | null>(0);

const clipboardInterval = ref(0);
const clipboardText = ref("");

const elScroller = useTemplateRef("scroller");

let activeRequest: Promise<void> | null = null;
let requestId = 0;

const route = useRoute();
const router = useRouter();

async function setRouteQuery() {
  const query = {
    q: q.value,
    reading: reading.value,
    qState: qState.value || "",
  };

  const oldQuery = (() => {
    const { q = "", reading = "", qState = "" } = route.query;
    return { q, reading, qState };
  })();

  if (JSON.stringify(query) === JSON.stringify(oldQuery)) return;

  await router.push({ query });
}

function loadFromRouteQuery() {
  if (!route.query.q && !route.query.reading) return;

  q.value = String(route.query.q || "");
  reading.value = String(route.query.reading || "");
  qState.value = String(route.query.qState || "") || null;
}

/** wait for typing to pause; also merges auto-kana's romaji -> kana double update */
const SEARCH_DEBOUNCE_MS = 200;
let searchTimer: ReturnType<typeof setTimeout> | undefined;

watch([q, qState, reading, searchReading, lang, dictsVersion], () => {
  // bump now, so an in-flight search for the old query discards its results
  const id = ++requestId;
  clearTimeout(searchTimer);
  searchTimer = setTimeout(async () => {
    await activeRequest;
    if (id !== requestId) return;

    nextOffset.value = 0;
    loadMore(true);
  }, SEARCH_DEBOUNCE_MS);
});

watch(lang, (value) => {
  if (value === "ko-KR") {
    reading.value = "";
    searchReading.value = false;
  }
});

let isWanakanaFinished = true;

async function onTermInput({ target }: InputEvent) {
  if (!(target instanceof HTMLInputElement)) return;
  if (!isWanakanaFinished) return;

  isWanakanaFinished = false;
  try {
    await nextTick();
    if (autoKana.value && ["", "ja-JP"].includes(lang.value)) {
      const conv = (s: string) =>
        toKana(s, { useObsoleteKana: true, IMEMode: true });

      let { selectionStart } = target;
      selectionStart = selectionStart || target.value.length;
      const prefix = conv(target.value.substring(0, selectionStart));

      target.value = prefix + target.value.substring(selectionStart);
      let i = 0;
      for (const c of prefix) {
        if (target.value[i++] !== c) break;
      }

      target.dispatchEvent(new Event("input", { bubbles: true }));
      await nextTick();

      target.selectionStart = i;
      target.selectionEnd = i;
    }
  } finally {
    isWanakanaFinished = true;
  }
}

function onScroll() {
  const el = elScroller.value;
  if (!el || loading.value || nextOffset.value === null) return;

  const remaining = el.scrollHeight - el.scrollTop - el.clientHeight;
  if (remaining < el.clientHeight / 2) loadMore();
}

async function loadMore(isNew?: boolean) {
  if (loading.value || nextOffset.value === null) return;

  let term = q.value.trimStart();
  const termReading = searchReading.value ? reading.value.trim() : "";

  if (!(term.trimEnd() || termReading)) {
    // empty query: don't leave the previous query's results up
    if (isNew) {
      items.value = [];
      nextOffset.value = null;
    }
    return;
  }

  loading.value = true;

  const id = requestId;

  const offset = nextOffset.value;
  const oldItems = items.value;

  let type = qState.value;

  if (type !== "exact" && term.includes(" ")) {
    term = term.split(" ", 2)[0];
    type = type || "exact";
  }

  const request = (async () => {
    try {
      const result = await searchDB.search({
        term,
        reading: termReading,
        limit: 5,
        offset,
        type,
      });
      if (id !== requestId) return; // the query changed meanwhile

      items.value = isNew ? result.items : [...oldItems, ...result.items];
      nextOffset.value = result.next;

      if (isNew && elScroller.value) {
        elScroller.value.scrollTop = 0;
      }

      openedItems.value = [
        ...openedItems.value,
        ...result.items.map((r) => r.id),
      ];
    } catch (e) {
      // e.g. "no dictionaries downloaded yet" on first launch. Never reject: a rejected
      // activeRequest would make every later search bail out at `await activeRequest`.
      console.error(e);
    } finally {
      loading.value = false;
    }
  })();

  activeRequest = request;
  await request;

  if (activeRequest === request) activeRequest = null;

  // Only check after this request updated the current query's items.
  if (id === requestId) {
    await nextTick();
    onScroll();
  }
}

function getAssetSrc(dict: string, src: string) {
  return searchDB.assetSrc(dict, src);
}

async function onAnchorClicked(href: string) {
  const params = new URLSearchParams(href.slice(1)); // drop the '?'
  const query = params.get("query");
  if (!query) return;

  await setRouteQuery();

  qState.value = "exact";
  q.value = query;

  await setRouteQuery();
}

// A checkbox click must not be preventDefault()ed: the browser then reverts the
// box after Vue has already re-rendered it, so it never shows as toggled.
const clipboardMonitor = computed({
  get: () => clipboardInterval.value !== 0,
  set: (on) => {
    if (on !== (clipboardInterval.value !== 0)) toggleClipboardMonitor();
  },
});

function toggleClipboardMonitor() {
  if (clipboardInterval.value) {
    clearInterval(clipboardInterval.value);
    clipboardInterval.value = 0;
  } else {
    clipboardInterval.value = setInterval(async () => {
      let newText: string;
      try {
        // also converts HTML clips to text (on Android, the clipboard plugin rejects them)
        newText =
          (await invoke<string | null>("read_clipboard_text"))?.trim() ?? "";
      } catch {
        return;
      }
      if (!newText) return; // clipboard is empty or holds something other than text
      if (
        !/[\p{sc=Han}\p{scx=Hiragana}\p{scx=Katakana}\p{sc=Hangul}]/u.test(
          newText.slice(0, 10),
        )
      ) {
        return;
      }

      if (clipboardText.value === newText) return;

      clipboardText.value = newText;
      q.value = newText;
    }, 1000);
  }
}

const routerListener = ref<() => void>();

onMounted(() => {
  loadFromRouteQuery();

  routerListener.value = router.afterEach(() => {
    loadFromRouteQuery();
  });
});

onBeforeUnmount(() => {
  clearTimeout(searchTimer);
  if (clipboardInterval.value) {
    clearInterval(clipboardInterval.value);
    clipboardInterval.value = 0;
  }

  routerListener.value?.();
});
</script>

<style scoped>
.container {
  padding: 1em;
  height: calc(100vh - 3em);
  display: grid;
  grid-template-rows: auto 1fr;

  .search-group {
    display: flex;
    width: 100%;
  }

  .search-input {
    box-sizing: border-box;
    min-width: 0;
    width: 100%;
    padding: 0.5rem;
    border: 1px solid #9ca3af;
    border-radius: 0.25rem;
    font: inherit;
  }

  .search-group .search-input {
    border-radius: 0.25rem 0 0 0.25rem;
  }

  .search-modes {
    display: flex;
  }

  .search-modes button {
    min-width: 2.5rem;
    border: 1px solid #9ca3af;
    border-left: 0;
    background: #f3f4f6;
    font: inherit;
    cursor: pointer;
  }

  .search-modes button:last-child {
    border-radius: 0 0.25rem 0.25rem 0;
  }

  .search-modes button[aria-pressed="true"] {
    background: #dbeafe;
  }

  .search-modes button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  input:focus-visible,
  button:focus-visible {
    outline: 2px solid #2563eb;
    outline-offset: 2px;
  }

  nav * + * {
    margin-inline-start: 0.5em;
  }

  label {
    white-space: nowrap;
  }
}

.scroller {
  overflow: auto;
}

.accordion-item {
  border-bottom: 1px solid #d1d5db;
}

.accordion-header {
  display: flex;
  align-items: center;
  width: 100%;
  min-height: 2.5rem;
  padding: 0.5rem;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: start;
  font: inherit;
  cursor: pointer;
}

.accordion-header::before {
  content: "›";
  margin-inline-end: 0.5rem;
  transition: transform 150ms ease;
}

.accordion-header[aria-expanded="true"]::before {
  transform: rotate(90deg);
}

.accordion-content {
  padding: 0.5rem;
}

.row {
  display: flex;
  flex-direction: row;
  justify-content: center;
  align-items: center;
  gap: 0.5em;
}

.row + .row {
  margin-top: 1em;
}
</style>
