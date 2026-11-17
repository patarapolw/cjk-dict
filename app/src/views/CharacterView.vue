<template>
  <main class="container">
    <header>
      <div class="search-box">
        <input
          ref="searchInput"
          class="search-input"
          inputmode="search"
          name="chars"
          v-model="q"
          placeholder="漢字"
          :lang="lang || undefined"
          autocapitalize="none"
          autocorrect="off"
          autocomplete="off"
          spellcheck="false"
          @keydown.esc="clearQuery"
        />
        <button
          v-if="q"
          type="button"
          class="clear-button"
          aria-label="Clear"
          title="Clear (Esc)"
          @click="clearQuery"
        >
          ×
        </button>
      </div>
      <p class="search-hint">
        <template v-if="chars.length">
          {{ chars.length > 1 ? "Characters" : "Character" }}
          <code>{{ chars.join(" ") }}</code
          >, and the characters made of
          {{ chars.length > 1 ? "all of them" : "it" }}
        </template>
        <template v-else>
          Type or paste characters, e.g. <code>語</code>, or components, e.g.
          <code>木口</code>
        </template>
      </p>
    </header>

    <div class="scroller" ref="scroller">
      <p v-if="error" class="no-results">{{ error }}</p>

      <section
        v-for="c in found"
        :key="c.ch"
        class="char-card"
        :data-char="c.ch"
      >
        <header class="char-header">
          <span class="glyph" :lang="lang || undefined">{{ c.ch }}</span>
          <div class="char-summary">
            <p v-if="c.hangul.length" class="hangul" lang="ko">
              <template v-for="(r, k) in c.hangul" :key="r">
                <template v-if="k"> · </template>
                <button
                  type="button"
                  class="link-button reading"
                  :title="readingTitle(c.ch, r)"
                  @click="toReading(c.ch, r, 'ko-KR')"
                >
                  {{ r }}
                </button>
              </template>
            </p>
            <p class="uses">
              {{ c.uses ? `in ${c.uses.toLocaleString()} words` : "" }}
            </p>
            <div class="item-actions">
              <button
                type="button"
                class="link-button"
                title="Search this character as a word"
                @click="toWords(c.ch, 'exact')"
              >
                = {{ c.ch }}
              </button>
              <button
                type="button"
                class="link-button"
                :disabled="tokenizerPending"
                title="Words containing it"
                @click="toWords(c.ch, 'fts')"
              >
                ~ {{ c.ch }}
              </button>
              <button
                v-if="chars.length > 1"
                type="button"
                class="link-button"
                title="This character only"
                @click="go(c.ch)"
              >
                字 {{ c.ch }}
              </button>
            </div>
          </div>
        </header>

        <div v-if="c.ids.length" class="breakdown">
          <h3>Components</h3>
          <ul class="ids-list">
            <li v-for="(ids, i) in sortIds(c.ids, lang)" :key="i">
              <span class="ids" :lang="lang || undefined">
                <template v-for="(t, j) in idsTokens(ids.ids)" :key="j">
                  <button
                    v-if="t.kind === 'component' && t.text !== c.ch"
                    type="button"
                    class="component"
                    :title="`Look up ${t.text}`"
                    @click="go(t.text)"
                  >
                    {{ t.text }}
                  </button>
                  <span v-else-if="t.kind === 'component'">{{ t.text }}</span>
                  <span v-else class="idc" :title="t.title">{{ t.text }}</span>
                </template>
              </span>
              <span
                v-if="ids.regions"
                class="regions"
                :title="regionTitle(ids.regions)"
                >{{ ids.regions }}</span
              >
            </li>
          </ul>
          <p v-if="!c.components.length" class="muted">
            Does not break down further.
          </p>
          <p
            v-else-if="c.all_components.length > c.components.length"
            class="all-components"
          >
            <span class="muted">All the way down:</span>
            <button
              v-for="comp in c.all_components"
              :key="comp"
              type="button"
              class="component"
              :lang="lang || undefined"
              :title="`Look up ${comp}`"
              @click="go(comp)"
            >
              {{ comp }}
            </button>
          </p>
        </div>

        <article
          v-for="(e, i) in sortByLang(c.entries, lang)"
          :key="i"
          class="kanji-entry"
        >
          <header>
            <span class="tag" data-category="dictionary">{{
              e.dict_path
            }}</span>
            <span
              v-for="t in splitTags(e.tags)"
              :key="t"
              class="tag"
              :title="e.tag_meta[t]?.notes || undefined"
              >{{ t }}</span
            >
          </header>
          <dl class="readings">
            <template v-if="e.onyomi">
              <dt>{{ isJa(e) ? "On" : "Reading" }}</dt>
              <dd :lang="e.lang || undefined">
                <template v-for="(r, k) in splitReadings(e.onyomi)" :key="k">
                  <template v-if="k">、</template>
                  <button
                    type="button"
                    class="link-button reading"
                    :title="readingTitle(c.ch, r)"
                    @click="toReading(c.ch, r)"
                  >
                    {{ r }}
                  </button>
                </template>
              </dd>
            </template>
            <template v-if="e.kunyomi">
              <dt>{{ isJa(e) ? "Kun" : "Reading" }}</dt>
              <dd :lang="e.lang || undefined">
                <template v-for="(r, k) in splitReadings(e.kunyomi)" :key="k">
                  <template v-if="k">、</template>
                  <button
                    type="button"
                    class="link-button reading"
                    :title="readingTitle(c.ch, r)"
                    @click="toReading(c.ch, r)"
                  >
                    {{ r }}
                  </button>
                </template>
              </dd>
            </template>
            <!-- other languages' readings, e.g. KANJIDIC's pinyin and Korean -->
            <template v-for="[name, value] in readingStats(e)" :key="name">
              <dt :title="name">{{ e.tag_meta[name].notes || name }}</dt>
              <dd :lang="STAT_LANGS[name]">
                <template v-if="READING_STAT_LANGS[name]">
                  <template v-for="(r, k) in value.split(/,\s*/)" :key="k">
                    <template v-if="k">, </template>
                    <button
                      type="button"
                      class="link-button reading"
                      :title="readingTitle(c.ch, r)"
                      @click="toReading(c.ch, r, READING_STAT_LANGS[name])"
                    >
                      {{ r }}
                    </button>
                  </template>
                </template>
                <template v-else>{{ value }}</template>
              </dd>
            </template>
          </dl>
          <ol v-if="e.meanings.length" class="meanings">
            <li v-for="(m, j) in e.meanings" :key="j">{{ m }}</li>
          </ol>
          <details v-if="sortedStats(e).length" class="stats">
            <summary>Details</summary>
            <table>
              <tr v-for="[name, value] in sortedStats(e)" :key="name">
                <th :title="name">{{ e.tag_meta[name]?.notes || name }}</th>
                <td>{{ value }}</td>
              </tr>
            </table>
          </details>
        </article>

        <details class="word-entries" @toggle="onWordsToggle(c.ch, $event)">
          <summary>Dictionary entries for {{ c.ch }}</summary>
          <p v-if="words[c.ch] === undefined" class="muted">Loading…</p>
          <p v-else-if="!words[c.ch].length" class="muted">None</p>
          <Term
            v-for="(ent, j) in words[c.ch]"
            :key="j"
            :row="ent"
            show-term
            :get-asset-src="getAssetSrc"
            @anchor-clicked="onAnchorClicked"
          />
        </details>
      </section>

      <section v-if="chars.length" class="containing">
        <h2>
          Characters made of
          <span :lang="lang || undefined">{{ chars.join(" + ") }}</span>
        </h2>
        <p v-if="!containing.length && containingNext === null" class="muted">
          None
        </p>
        <div class="char-grid" :lang="lang || undefined">
          <button
            v-for="it in containing"
            :key="it.ch"
            type="button"
            class="char-button"
            :title="it.uses ? `in ${it.uses.toLocaleString()} words` : ''"
            @click="go(it.ch)"
          >
            {{ it.ch }}
          </button>
        </div>
        <button
          v-if="containingNext !== null"
          type="button"
          class="more-button"
          :disabled="loadingMore"
          @click="loadContaining()"
        >
          More
        </button>
      </section>
    </div>
  </main>
</template>

<script setup lang="ts">
import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  useTemplateRef,
  watch,
} from "vue";
import { useRoute, useRouter } from "vue-router";

import Term from "../components/yomitan/Term.vue";
import { IHanChar, IKanjiEntry, ITerm, searchDB } from "../db/search";
import {
  READING_STAT_LANGS,
  REGION_NAMES,
  hanChars,
  idsTokens,
  readingLang,
  readingQuery,
  sortByLang,
  sortIds,
} from "../util/han";
import { dictsVersion, toBeLoaded } from "../util/loading";
import { lang } from "../util/settings";

/** characters [`han_chars`] looks up at once (MAX_HAN_CHARS in search.rs) */
const MAX_CHARS = 20;
const PAGE = 60;

const q = ref("");
const chars = computed(() => hanChars(q.value).slice(0, MAX_CHARS));

const found = ref<IHanChar[]>([]);
const error = ref("");
const containing = ref<{ ch: string; uses: number }[]>([]);
const containingNext = ref<number | null>(null);
const loadingMore = ref(false);
/** single-character word entries, loaded when their section is opened */
const words = ref<Record<string, ITerm[]>>({});

const tokenizerPending = computed(() => toBeLoaded.value.some((d) => !d.dict));

const elSearchInput = useTemplateRef("searchInput");
const elScroller = useTemplateRef("scroller");

const route = useRoute();
const router = useRouter();

let requestId = 0;

async function load() {
  const id = ++requestId;
  const text = chars.value.join("");
  words.value = {};
  if (!text) {
    found.value = [];
    containing.value = [];
    containingNext.value = null;
    error.value = "";
    return;
  }
  try {
    const [cs, list] = await Promise.all([
      searchDB.hanChars(text),
      searchDB.hanContaining(text, PAGE, 0),
    ]);
    if (id !== requestId) return;
    found.value = cs;
    containing.value = list.items;
    containingNext.value = list.next;
    error.value = "";
    if (elScroller.value) elScroller.value.scrollTop = 0;
  } catch (e) {
    // e.g. "no dictionaries downloaded yet" on first launch
    console.error(e);
    if (id !== requestId) return;
    found.value = [];
    containing.value = [];
    containingNext.value = null;
    error.value = String(e);
  }
}

async function loadContaining() {
  const offset = containingNext.value;
  if (offset === null || loadingMore.value) return;
  const id = requestId;
  loadingMore.value = true;
  try {
    const list = await searchDB.hanContaining(
      chars.value.join(""),
      PAGE,
      offset,
    );
    if (id !== requestId) return;
    containing.value = [...containing.value, ...list.items];
    containingNext.value = list.next;
  } catch (e) {
    console.error(e);
  } finally {
    loadingMore.value = false;
  }
}

async function onWordsToggle(ch: string, e: Event) {
  if (!(e.target as HTMLDetailsElement).open || words.value[ch]) return;
  const id = requestId;
  try {
    const result = await searchDB.search({
      term: ch,
      reading: "",
      readingGlob: "",
      limit: 5,
      offset: 0,
      type: "exact",
    });
    if (id !== requestId) return;
    words.value = {
      ...words.value,
      [ch]: result.items.flatMap((it) => it.entries),
    };
  } catch (err) {
    console.error(err);
    if (id === requestId) words.value = { ...words.value, [ch]: [] };
  }
}

/** wait for typing to pause */
const DEBOUNCE_MS = 200;
let timer: ReturnType<typeof setTimeout> | undefined;
watch([chars, dictsVersion], () => {
  clearTimeout(timer);
  timer = setTimeout(load, DEBOUNCE_MS);
});

async function setRouteQuery() {
  if (String(route.query.q ?? "") === q.value) return;
  await router.push({ query: { q: q.value } });
}

/** Look up something else, keeping the current lookup in the history (back button). */
async function go(text: string) {
  await setRouteQuery();
  q.value = text;
  await setRouteQuery();
}

/**
 * Words starting with `ch` read `reading`, e.g. `<興>{キョウ}`. A Chinese or Korean reading
 * only matches its language's entries: switch the search language to it, unless it is "all".
 */
async function toReading(
  ch: string,
  reading: string,
  readingIn = readingLang(reading),
) {
  if (lang.value && lang.value !== readingIn) lang.value = readingIn;
  await toWords(readingQuery(ch, reading), "");
}

const readingTitle = (ch: string, reading: string) =>
  `Words with ${ch} read ${reading}: ${readingQuery(ch, reading)}`;

async function toWords(term: string, qState: string) {
  await setRouteQuery();
  await router.push({ path: "/dictionary", query: { q: term, qState } });
}

async function clearQuery() {
  if (q.value) await setRouteQuery();
  q.value = "";
  elSearchInput.value?.focus();
}

function onAnchorClicked(href: string) {
  const query = new URLSearchParams(href.slice(1)).get("query");
  if (query) toWords(query, "exact");
}

function getAssetSrc(dict: string, src: string) {
  return searchDB.assetSrc(dict, src);
}

const splitTags = (tags: string) => tags.split(/\s+/).filter(Boolean);
const splitReadings = (s: string) => s.split(/\s+/).filter(Boolean);
const isJa = (e: IKanjiEntry) => (e.lang ?? "").startsWith("ja");

function regionTitle(regions: string) {
  return [...regions].map((r) => REGION_NAMES[r] ?? r).join(", ");
}

/** stats of the "reading" category of the dictionary's tags */
const isReading = (e: IKanjiEntry, name: string) =>
  e.tag_meta[name]?.category === "reading";

/** languages of KANJIDIC's reading stats */
const STAT_LANGS: Record<string, string> = {
  pinyin: "zh",
  korean_h: "ko",
  korean_r: "ko-Latn",
  vietnam: "vi",
  nanori: "ja",
};

/** by the tag bank's sort order, then name */
function byOrder(e: IKanjiEntry, stats: [string, string][]) {
  const order = (name: string) => e.tag_meta[name]?.order ?? 0;
  return [...stats].sort(
    ([a], [b]) => order(a) - order(b) || a.localeCompare(b),
  );
}

const readingStats = (e: IKanjiEntry) =>
  byOrder(
    e,
    e.stats.filter(([name]) => isReading(e, name)),
  );

const sortedStats = (e: IKanjiEntry) =>
  byOrder(
    e,
    e.stats.filter(([name]) => !isReading(e, name)),
  );

let removeRouterListener: (() => void) | undefined;

onMounted(() => {
  q.value = String(route.query.q ?? "");
  removeRouterListener = router.afterEach((to) => {
    if (to.path === "/kanji") q.value = String(to.query.q ?? "");
  });
});

onBeforeUnmount(() => {
  clearTimeout(timer);
  removeRouterListener?.();
});
</script>

<style scoped>
.container {
  box-sizing: border-box;
  padding: 1em;
  min-height: 0;
  height: 100%;
  display: grid;
  grid-template-rows: auto 1fr;
}

.search-box {
  position: relative;
}

.search-input {
  box-sizing: border-box;
  width: 100%;
  padding: 0.5rem;
  padding-inline-end: 2rem;
  border: 1px solid var(--app-input-border);
  border-radius: 0.25rem;
  background: var(--app-surface);
  color: var(--app-text);
  font: inherit;
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

input:focus-visible,
button:focus-visible {
  outline: 2px solid var(--app-focus);
  outline-offset: 2px;
}

.scroller {
  min-height: 0;
  overflow: auto;
}

.no-results,
.muted {
  color: var(--app-text-muted);
}

.char-card {
  padding: 0.75rem 0.5rem;
  border-bottom: 1px solid var(--app-border);
}

.char-header {
  display: flex;
  gap: 1rem;
  align-items: center;
}

.glyph {
  font-size: 4rem;
  line-height: 1;
  min-width: 4.5rem;
  text-align: center;
}

.char-summary p {
  margin: 0 0 0.25rem;
}

.hangul {
  font-size: 1.125rem;
}

.uses {
  font-size: 0.875rem;
  color: var(--app-text-muted);
}

.item-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem 1rem;
  font-size: 0.875rem;
}

h2,
h3 {
  margin: 0.75rem 0 0.25rem;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--app-text-muted);
}

.ids-list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.ids-list li + li {
  margin-top: 0.25rem;
}

.ids {
  font-size: 1.5rem;
}

.idc {
  color: var(--app-text-muted);
  cursor: help;
}

.regions {
  margin-inline-start: 0.5rem;
  font-size: 0.75rem;
  color: var(--app-text-muted);
  cursor: help;
}

.component,
.char-button {
  border: 0;
  padding: 0 0.125rem;
  background: transparent;
  color: var(--app-link);
  font: inherit;
  cursor: pointer;
}

.component:hover,
.char-button:hover {
  background: var(--app-selected-background);
  border-radius: 0.25rem;
}

.all-components {
  margin: 0.25rem 0 0;
  font-size: 1.125rem;
}

.all-components .muted {
  font-size: 0.875rem;
  margin-inline-end: 0.25rem;
}

.kanji-entry {
  margin-top: 0.75rem;
}

.readings {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 0.125rem 0.75rem;
  margin: 0.25rem 0;
}

.readings dt {
  color: var(--app-text-muted);
  font-size: 0.875rem;
}

.readings dd {
  margin: 0;
}

.meanings {
  margin: 0.25rem 0;
  padding-inline-start: 1.5rem;
}

.stats summary,
.word-entries summary {
  cursor: pointer;
  color: var(--app-link);
  font-size: 0.875rem;
}

.stats table {
  font-size: 0.875rem;
  border-collapse: collapse;
}

.stats th {
  padding-inline-end: 1rem;
  text-align: start;
  font-weight: normal;
  color: var(--app-text-muted);
}

.word-entries {
  margin-top: 0.75rem;
}

.word-entries[open] summary {
  margin-bottom: 0.5rem;
}

.containing {
  padding: 0.5rem;
}

.char-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
  font-size: 1.75rem;
}

.char-button {
  min-width: 2.5rem;
  min-height: 2.5rem;
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

.more-button {
  margin-top: 0.5rem;
  padding: 0.25rem 1rem;
  border: 1px solid var(--app-input-border);
  border-radius: 0.25rem;
  background: var(--app-control-background);
  color: var(--app-text);
  font: inherit;
  cursor: pointer;
}
</style>
