<template>
  <main class="container">
    <header>
      <div class="search-group">
        <div class="search-box">
          <input
            ref="searchInput"
            class="search-input"
            inputmode="search"
            name="term"
            v-model="q"
            :placeholder="placeholder"
            :lang="lang || undefined"
            autocapitalize="none"
            autocorrect="off"
            autocomplete="off"
            spellcheck="false"
            @input="onTermInput"
            @compositionstart="onCompositionStart"
            @compositionupdate="onCompositionUpdate"
            @compositionend="onCompositionEnd"
            @keydown.esc="clearQuery"
          />
          <button
            v-if="q"
            type="button"
            class="clear-button"
            aria-label="Clear"
            v-tooltip="'Clear (Esc)'"
            @click="clearQuery"
          >
            ×
          </button>
        </div>
        <div class="search-modes" role="group" aria-label="Search mode">
          <button
            type="button"
            :aria-pressed="qState === 'fts'"
            :disabled="tokenizerPending"
            v-tooltip="
              tokenizerPending
                ? '~ (token search) is available once the tokenizer download finishes'
                : 'Token search: words containing it'
            "
            @click="toggleQueryState('fts')"
          >
            ~
          </button>
          <button
            type="button"
            :aria-pressed="qState === 'exact'"
            v-tooltip="'Exact match'"
            @click="toggleQueryState('exact')"
          >
            =
          </button>
          <button
            type="button"
            :aria-pressed="readingMode"
            v-tooltip="'Reading search: type only the reading, no {} needed'"
            @click="toggleReadingMode"
          >
            {}
          </button>
        </div>
      </div>
      <p class="search-hint">
        <!-- what the query is understood as, so the syntax and modes explain themselves -->
        <template v-if="charReading">
          Searching words with <code>{{ charReading.ch }}</code> read
          <code>{{ charReading.reading }}</code>
        </template>
        <template v-else-if="parsed.term || hintReading">
          Searching
          <code v-if="parsed.term">{{ parsed.term }}</code>
          <template v-if="hintReading">
            {{ parsed.term ? "read" : "by reading" }}
            <code>{{ hintReading }}</code>
          </template>
          <template v-if="romanizedQuery"> as romanization</template>
          <template v-if="qState === 'exact'"> (exact)</template>
          <template v-else-if="qState === 'fts'">
            (words containing it)</template
          >
          <button
            v-if="parsed.rest"
            type="button"
            class="link-button"
            @click="searchNext"
          >
            Next: {{ parsed.rest }}
          </button>
        </template>
        <template v-else-if="readingMode">
          Type only the reading<template v-if="examples.reading"
            >, e.g. <code>{{ examples.reading }}</code></template
          >
        </template>
        <template v-else-if="examples.furigana">
          Add a reading: <code>{{ examples.furigana }}</code
          ><!-- on a touchscreen, the long-press line below explains {} instead,
               in less room --><template v-if="!touchScreen"
            >, or press <code>{}</code> to search by reading only</template
          >
        </template>
        <span v-if="touchScreen && !q.trim()" class="touch-help">
          Long-press a button or option to see what it does.
        </span>
      </p>
      <nav>
        <label
          v-if="isJapanese"
          v-tooltip="'Convert romaji to kana as you type: nihongo → にほんご'"
        >
          <input type="checkbox" name="autokana" v-model="autoKana" />
          <span>Auto-convert to Kana</span>
        </label>

        <label
          v-if="isJapanese"
          v-tooltip="
            'When a Japanese IME converts にほんご to 日本語, also match the reading you typed: <日本>{にほん}語'
          "
        >
          <input type="checkbox" name="imefurigana" v-model="imeFurigana" />
          <span>Get reading from IME</span>
        </label>

        <label
          v-if="isKorean"
          v-tooltip="
            'Type Revised Romanization to find Hangul: hangugeo → 한국어'
          "
        >
          <input type="checkbox" name="romanko" v-model="romanKo" />
          <span>Search by romanization</span>
        </label>

        <label>
          <input
            type="checkbox"
            name="clipboard"
            v-model="clipboardMonitorOn"
          />
          <span>Clipboard monitor</span>
        </label>

        <DictFilter
          :dicts="shownDicts"
          v-model:excluded="excludedDicts"
          all-label="Search all"
          purpose="search"
        />
      </nav>
    </header>
    <div class="scroller" ref="scroller">
      <p v-if="noResults && !noDicts" class="no-results">No results</p>
      <section v-for="it in items" :key="it.id" class="accordion-item">
        <button
          type="button"
          class="accordion-header"
          :aria-expanded="openedItems.includes(it.id)"
          @click="toggleItem(it.id)"
        >
          <ruby
            v-if="headReading(it)"
            class="headword"
            :lang="itemLang(it)"
            :data-pronunciation-categories="categoriesOf(it)"
            >{{ headTerm(it) }}<rt>{{ headReading(it) }}</rt></ruby
          >
          <span
            v-else
            class="headword"
            :lang="itemLang(it)"
            :data-pronunciation-categories="categoriesOf(it)"
            >{{ headTerm(it) }}</span
          >
        </button>
        <div v-if="openedItems.includes(it.id)" class="accordion-content">
          <div class="item-actions" :lang="itemLang(it)">
            <button
              type="button"
              class="link-button"
              v-tooltip="'Search this word only'"
              @click="searchFor(furiganaOf(it), 'exact')"
            >
              = {{ headTerm(it) }}
            </button>
            <button
              type="button"
              class="link-button"
              :disabled="tokenizerPending"
              v-tooltip="'Words containing it'"
              @click="searchFor(headTerm(it), 'fts')"
            >
              ~ {{ headTerm(it) }}
            </button>
            <button
              v-if="headReading(it)"
              type="button"
              class="link-button"
              v-tooltip="'Words with this reading'"
              @click="searchFor(braced(headReading(it)), 'exact')"
            >
              {{ braced(headReading(it)) }}
            </button>
            <button
              v-if="hanChars(headTerm(it)).length"
              type="button"
              class="link-button"
              v-tooltip="'Its characters: components, readings and meanings'"
              @click="toCharacters(headTerm(it))"
            >
              字 {{ hanChars(headTerm(it)).join("") }}
            </button>
          </div>
          <PitchAccents
            v-if="it.pitches?.length"
            :entries="it.pitches"
            :headwords="headwordsOf(it.entries)"
          />
          <Term
            v-for="(ent, j) in it.entries"
            :key="j"
            :row="ent"
            :show-term="
              ent.term !== headTerm(it) || ent.reading !== it.entries[0].reading
            "
            :get-asset-src="getAssetSrc"
            @anchor-clicked="onAnchorClicked"
          />
        </div>
      </section>
      <!-- results load as the list scrolls near its end; a list too short to scroll, e.g.
           with every result collapsed, needs a way to ask too -->
      <button
        v-if="items.length && nextOffset !== null"
        type="button"
        class="more-button"
        :disabled="loading"
        @click="loadMore()"
      >
        More results
      </button>
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
import DictFilter from "../components/DictFilter.vue";
import PitchAccents from "../components/PitchAccents.vue";
import Term from "../components/yomitan/Term.vue";
import { ISearchTerm, ITerm, searchDB } from "../db/search";
import { clipboardMonitorOn, lastClipboardText } from "../util/clipboard";
import {
  isImeReading,
  isRomanization,
  normalizeQuery,
  parseQuery,
  toFurigana,
} from "../util/furigana";
import {
  dictList,
  excludedDicts,
  loadDicts,
  noneIncluded,
  wordDicts,
} from "../util/dicts";
import { hanChars } from "../util/han";
import {
  headwordsOf,
  isVerbOrAdjective,
  pronunciationCategories,
} from "../util/pitch";
import { declaredHeadLang, headLang, refineHeadLangs } from "../util/lang";
import { dictsVersion, toBeLoaded } from "../util/loading";
import { remainingScroll, scrollToStart } from "../util/scroll";
import { lang } from "../util/settings";
import { touchScreen, vTooltip } from "../util/tooltip";

type Item = ISearchTerm & { entries: ITerm[] };

const q = ref("");
const items = ref<Item[]>([]);
const openedItems = ref<number[]>([]);
const noResults = ref(false);

/**
 * reading search: the whole query is a reading, as if typed as `{reading}`. Korean
 * headwords in Hanja get Hangul readings in search.db; a Hangul headword is its own reading.
 */
const readingMode = ref(false);

function toggleReadingMode() {
  readingMode.value = !readingMode.value;
  elSearchInput.value?.focus();
}

const isJapanese = computed(() => ["", "ja-JP"].includes(lang.value));
const autoKana = ref(lang.value === "ja-JP");

const parsed = computed(() => {
  const query = q.value.trim();
  // auto-convert to kana leaves an incomplete syllable as typed (にほんg): search its kana
  // row (にほん[がぎぐげごっ]) instead of nothing, which would split the query into words
  const opts = { kanaRow: autoKana.value && isJapanese.value };
  // furigana syntax typed by hand still wins
  if (readingMode.value && query && !/[<>{}＜＞｛｝]/u.test(query)) {
    return parseQuery(`{${query}}`, opts);
  }
  return parseQuery(q.value, opts);
});

// "~" needs the lindera tokenizer, which downloads in the background on first launch
const tokenizerPending = computed(() => toBeLoaded.value.some((d) => !d.dict));
const qState = ref<string | null>(null);

function toggleQueryState(value: string) {
  qState.value = qState.value === value ? null : value;
}

async function toggleItem(id: number) {
  openedItems.value = openedItems.value.includes(id)
    ? openedItems.value.filter((itemId) => itemId !== id)
    : [...openedItems.value, id];
  // collapsing can leave the list too short to scroll, so that onScroll never asks for more
  await nextTick();
  onScroll();
}

const imeFurigana = ref(true);

const isKorean = computed(() => lang.value === "ko-KR");
/** search Korean by typing romanization: hangugeo → 한국어 */
const romanKo = ref(true);
/** the term is Latin letters, matched against how Hangul terms are romanized */
const romanizedQuery = computed(
  () =>
    isKorean.value &&
    romanKo.value &&
    isRomanization(parsed.value.term) &&
    !parsed.value.reading,
);

/** the parsed reading, e.g. `にほん*` (`*`: the rest of the reading is unknown) */
/** `*<興>{キョウ}`: words with 興 read キョウ, by the index of characters' readings */
const charReading = computed(() =>
  parsed.value.ch && parsed.value.chReading && !qState.value
    ? { ch: parsed.value.ch, reading: parsed.value.chReading }
    : null,
);

const hintReading = computed(
  () => parsed.value.reading || parsed.value.readingGlob,
);

/** per-language examples for the search hint and placeholder */
const examples = computed(
  () =>
    ({
      "ja-JP": { furigana: "<日本>{にほん}語", reading: "にほんご" },
      "": { furigana: "<日本>{にほん}語", reading: "にほんご" },
      zh: { furigana: "<中国>{zhong guo}", reading: "zhong1 guo2" },
      "ko-KR": { furigana: "<光速>{광속}", reading: "광속" },
    })[lang.value] ?? { furigana: "", reading: "" },
);

const placeholder = computed(() =>
  readingMode.value
    ? examples.value.reading || "Reading"
    : ({ "ja-JP": "日本語", zh: "中文", "ko-KR": "한국어" }[lang.value] ??
      "Search"),
);

/** the word dictionaries of the search language, to choose from */
const shownDicts = computed(() => wordDicts(dictList.value, lang.value));

/** every dictionary listed is left out, e.g. after switching the search language */
const noDicts = computed(() =>
  noneIncluded(shownDicts.value, excludedDicts.value),
);

watch(dictsVersion, () => loadDicts());

const loading = ref(false);
const nextOffset = ref<number | null>(0);

/** the monitor's timer while this view is shown; 0 when off */
let clipboardInterval = 0;

const elScroller = useTemplateRef("scroller");
const elSearchInput = useTemplateRef("searchInput");

let activeRequest: Promise<void> | null = null;
let requestId = 0;

const route = useRoute();
const router = useRouter();

async function setRouteQuery() {
  const query = {
    q: q.value,
    qState: qState.value || "",
    reading: readingMode.value ? "1" : "",
  };

  const oldQuery = (() => {
    const { q = "", qState = "", reading = "" } = route.query;
    return { q, qState, reading };
  })();

  if (JSON.stringify(query) === JSON.stringify(oldQuery)) return;

  await router.push({ query });
}

function loadFromRouteQuery() {
  if (!route.query.q) return;

  q.value = String(route.query.q || "");
  qState.value = String(route.query.qState || "") || null;
  readingMode.value = route.query.reading === "1";
}

/** Search for something else, keeping the current search in the history (back button). */
async function searchFor(query: string, state: string | null = qState.value) {
  await setRouteQuery();
  qState.value = state;
  // a term, or a reading in {} syntax: never wrap it in {} again
  readingMode.value = false;
  q.value = query;
  await setRouteQuery();
}

/** The characters view, keeping the current search in the history (back button). */
async function toCharacters(term: string) {
  await setRouteQuery();
  await router.push({ path: "/kanji", query: { q: hanChars(term).join("") } });
}

function searchNext() {
  searchFor(parsed.value.rest, null);
}

async function clearQuery() {
  if (q.value) await setRouteQuery();
  q.value = "";
  elSearchInput.value?.focus();
}

/** wait for typing to pause; also merges auto-kana's romaji -> kana double update */
const SEARCH_DEBOUNCE_MS = 200;
let searchTimer: ReturnType<typeof setTimeout> | undefined;

watch(
  [
    q,
    qState,
    readingMode,
    lang,
    dictsVersion,
    romanKo,
    autoKana,
    excludedDicts,
  ],
  () => {
    // bump now, so an in-flight search for the old query discards its results
    const id = ++requestId;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(async () => {
      await activeRequest;
      if (id !== requestId) return;

      nextOffset.value = 0;
      loadMore(true);
    }, SEARCH_DEBOUNCE_MS);
  },
  { deep: true },
);

let isWanakanaFinished = true;

async function onTermInput(e: Event) {
  const { target } = e;
  if (!(target instanceof HTMLInputElement)) return;
  // the IME is still composing: leave its text alone
  if ((e as InputEvent).isComposing) return;
  if (!isWanakanaFinished) return;

  isWanakanaFinished = false;
  try {
    await nextTick();
    if (autoKana.value && isJapanese.value) {
      const conv = (s: string) =>
        toKana(s, {
          useObsoleteKana: true,
          IMEMode: true,
          // keep the furigana syntax as typed
          customKanaMapping: { "<": "<", ">": ">", "{": "{", "}": "}" },
        });

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

/*
 * IME furigana: while a Japanese IME composes, it shows the reading (にほんご) before the
 * user converts it to kanji (日本語). When the conversion is committed, the committed text is
 * replaced with furigana syntax (<日本語>{にほんご}), so the search also matches the reading.
 */
let imeReading = "";

function onCompositionStart() {
  imeReading = "";
}

function onCompositionUpdate(e: CompositionEvent) {
  if (e.data && isImeReading(e.data)) imeReading = e.data.replace(/ｎ/g, "ん");
}

async function onCompositionEnd(e: CompositionEvent) {
  const reading = imeReading;
  imeReading = "";
  const committed = e.data;
  const target = e.target;
  if (!imeFurigana.value || !isJapanese.value) return;
  if (!reading || !committed || !(target instanceof HTMLInputElement)) return;

  const markup = toFurigana(committed, reading);
  if (markup === committed) return;

  // let v-model take the committed text first
  await nextTick();
  const end = target.selectionStart ?? target.value.length;
  const start = end - committed.length;
  if (start < 0 || target.value.slice(start, end) !== committed) return;

  // don't nest inside furigana the user typed, e.g. <日本>{|}
  const before = normalizeQuery(target.value.slice(0, start));
  if (before.lastIndexOf("{") > before.lastIndexOf("}")) return;
  if (before.lastIndexOf("<") > before.lastIndexOf(">")) return;

  q.value = target.value.slice(0, start) + markup + target.value.slice(end);
  await nextTick();
  const caret = start + markup.length;
  target.setSelectionRange(caret, caret);
}

function onScroll() {
  const el = elScroller.value;
  if (!el || loading.value || nextOffset.value === null) return;

  const { remaining, clientHeight } = remainingScroll(el);
  if (remaining < clientHeight / 2) loadMore();
}

/** language to segment `text` with: the setting, or a guess from its script */
function segmentLang(text: string) {
  if (lang.value) return lang.value;
  if (/\p{sc=Hangul}/u.test(text)) return "ko-KR";
  if (/[\p{scx=Hiragana}\p{scx=Katakana}]/u.test(text)) return "ja-JP";
  return "zh";
}

async function loadMore(isNew?: boolean) {
  if (loading.value || nextOffset.value === null) return;

  const query = parsed.value;
  const term = query.term.trim();
  const { reading, readingGlob, rest } = query;
  const romanized = romanizedQuery.value;

  if (!(term || reading)) {
    // empty query: don't leave the previous query's results up
    if (isNew) {
      items.value = [];
      nextOffset.value = null;
      noResults.value = false;
    }
    return;
  }

  loading.value = true;

  const id = requestId;

  const offset = nextOffset.value;
  const oldItems = items.value;

  let type = qState.value;
  // a space marks a word boundary: look up the first word only
  if (rest) type = type || "exact";

  // an incomplete syllable is still being typed: what it starts, not an exact word
  if (query.incomplete && type === "exact") type = null;

  let segmented: string[] = [];

  const request = (async () => {
    try {
      const result = await searchDB.search({
        term,
        reading,
        readingGlob,
        romanized,
        ch: charReading.value?.ch,
        chReading: charReading.value?.reading,
        excludeDicts: excludedDicts.value,
        limit: 5,
        offset,
        type,
      });
      if (id !== requestId) return; // the query changed meanwhile

      // Nothing matches a longer text, e.g. a sentence from the clipboard: look up its
      // first word instead (and offer the rest as "Next").
      if (
        isNew &&
        !result.items.length &&
        !rest &&
        !reading &&
        !readingGlob &&
        !romanized &&
        !query.incomplete &&
        type !== "fts" &&
        [...term].length > 1
      ) {
        segmented = await searchDB.segment(term, segmentLang(term));
        if (id !== requestId) return;
        if (segmented.length > 1) return; // handled below, as a new query
      }

      items.value = isNew ? result.items : [...oldItems, ...result.items];
      nextOffset.value = result.next;
      if (isNew) noResults.value = !result.items.length;

      if (isNew && elScroller.value) scrollToStart(elScroller.value);

      openedItems.value = [
        ...openedItems.value,
        ...result.items.map((r) => r.id),
      ];
    } catch (e) {
      // e.g. "no dictionaries downloaded yet" on first launch. Never reject: a rejected
      // activeRequest would make every later search bail out at `await activeRequest`.
      console.error(e);
      // Stop paging: with no results the list never fills the screen, so onScroll()
      // below would call loadMore() again and retry the failing search forever. The
      // next query, or dictionaries being put in place, starts over at offset 0.
      if (id === requestId) nextOffset.value = null;
    } finally {
      loading.value = false;
    }
  })();

  activeRequest = request;
  await request;

  if (activeRequest === request) activeRequest = null;

  if (segmented.length > 1 && id === requestId) {
    q.value = `${segmented[0]} ${segmented.slice(1).join("")}`;
    return;
  }

  // Only check after this request updated the current query's items.
  if (id === requestId) {
    await nextTick();
    onScroll();
  }
}

function headTerm(it: Item) {
  return it.entries[0]?.term || it.term;
}

/** the headings' languages, so their Han characters take that language's forms */
const itemLangs = computed(() => {
  const langs = refineHeadLangs(
    items.value,
    (it) => headLang(it.entries, lang.value),
    headTerm,
    (it) => declaredHeadLang(it.entries),
  );
  return new Map(items.value.map((it, i) => [it.id, langs[i]]));
});

function itemLang(it: Item) {
  return itemLangs.value.get(it.id);
}

/**
 * the headword's pitch patterns, as Yomitan puts them on it for custom CSS, e.g. `heiban`;
 * none without pitch accents
 */
function categoriesOf(it: Item): string | undefined {
  const [head] = headwordsOf(it.entries);
  if (!head || !it.pitches?.length) return undefined;
  return (
    pronunciationCategories(
      head,
      it.pitches,
      isVerbOrAdjective(it.entries[0]?.rules),
    ) || undefined
  );
}

function headReading(it: Item) {
  // Korean dictionaries give none: search.db's, e.g. 광속 for 光速
  const reading = it.entries[0]?.reading || it.reading.trim();
  return reading && reading !== headTerm(it) ? reading : "";
}

/** reading-only query syntax */
const braced = (reading: string) => `{${reading}}`;

/** the result as a query in furigana syntax, e.g. <食>{た}べる */
function furiganaOf(it: Item) {
  return toFurigana(headTerm(it), headReading(it));
}

function getAssetSrc(dict: string, src: string) {
  return searchDB.assetSrc(dict, src);
}

async function onAnchorClicked(href: string) {
  const params = new URLSearchParams(href.slice(1)); // drop the '?'
  const query = params.get("query");
  if (!query) return;

  await searchFor(query, "exact");
}

function stopClipboardMonitor() {
  clearInterval(clipboardInterval);
  clipboardInterval = 0;
}

function startClipboardMonitor() {
  if (clipboardInterval) return;
  clipboardInterval = setInterval(async () => {
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

    if (lastClipboardText.value === newText) return;

    lastClipboardText.value = newText;
    // copied text is a term: search it in the normal mode, not by reading, ~ or =
    readingMode.value = false;
    qState.value = null;
    q.value = newText;
  }, 1000);
}

// The setting outlives this view (see clipboardMonitorOn); the timer runs only while
// the view is shown.
watch(clipboardMonitorOn, (on) =>
  on ? startClipboardMonitor() : stopClipboardMonitor(),
);

const routerListener = ref<() => void>();

/** a taller window, or a rotated phone, can leave room for more results too */
let resizeObserver: ResizeObserver | undefined;

onMounted(() => {
  loadFromRouteQuery();
  loadDicts();
  if (clipboardMonitorOn.value) startClipboardMonitor();

  if (typeof ResizeObserver !== "undefined" && elScroller.value) {
    resizeObserver = new ResizeObserver(() => onScroll());
    resizeObserver.observe(elScroller.value);
  }
  // Either the results list scrolls, or in a short window the whole page does (see
  // scrollContainer). Scroll events don't bubble, but they can be captured.
  window.addEventListener("scroll", onScroll, { capture: true, passive: true });
  window.addEventListener("resize", onScroll, { passive: true });

  routerListener.value = router.afterEach((to) => {
    // not the query of another view being navigated to
    if (to.path === "/dictionary") loadFromRouteQuery();
  });
});

onBeforeUnmount(() => {
  clearTimeout(searchTimer);
  resizeObserver?.disconnect();
  window.removeEventListener("scroll", onScroll, { capture: true });
  window.removeEventListener("resize", onScroll);
  stopClipboardMonitor();

  routerListener.value?.();
});
</script>

<style scoped>
.container {
  box-sizing: border-box;
  padding: 1em;
  /* Fill the layout's 1fr row instead of guessing the app header's height */
  min-height: 0;
  height: 100%;
  display: grid;
  grid-template-rows: auto 1fr;
  /* not wider than the window (see App.vue) */
  grid-template-columns: minmax(0, 1fr);

  .search-group {
    display: flex;
    width: 100%;
  }

  .search-input {
    box-sizing: border-box;
    min-width: 0;
    width: 100%;
    padding: 0.5rem;
    border: 1px solid var(--app-input-border);
    background: var(--app-surface);
    color: var(--app-text);
    border-radius: 0.25rem;
    font: inherit;
  }

  .search-box {
    position: relative;
    flex-grow: 1;
    min-width: 0;
  }

  .search-group .search-input {
    border-radius: 0.25rem 0 0 0.25rem;
    padding-inline-end: 2rem;
  }

  .clear-button {
    position: absolute;
    inset-block: 0;
    inset-inline-end: 0;
    width: 2rem;
    border: 0;
    background: transparent;
    color: var(--app-text-muted);
    font: inherit;
    font-size: 1.25rem;
    cursor: pointer;
  }

  .search-hint {
    margin: 0.25rem 0;
    font-size: 0.8125rem;
    color: var(--app-text-muted);
  }

  .search-modes {
    display: flex;
  }

  .search-modes button {
    min-width: 2.5rem;
    border: 1px solid var(--app-input-border);
    border-left: 0;
    background: var(--app-control-background);
    color: var(--app-text);
    font: inherit;
    cursor: pointer;
  }

  .search-modes button:last-child {
    border-radius: 0 0.25rem 0.25rem 0;
  }

  .search-modes button[aria-pressed="true"] {
    background: var(--app-selected-background);
  }

  .search-modes button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  input:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--app-focus);
    outline-offset: 2px;
  }

  /* options wrap onto more lines in a narrow window */
  nav {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25em 1em;
  }

  nav label {
    display: inline-flex;
    align-items: center;
    gap: 0.5em;
    min-width: 0;
  }
}

.scroller {
  /* Let the 1fr row shrink below the content height so this element scrolls */
  min-height: 0;
  overflow: auto;
}

.no-results {
  padding: 0.5rem;
  color: var(--app-text-muted);
}

.more-button {
  display: block;
  margin: 0.75rem auto;
  padding: 0.25rem 1rem;
  border: 1px solid var(--app-input-border);
  border-radius: 0.25rem;
  background: var(--app-control-background);
  color: var(--app-text);
  font: inherit;
  cursor: pointer;
}

.more-button:disabled {
  cursor: progress;
  opacity: 0.55;
}

.link-button {
  border: 0;
  padding: 0;
  background: transparent;
  color: var(--app-link);
  font: inherit;
  cursor: pointer;
}

.link-button:hover:not(:disabled) {
  text-decoration: underline;
}

.link-button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.headword {
  font-size: 1.25rem;
}

.item-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem 1rem;
  margin-bottom: 0.5rem;
  font-size: 0.875rem;
}

.accordion-item {
  border-bottom: 1px solid var(--app-border);
}

.accordion-header {
  display: flex;
  align-items: center;
  width: 100%;
  min-height: 2.5rem;
  padding: 0.5rem;
  border: 0;
  background: var(--app-surface);
  color: var(--app-text);
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
/* A short window, such as an Android pop-up (floating) window: the search box and
   options would leave little room for results, so the whole page scrolls instead
   (App.vue). Keep in step with the media query there. */
@media (max-height: 28rem) {
  .container {
    height: auto;
    display: block;
    padding: 0.5em;
  }

  .scroller {
    overflow: visible;
  }
}
</style>
