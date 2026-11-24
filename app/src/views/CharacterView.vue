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
          v-tooltip="'Clear (Esc)'"
          @click="clearQuery"
        >
          ×
        </button>
      </div>
      <p class="search-hint">
        <template v-if="readingQ">
          Characters read <code>{{ readingQ }}</code>
          <template v-if="romanizedQ"> as romanization</template>
        </template>
        <template v-else-if="superQuery && chars.length">
          Characters made directly of <code>{{ chars.join(" + ") }}</code>
        </template>
        <template v-else-if="chars.length">
          {{ chars.length > 1 ? "Characters" : "Character" }}
          <code>{{ chars.join(" ") }}</code>
        </template>
        <template v-else>
          Type or paste characters, e.g. <code>語</code>. Several, e.g.
          <code>木口</code>, also find the characters made of all of them: 呆,
          杏, …; <code>*口</code> shows the characters made directly of 口, each
          in full. A reading, e.g. <code>きょう</code>, <code>xing</code> or
          <code>흥</code>, finds the characters read so; with Korean selected,
          romanization does too: <code>heung</code>.
        </template>
        <span
          v-if="touchScreen && !chars.length && !readingQ"
          class="touch-help"
        >
          Long-press a button or option to see what it does.
        </span>
      </p>
      <nav>
        <DictFilter
          :dicts="shownDicts"
          v-model:excluded="excludedCharDicts"
          all-label="Show all"
          purpose="see its entries"
        />
      </nav>
    </header>

    <div class="scroller" ref="scroller">
      <p v-if="error" class="no-results">{{ error }}</p>
      <p v-else-if="noneFound" class="no-results">
        No characters read
        <span :lang="readingLang(readingQ)">{{ readingQ }}</span>
      </p>

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
                  v-tooltip="readingTitle(c.ch, r)"
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
                v-tooltip="'Search this character as a word'"
                @click="toWords(c.ch, 'exact')"
              >
                = {{ c.ch }}
              </button>
              <button
                type="button"
                class="link-button"
                :disabled="tokenizerPending"
                v-tooltip="'Words containing it'"
                @click="toWords(c.ch, 'fts')"
              >
                ~ {{ c.ch }}
              </button>
              <button
                v-if="found.length > 1 || paged"
                type="button"
                class="link-button"
                v-tooltip="'This character only'"
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
                    v-tooltip="`Look up ${t.text}`"
                    @click="go(t.text)"
                  >
                    {{ t.text }}
                  </button>
                  <span v-else-if="t.kind === 'component'">{{ t.text }}</span>
                  <span v-else class="idc" v-tooltip="t.title">{{
                    t.text
                  }}</span>
                </template>
              </span>
              <span
                v-if="ids.regions"
                class="regions"
                v-tooltip="regionTitle(ids.regions)"
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
              v-tooltip="`Look up ${comp}`"
              @click="go(comp)"
            >
              {{ comp }}
            </button>
          </p>
        </div>

        <div v-if="usedIn[c.ch]" class="used-in">
          <h3>Used in</h3>
          <p v-if="!usedIn[c.ch].items.length" class="muted">
            Not a component of another character.
          </p>
          <p v-else class="used-in-list" :lang="lang || undefined">
            <button
              v-for="it in usedIn[c.ch].items"
              :key="it.ch"
              type="button"
              class="component"
              v-tooltip="usedInTitle(it)"
              @click="go(it.ch)"
            >
              {{ it.ch }}
            </button>
            <span
              v-if="usedIn[c.ch].total > usedIn[c.ch].items.length"
              class="muted used-in-more"
              >{{ usedIn[c.ch].total - usedIn[c.ch].items.length }} more</span
            >
          </p>
          <button
            v-if="usedIn[c.ch].items.length"
            type="button"
            class="super-search"
            v-tooltip="superTitle(c.ch)"
            @click="go(`*${c.ch}`)"
          >
            Search
            <span class="super-query" :lang="lang || undefined"
              >*{{ c.ch }}</span
            >
          </button>
        </div>

        <article v-for="(e, i) in shownEntries(c)" :key="i" class="kanji-entry">
          <header>
            <span
              class="tag"
              data-category="dictionary"
              :lang="e.lang || undefined"
              >{{ e.dict_path }}</span
            >
            <span
              v-for="t in splitTags(e.tags)"
              :key="t"
              class="tag"
              v-tooltip="e.tag_meta[t]?.notes || undefined"
              >{{ t }}</span
            >
          </header>
          <dl class="readings">
            <template v-if="e.onyomi">
              <dt>{{ readingLabel(e.onyomi) }}</dt>
              <dd :lang="e.lang || undefined">
                <template v-for="(r, k) in splitReadings(e.onyomi)" :key="k">
                  <template v-if="k">、</template>
                  <button
                    type="button"
                    class="link-button reading"
                    v-tooltip="readingTitle(c.ch, r)"
                    @click="toReading(c.ch, r)"
                  >
                    {{ r }}
                  </button>
                </template>
              </dd>
            </template>
            <template v-if="e.kunyomi">
              <dt>{{ readingLabel(e.kunyomi) }}</dt>
              <dd :lang="e.lang || undefined">
                <template v-for="(r, k) in splitReadings(e.kunyomi)" :key="k">
                  <template v-if="k">、</template>
                  <button
                    type="button"
                    class="link-button reading"
                    v-tooltip="readingTitle(c.ch, r)"
                    @click="toReading(c.ch, r)"
                  >
                    {{ r }}
                  </button>
                </template>
              </dd>
            </template>
            <!-- other languages' readings, e.g. KANJIDIC's pinyin and Korean -->
            <template v-for="[name, value] in readingStats(e)" :key="name">
              <dt v-tooltip="name">{{ e.tag_meta[name].notes || name }}</dt>
              <dd :lang="STAT_LANGS[name]">
                <template v-if="READING_STAT_LANGS[name]">
                  <template v-for="(r, k) in value.split(/,\s*/)" :key="k">
                    <template v-if="k">, </template>
                    <button
                      type="button"
                      class="link-button reading"
                      v-tooltip="readingTitle(c.ch, r)"
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
          <ol v-if="e.meanings.length" class="meanings" :lang="glossLang(e)">
            <li v-for="(m, j) in e.meanings" :key="j">{{ m }}</li>
          </ol>
          <details v-if="sortedStats(e).length" class="stats">
            <summary>Details</summary>
            <table>
              <tr v-for="[name, value] in sortedStats(e)" :key="name">
                <th v-tooltip="name">{{ e.tag_meta[name]?.notes || name }}</th>
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

      <button
        v-if="paged && superNext !== null"
        type="button"
        class="more-button super-more"
        :disabled="loadingMore"
        @click="loadSuper()"
      >
        More
      </button>

      <!-- one character's are under "Used in" on its card -->
      <section v-if="!paged && chars.length > 1" class="containing">
        <h2>
          Characters made of
          <span :lang="lang || undefined">{{ chars.join(" + ") }}</span>
        </h2>
        <p class="muted containing-note">
          Each is made of all {{ chars.length }} characters, at any depth.
        </p>
        <p v-if="!containing.length && containingNext === null" class="muted">
          None
        </p>
        <div class="char-grid" :lang="lang || undefined">
          <button
            v-for="it in containing"
            :key="it.ch"
            type="button"
            class="char-button"
            v-tooltip="usedInTitle(it)"
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

import DictFilter from "../components/DictFilter.vue";
import Term from "../components/yomitan/Term.vue";
import { IHanChar, IHanList, IKanjiEntry, ITerm, searchDB } from "../db/search";
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
import {
  charDicts,
  dictList,
  excludedCharDicts,
  isOfLang,
  loadDicts,
} from "../util/dicts";
import { isRomanization } from "../util/furigana";
import { glossLang } from "../util/lang";
import { dictsVersion, toBeLoaded } from "../util/loading";
import { scrollToStart } from "../util/scroll";
import { lang } from "../util/settings";
import { touchScreen, vTooltip } from "../util/tooltip";

/** characters [`han_chars`] looks up at once (MAX_HAN_CHARS in search.rs) */
const MAX_CHARS = 20;
const PAGE = 60;
/** characters made directly of a character, shown on its card */
const USED_IN_PAGE = 30;

const q = ref("");
/**
 * `{きょう}`, or what has no Han characters (`きょう`, `xing`, `흥`): the characters read so,
 * each shown in full
 */
const readingQ = computed(() => {
  const t = q.value.trim();
  const m = /^[{｛]([^}｝]*)[}｝]?$/u.exec(t);
  if (m) return m[1].trim();
  return /^[*＊]/.test(t) || hanChars(t).length ? "" : t;
});
const chars = computed(() =>
  readingQ.value ? [] : hanChars(q.value).slice(0, MAX_CHARS),
);
/** `*木口`: the characters made directly of all of them, each shown in full */
const superQuery = computed(() => !readingQ.value && /^\s*[*＊]/.test(q.value));
/** with Korean selected, a reading in Latin letters is romanization (`heung`: 흥), as in Words */
const romanizedQ = computed(
  () => lang.value === "ko-KR" && isRomanization(readingQ.value),
);
/** characters listed a page at a time, each in full: `*木口` or a reading */
const paged = computed(() => superQuery.value || !!readingQ.value);

const found = ref<IHanChar[]>([]);
const error = ref("");
const containing = ref<{ ch: string; uses: number }[]>([]);
const containingNext = ref<number | null>(null);
const loadingMore = ref(false);
/** each character's characters made directly of it (super-characters) */
const usedIn = ref<Record<string, IHanList>>({});
/** with a `*` query or a reading, the offset of its next page of characters */
const superNext = ref<number | null>(null);
/** a reading that no character has */
const noneFound = ref(false);
/** single-character word entries, loaded when their section is opened */
const words = ref<Record<string, ITerm[]>>({});

const tokenizerPending = computed(() => toBeLoaded.value.some((d) => !d.dict));

/** the kanji dictionaries of the search language, to choose from */
const shownDicts = computed(() => charDicts(dictList.value, lang.value));

/**
 * A character's kanji dictionary entries to show: those of the dictionaries chosen, and
 * as in Words, of the search language or of none (KANJIDIC); the search language's first
 */
const shownEntries = (c: IHanChar) =>
  sortByLang(
    c.entries.filter(
      (e) =>
        isOfLang(e.lang, lang.value) &&
        !excludedCharDicts.value.includes(e.dict_path),
    ),
    lang.value,
  );

watch(dictsVersion, () => loadDicts());

const elSearchInput = useTemplateRef("searchInput");
const elScroller = useTemplateRef("scroller");

const route = useRoute();
const router = useRouter();

let requestId = 0;

async function load() {
  const id = ++requestId;
  const text = chars.value.join("");
  const reading = readingQ.value;
  const romanized = romanizedQ.value;
  words.value = {};
  if (!text && !reading) {
    clear();
    return;
  }
  try {
    if (paged.value) {
      const page = await superPage(text, reading, romanized, 0);
      if (id !== requestId) return;
      clear();
      found.value = page.found;
      usedIn.value = page.usedIn;
      superNext.value = page.next;
      noneFound.value = !!reading && !page.found.length;
    } else {
      const [cs, list, direct] = await Promise.all([
        searchDB.hanChars(text),
        chars.value.length > 1
          ? searchDB.hanContaining(text, PAGE, 0)
          : { items: [], next: null },
        usedInOf(chars.value),
      ]);
      if (id !== requestId) return;
      clear();
      found.value = cs;
      usedIn.value = direct;
      containing.value = list.items;
      containingNext.value = list.next;
    }
    if (elScroller.value) scrollToStart(elScroller.value);
  } catch (e) {
    // e.g. "no dictionaries downloaded yet" on first launch
    console.error(e);
    if (id !== requestId) return;
    clear();
    error.value = String(e);
  }
}

function clear() {
  found.value = [];
  usedIn.value = {};
  superNext.value = null;
  containing.value = [];
  containingNext.value = null;
  noneFound.value = false;
  error.value = "";
}

/** Each of `chs`' characters made directly of it */
async function usedInOf(chs: string[]): Promise<Record<string, IHanList>> {
  const lists = await Promise.all(
    chs.map((ch) => searchDB.hanContaining(ch, USED_IN_PAGE, 0, true)),
  );
  return Object.fromEntries(chs.map((ch, i) => [ch, lists[i]]));
}

/**
 * A page of the characters read `reading`, or else made directly of all of `components`,
 * each in full
 */
async function superPage(
  components: string,
  reading: string,
  romanized: boolean,
  offset: number,
) {
  const list = reading
    ? await searchDB.hanByReading(reading, MAX_CHARS, offset, romanized)
    : await searchDB.hanContaining(components, MAX_CHARS, offset, true);
  const chs = list.items.map((it) => it.ch);
  const [cs, direct] = chs.length
    ? await Promise.all([searchDB.hanChars(chs.join("")), usedInOf(chs)])
    : [[], {}];
  return { found: cs, usedIn: direct, next: list.next };
}

async function loadSuper() {
  const offset = superNext.value;
  if (offset === null || loadingMore.value) return;
  const id = requestId;
  loadingMore.value = true;
  try {
    const page = await superPage(
      chars.value.join(""),
      readingQ.value,
      romanizedQ.value,
      offset,
    );
    if (id !== requestId) return;
    found.value = [...found.value, ...page.found];
    usedIn.value = { ...usedIn.value, ...page.usedIn };
    superNext.value = page.next;
  } catch (e) {
    console.error(e);
  } finally {
    loadingMore.value = false;
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

const usedInTitle = (it: { ch: string; uses: number }) =>
  it.uses ? `${it.ch}, in ${it.uses.toLocaleString()} words` : it.ch;
const superTitle = (components: string) =>
  `Every character made directly of ${components}, each in full`;

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
watch([chars, readingQ, romanizedQ, dictsVersion], () => {
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
/**
 * On and kun readings tell themselves apart by script (コウ, おこ.る); other kanji
 * dictionaries put other readings in those fields, e.g. CC-CEDICT's Hanzi edition its pinyin.
 */
function readingLabel(readings: string) {
  if (/^[\p{scx=Katakana}ー\s.\-]+$/u.test(readings)) return "On";
  if (/^[\p{scx=Hiragana}ー\s.\-]+$/u.test(readings)) return "Kun";
  return "Reading";
}

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
  loadDicts();
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
  /* not wider than the window (see App.vue) */
  grid-template-columns: minmax(0, 1fr);
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

/* options wrap onto more lines in a narrow window, as in Words */
nav {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25em 1em;
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

.used-in-list {
  margin: 0.25rem 0 0;
  font-size: 1.125rem;
}

.super-search {
  display: inline-flex;
  align-items: baseline;
  gap: 0.375rem;
  margin-top: 0.375rem;
  /* big enough to tap */
  min-height: 2.25rem;
  padding: 0.25rem 0.75rem;
  border: 1px solid var(--app-input-border);
  border-radius: 0.25rem;
  background: var(--app-control-background);
  color: var(--app-text);
  font: inherit;
  font-size: 0.875rem;
  cursor: pointer;
}

.super-search:hover {
  background: var(--app-selected-background);
}

.super-query {
  font-size: 1.125rem;
}

.used-in-more {
  margin-inline-start: 0.375rem;
  font-size: 0.875rem;
  white-space: nowrap;
}

.super-more {
  margin: 0 0.5rem 0.5rem;
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

.containing h2 {
  margin-bottom: 0;
}

.containing-note {
  margin: 0.25rem 0 0.5rem;
  font-size: 0.875rem;
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
