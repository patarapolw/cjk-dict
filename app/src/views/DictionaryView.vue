<template>
  <main class="container">
    <header>
      <InputGroup>
        <InputText
          inputmode="search"
          name="term"
          v-model="q"
          :placeholder="searchReading ? 'Term' : ''"
          autocapitalize="none"
          autocorrect="off"
          autocomplete="off"
          spellcheck="false"
          @input="onTermInput"
          fluid
        />
        <InputGroupAddon>
          <SelectButton
            v-model="qState"
            :options="qStateOptions"
            option-label="label"
            option-value="value"
            option-disabled="disabled"
            v-tooltip.bottom="
              tokenizerPending
                ? '~ (token search) is available once the tokenizer download finishes'
                : undefined
            "
          />
        </InputGroupAddon>
      </InputGroup>
      <InputText
        v-if="searchReading"
        inputmode="search"
        name="reading"
        v-model="reading"
        placeholder="Reading"
        autocapitalize="none"
        autocorrect="off"
        autocomplete="off"
        spellcheck="false"
        @input="onTermInput"
        fluid
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

        <label @click.prevent="toggleClipboardMonitor()">
          <input
            type="checkbox"
            name="clipboard"
            :model-value="clipboardInterval !== 0"
          />
          <span>Clipboard monitor</span>
        </label>
      </nav>
    </header>
    <div class="scroller" ref="scroller" @scroll.passive="onScroll">
      <Accordion multiple v-model:value="openedItems">
        <AccordionPanel v-for="it in items" :key="it.id" :value="it.id">
          <AccordionHeader>
            <span v-if="!openedItems.includes(it.id)">
              {{ it.entries[0]?.term || it.term }}
            </span>
          </AccordionHeader>
          <AccordionContent>
            <Term
              v-for="(ent, j) in it.entries"
              :key="j"
              :row="ent"
              :dictionary="ent.dict_path"
              :get-asset-src="getAssetSrc"
              @anchor-clicked="onAnchorClicked"
            />
          </AccordionContent>
        </AccordionPanel>
      </Accordion>
    </div>
  </main>
</template>

<script setup lang="ts">
import Accordion from "primevue/accordion";
import AccordionContent from "primevue/accordioncontent";
import AccordionHeader from "primevue/accordionheader";
import AccordionPanel from "primevue/accordionpanel";
import InputGroup from "primevue/inputgroup";
import InputGroupAddon from "primevue/inputgroupaddon";
import InputText from "primevue/inputtext";
import SelectButton from "primevue/selectbutton";

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

import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { useRoute, useRouter } from "vue-router";
import Term from "../components/yomitan/Term.vue";
import { ISearchTerm, ITerm, searchDB } from "../db/search";
import { toBeLoaded } from "../util/loading";
import { lang } from "../util/settings";

const q = ref("");
const reading = ref("");
const items = ref<(ISearchTerm & { entries: ITerm[] })[]>([]);
const openedItems = ref<number[]>([]);

// "~" needs the lindera tokenizer, which downloads in the background on first launch
const tokenizerPending = computed(() => toBeLoaded.value.length > 0);
const qStateOptions = computed(() => [
  { label: "~", value: "fts", disabled: tokenizerPending.value },
  { label: "=", value: "exact", disabled: false },
]);
const qState = ref<string | null>(null);

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

watch([q, qState, reading, lang], async () => {
  const id = ++requestId;
  await activeRequest;
  if (id !== requestId) return;

  nextOffset.value = 0;
  loadMore(true);
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

  if (!(term.trimEnd() || termReading)) return;

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

      items.value = isNew ? result.items : [...oldItems, ...result.items];
      nextOffset.value = result.next;

      if (isNew && elScroller.value) {
        elScroller.value.scrollTop = 0;
      }

      openedItems.value = [
        ...openedItems.value,
        ...result.items.map((r) => r.id),
      ];
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

function toggleClipboardMonitor() {
  if (clipboardInterval.value) {
    clearInterval(clipboardInterval.value);
    clipboardInterval.value = 0;
  } else {
    clipboardInterval.value = setInterval(async () => {
      const newText = (await readText()).trim();
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
