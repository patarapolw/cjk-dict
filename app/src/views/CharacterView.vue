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
          <template v-if="readingAs.length">
            as
            <template v-for="(a, i) in readingAs" :key="i"
              >{{ i ? (i < readingAs.length - 1 ? ", " : " or ") : ""
              }}<code v-if="a.kana" lang="ja">{{ a.text }}</code
              ><template v-else>{{ a.text }}</template></template
            ></template
          >
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
          <code>흥</code>, finds the characters read so; so do romaji and Korean
          romanization, by the search language: <code>kyou</code>,
          <code>heung</code>.
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
        <!-- the glyph on the left, the rest stacked on the right, the same on every card;
             a tap anywhere but on a link opens or closes it -->
        <header class="char-header" @click="onHeaderClick(c.ch, $event)">
          <button
            type="button"
            class="char-toggle"
            :aria-expanded="isOpen(c.ch)"
            v-tooltip="isOpen(c.ch) ? 'Show less' : 'Show more'"
            @click="toggle(c.ch)"
          >
            <span class="glyph" :lang="lang || undefined">{{ c.ch }}</span>
          </button>
          <div class="char-summary">
            <p v-if="gist(c)" class="gist-meanings" :lang="gist(c)!.lang">
              {{ gist(c)!.text }}
            </p>
            <!-- the words with it anywhere, as *日* -->
            <button
              v-if="c.uses"
              type="button"
              class="link-button uses"
              v-tooltip="`Words with ${c.ch} anywhere: *${c.ch}*`"
              @click="toWords(`*${c.ch}*`, '')"
            >
              in {{ c.uses.toLocaleString() }} words
            </button>
            <!-- opened, the dictionaries' readings are in their entries, below -->
            <p
              v-if="isOpen(c.ch) && c.hangul.length && hangulShown"
              class="hangul"
              lang="ko"
            >
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
            <!-- collapsed: every reading but kun and name readings, of every entry shown -->
            <!-- one kind alone needs no label -->
            <dl
              v-if="!isOpen(c.ch) && cardReadings(c).length"
              class="readings card-readings"
              :class="{ single: cardReadings(c).length === 1 }"
            >
              <template v-for="row in cardReadings(c)" :key="row.key">
                <dt
                  v-if="cardReadings(c).length > 1"
                  v-tooltip="row.title || row.label"
                >
                  {{ row.label }}
                </dt>
                <dd :lang="row.lang">
                  <template v-for="(it, k) in row.items" :key="k">
                    <template v-if="k">{{ row.sep }}</template>
                    <button
                      v-if="it.searchIn"
                      type="button"
                      class="link-button reading"
                      v-tooltip="readingTitle(it.of, it.r)"
                      @click="toReading(it.of, it.r, it.searchIn)"
                    >
                      {{ it.r
                      }}<span v-if="it.okurigana" class="okurigana">{{
                        it.okurigana
                      }}</span>
                    </button>
                    <template v-else>{{ it.r }}</template>
                  </template>
                </dd>
              </template>
            </dl>
            <div class="item-actions" :lang="lang || undefined">
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
                v-tooltip="`Words with ${c.ch} anywhere`"
                @click="toWords(`*${c.ch}*`, '')"
              >
                *{{ c.ch }}*
              </button>
            </div>
          </div>
        </header>

        <div v-if="c.variants.length" class="variants">
          <h3>Variants</h3>
          <ul class="variant-list">
            <li v-for="v in c.variants" :key="v.ch">
              <button
                type="button"
                class="component"
                :lang="lang || undefined"
                v-tooltip="variantTitle(v)"
                @click="go(v.ch)"
              >
                {{ v.ch }}
              </button>
              <span class="variant-kind">{{ variantLabel(v.kinds) }}</span>
            </li>
          </ul>
        </div>

        <div v-if="isOpen(c.ch)" class="char-details">
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
            <!-- a link to them all, each in full: *日 -->
            <h3>
              <button
                v-if="usedIn[c.ch].items.length"
                type="button"
                class="link-button used-in-heading"
                v-tooltip="superTitle(c.ch)"
                @click="go(`*${c.ch}`)"
              >
                Used in ›
              </button>
              <template v-else>Used in</template>
            </h3>
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
              <!-- the rest, and all of them each in full: *日 -->
              <button
                v-if="usedIn[c.ch].total > usedIn[c.ch].items.length"
                type="button"
                class="link-button used-in-more"
                v-tooltip="superTitle(c.ch)"
                @click="go(`*${c.ch}`)"
              >
                {{
                  (
                    usedIn[c.ch].total - usedIn[c.ch].items.length
                  ).toLocaleString()
                }}
                more
              </button>
            </p>
          </div>

          <p v-if="cardEntries(c).some((it) => it.via)" class="muted via-note">
            No entry for {{ c.ch }}; its variants':
          </p>
          <article
            v-for="({ e, via, of: ech }, i) in cardEntries(c)"
            :key="i"
            class="kanji-entry"
            :data-via="via?.ch"
          >
            <header>
              <button
                v-if="via"
                type="button"
                class="via-variant"
                v-tooltip="`Look up ${variantTitle(via)}`"
                @click="go(via.ch)"
              >
                <span :lang="lang || undefined">{{ via.ch }}</span>
                {{ variantLabel(via.kinds) }}
              </button>
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
            <!-- the dictionary's own readings, so that it's clear where each comes from -->
            <dl v-if="allReadings(e, ech).length" class="readings">
              <template v-for="row in allReadings(e, ech)" :key="row.key">
                <dt v-tooltip="row.title">{{ row.label }}</dt>
                <dd :lang="row.lang">
                  <template v-for="(it, k) in row.items" :key="k">
                    <template v-if="k">{{ row.sep }}</template>
                    <button
                      v-if="it.searchIn"
                      type="button"
                      class="link-button reading"
                      v-tooltip="readingTitle(it.of, it.r)"
                      @click="toReading(it.of, it.r, it.searchIn)"
                    >
                      {{ it.r }}
                    </button>
                    <template v-else>{{ it.r }}</template>
                  </template>
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
                  <th v-tooltip="name">
                    {{ e.tag_meta[name]?.notes || name }}
                  </th>
                  <td>{{ value }}</td>
                </tr>
              </table>
            </details>
          </article>
        </div>
      </section>

      <button
        v-if="paged && superNext !== null"
        type="button"
        class="link-button used-in-more super-more"
        :disabled="loadingMore"
        @click="loadSuper()"
      >
        {{ (superTotal - superNext).toLocaleString() }} more
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
        <!-- until the characters shown have loaded, not "None", nor the last query's -->
        <p v-if="containingOf !== chars.join('')" class="muted">Loading…</p>
        <p
          v-else-if="!containing.length && containingNext === null"
          class="muted"
        >
          None
        </p>
        <div
          v-if="containingOf === chars.join('')"
          class="char-grid"
          :lang="lang || undefined"
        >
          <button
            v-for="it in containing"
            :key="it.ch"
            type="button"
            class="char-button"
            v-tooltip="usedInTitle(it)"
            @click="go(it.ch)"
          >
            <span class="char-glyph">{{ it.ch }}</span>
            <span v-if="it.uses" class="char-uses">{{
              it.uses.toLocaleString()
            }}</span>
          </button>
          <!-- the next page, as "Used in" says how many more -->
          <button
            v-if="containingNext !== null"
            type="button"
            class="link-button used-in-more grid-more"
            :disabled="loadingMore"
            @click="loadContaining()"
          >
            {{ (containingTotal - containingNext).toLocaleString() }} more
          </button>
        </div>
      </section>
    </div>
  </main>
</template>

<script lang="ts">
/** queries whose open cards are remembered */
const OPENED_KEPT = 50;

/**
 * The cards open per query, kept while the app runs: going to Words and back mounts the
 * view again, with the cards as they were
 */
const openedByQuery = new Map<string, string[]>();

function rememberOpened(query: string, chs: string[]) {
  const key = query.trim();
  openedByQuery.delete(key);
  openedByQuery.set(key, chs);
  // the oldest first
  for (const k of openedByQuery.keys()) {
    if (openedByQuery.size <= OPENED_KEPT) break;
    openedByQuery.delete(k);
  }
}
</script>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useTemplateRef,
  watch,
} from "vue";
import { useRoute, useRouter } from "vue-router";

import DictFilter from "../components/DictFilter.vue";
import {
  IHanChar,
  IHanList,
  IHanVariant,
  IKanjiEntry,
  ILatin,
  searchDB,
} from "../db/search";
import {
  READING_STAT_LANGS,
  REGION_NAMES,
  hanChars,
  idsTokens,
  readingLang,
  readingQuery,
  sortByLang,
  sortIds,
  toneMarks,
  variantLabel,
  variantTitle,
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
import { romajiReading } from "../util/romaji";
import { dictsVersion } from "../util/loading";
import { scrollToStart } from "../util/scroll";
import { lang } from "../util/settings";
import { touchScreen, vTooltip } from "../util/tooltip";

/** characters [`han_chars`] looks up at once (MAX_HAN_CHARS in search.rs) */
const MAX_CHARS = 20;
const PAGE = 60;
/** characters made directly of a character, shown on its card */
const USED_IN_PAGE = 30;
/** a query's open cards: as they were left, else none */
const initialOpened = (query: string) => openedByQuery.get(query.trim()) ?? [];

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
/**
 * How to look the reading up, by the search language. A reading in Latin letters is:
 * - with Japanese, romaji, looked up in kana (`kyou` as きょう), or else pinyin;
 * - with Korean, romanization as in Words (`heung`: 흥);
 * - with Chinese, pinyin;
 * - with Auto-detect, all of them at once.
 */
const lookup = computed(() => {
  const r = readingQ.value;
  const auto = !lang.value;
  const kana = auto || lang.value === "ja-JP" ? romajiReading(r) : null;
  return {
    readings: kana ? (auto ? [r, kana] : [kana]) : [r],
    latin: {
      pinyin: lang.value !== "ko-KR",
      korean: auto || lang.value === "ko-KR",
    },
    kana,
    /** Latin letters, read as more than one thing, or as other than pinyin */
    romanized: isRomanization(r),
  };
});
/** what the reading was taken as, for the hint: きょう, pinyin, Korean romanization */
const readingAs = computed(() => {
  const { kana, latin, romanized } = lookup.value;
  const out: { text: string; kana?: boolean }[] = [];
  if (kana) out.push({ text: kana, kana: true });
  if (!romanized) return out;
  // with Auto-detect: every way
  if (latin.pinyin && latin.korean) out.push({ text: "pinyin" });
  if (latin.korean) out.push({ text: "Korean romanization" });
  return out;
});
/** the key of what a reading looks up, to search again when it changes */
const lookupKey = computed(() => JSON.stringify(lookup.value));
/** characters listed a page at a time, each in full: `*木口` or a reading */
const paged = computed(() => superQuery.value || !!readingQ.value);

const found = ref<IHanChar[]>([]);
const error = ref("");
const containing = ref<{ ch: string; uses: number }[]>([]);
const containingNext = ref<number | null>(null);
/** how many characters `containing` has on every page */
const containingTotal = ref(0);
/** the characters `containing` is of, which the ones typed may have moved on from */
const containingOf = ref("");
const loadingMore = ref(false);
/** each character's characters made directly of it (super-characters) */
const usedIn = ref<Record<string, IHanList>>({});
/** with a `*` query or a reading, the offset of its next page of characters */
const superNext = ref<number | null>(null);
/** how many characters a `*` query or a reading has on every page */
const superTotal = ref(0);
/** a reading that no character has */
const noneFound = ref(false);
/** a card's own Hangul readings, of no dictionary: with Korean or Auto-detect */
const hangulShown = computed(() => !lang.value || lang.value === "ko-KR");

/** the characters whose cards are open */
const opened = ref<string[]>([]);

const isOpen = (ch: string) => opened.value.includes(ch);

/** a tap on a card's header, but not on one of its links or buttons: open or close it */
function onHeaderClick(ch: string, e: MouseEvent) {
  if ((e.target as Element).closest("button, a, summary")) return;
  // selecting text isn't a tap
  if (window.getSelection()?.toString()) return;
  toggle(ch);
}

function toggle(ch: string) {
  opened.value = isOpen(ch)
    ? opened.value.filter((c) => c !== ch)
    : [...opened.value, ch];
  rememberOpened(q.value, opened.value);
}

/**
 * A collapsed card's gist: the meanings of its first entry shown, or with none, of its
 * variants', on one line that the card's width cuts off
 */
function gist(c: IHanChar) {
  const first = cardEntries(c)[0];
  if (!first?.e.meanings.length) return null;
  return { text: first.e.meanings.join("; "), lang: glossLang(first.e) };
}

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

/** the characters [`han_chars`] returned for the variants of characters with no entry */
const variantChars = ref<Record<string, IHanChar>>({});
/** variants asked for, so that one that failed isn't asked for again */
let variantsAsked = new Set<string>();

/**
 * A card's kanji dictionary entries: its own shown entries, or with none, its variants'
 * (`via`); `of` is the character an entry is of, to search the words with its readings
 */
function cardEntries(c: IHanChar) {
  type Item = { e: IKanjiEntry; of: string; via?: IHanVariant };
  const own: Item[] = shownEntries(c).map((e) => ({ e, of: c.ch }));
  if (own.length) return own;
  return c.variants.flatMap((via): Item[] => {
    const v = variantChars.value[via.ch];
    return v ? shownEntries(v).map((e) => ({ e, of: via.ch, via })) : [];
  });
}

/** variants to look up: of the characters with no entry shown */
const variantsToAsk = computed(() =>
  found.value
    .filter((c) => !shownEntries(c).length)
    .flatMap((c) => c.variants.map((v) => v.ch))
    .filter((ch) => !variantsAsked.has(ch) && !variantChars.value[ch]),
);

watch(variantsToAsk, async (chs) => {
  const todo = [...new Set(chs)];
  if (!todo.length) return;
  todo.forEach((ch) => variantsAsked.add(ch));
  const id = requestId;
  try {
    for (let i = 0; i < todo.length; i += MAX_CHARS) {
      const cs = await searchDB.hanChars(
        todo.slice(i, i + MAX_CHARS).join(""),
        lang.value,
      );
      if (id !== requestId) return;
      variantChars.value = {
        ...variantChars.value,
        ...Object.fromEntries(cs.map((c) => [c.ch, c])),
      };
    }
  } catch (e) {
    console.error(e);
  }
});

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
  const { readings, latin } = lookup.value;
  if (!text && !reading) {
    clear();
    return;
  }
  try {
    if (paged.value) {
      const page = await superPage(text, reading ? readings : [], latin, 0);
      if (id !== requestId) return;
      clear();
      found.value = page.found;
      opened.value = initialOpened(q.value);
      usedIn.value = page.usedIn;
      superNext.value = page.next;
      superTotal.value = page.total;
      noneFound.value = !!reading && !page.found.length;
    } else {
      const [cs, list, direct] = await Promise.all([
        searchDB.hanChars(text, lang.value),
        chars.value.length > 1
          ? searchDB.hanContaining(text, PAGE, 0, false, lang.value)
          : { items: [], next: null, total: 0 },
        usedInOf(chars.value),
      ]);
      if (id !== requestId) return;
      clear();
      found.value = cs;
      opened.value = initialOpened(q.value);
      usedIn.value = direct;
      containing.value = list.items;
      containingOf.value = text;
      containingNext.value = list.next;
      containingTotal.value = list.total;
    }
    if (elScroller.value) scrollToStart(elScroller.value);
  } catch (e) {
    // e.g. "no dictionaries downloaded yet" on first launch
    console.error(e);
    if (id !== requestId) return;
    clear();
    error.value = String(e);
    // the error says why: not "Loading…" under it
    containingOf.value = text;
  }
}

function clear() {
  found.value = [];
  opened.value = [];
  variantChars.value = {};
  variantsAsked = new Set();
  usedIn.value = {};
  superNext.value = null;
  containing.value = [];
  containingNext.value = null;
  containingOf.value = "";
  noneFound.value = false;
  error.value = "";
}

/** Each of `chs`' characters made directly of it */
async function usedInOf(chs: string[]): Promise<Record<string, IHanList>> {
  const lists = await Promise.all(
    chs.map((ch) =>
      searchDB.hanContaining(ch, USED_IN_PAGE, 0, true, lang.value),
    ),
  );
  return Object.fromEntries(chs.map((ch, i) => [ch, lists[i]]));
}

/**
 * A page of the characters read any of `readings`, or else made directly of all of
 * `components`, each in full
 */
async function superPage(
  components: string,
  readings: string[],
  latin: ILatin,
  offset: number,
) {
  const list = readings.length
    ? await searchDB.hanByReading(
        readings,
        latin,
        MAX_CHARS,
        offset,
        lang.value,
      )
    : await searchDB.hanContaining(
        components,
        MAX_CHARS,
        offset,
        true,
        lang.value,
      );
  const chs = list.items.map((it) => it.ch);
  const [cs, direct] = chs.length
    ? await Promise.all([
        searchDB.hanChars(chs.join(""), lang.value),
        usedInOf(chs),
      ])
    : [[], {}];
  return { found: cs, usedIn: direct, next: list.next, total: list.total };
}

async function loadSuper() {
  const offset = superNext.value;
  if (offset === null || loadingMore.value) return;
  const id = requestId;
  loadingMore.value = true;
  try {
    const page = await superPage(
      chars.value.join(""),
      readingQ.value ? lookup.value.readings : [],
      lookup.value.latin,
      offset,
    );
    if (id !== requestId) return;
    found.value = [...found.value, ...page.found];
    usedIn.value = { ...usedIn.value, ...page.usedIn };
    superNext.value = page.next;
    superTotal.value = page.total;
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
      false,
      lang.value,
    );
    if (id !== requestId) return;
    containing.value = [...containing.value, ...list.items];
    containingNext.value = list.next;
    containingTotal.value = list.total;
  } catch (e) {
    console.error(e);
  } finally {
    loadingMore.value = false;
  }
}

const usedInTitle = (it: { ch: string; uses: number }) =>
  it.uses ? `${it.ch}, in ${it.uses.toLocaleString()} words` : it.ch;
const superTitle = (components: string) =>
  `Every character made directly of ${components}, each in full: *${components}`;

/** wait for typing to pause */
const DEBOUNCE_MS = 200;
let timer: ReturnType<typeof setTimeout> | undefined;
// the search language too: the characters most used in its words first, and their counts
/** the query changes by navigation (a link, Back, opening the view), not typing: load at once */
let navigated = false;
watch([chars, lookupKey, dictsVersion, lang], () => {
  clearTimeout(timer);
  if (navigated) {
    navigated = false;
    load();
  } else timer = setTimeout(load, DEBOUNCE_MS);
});

/** sets the query from navigation, to be looked up without waiting for typing to pause */
function navigateTo(text: string) {
  if (text !== q.value) navigated = true;
  q.value = text;
  // the watch above has run by then, or won't, the characters being the same
  nextTick(() => (navigated = false));
}

async function setRouteQuery() {
  if (String(route.query.q ?? "") === q.value) return;
  await router.push({ query: { q: q.value } });
}

/** Look up something else, keeping the current lookup in the history (back button). */
async function go(text: string) {
  await setRouteQuery();
  navigateTo(text);
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

/** a reading, searching the words with `of` read so in `searchIn`; with none, text */
type ReadingItem = {
  r: string;
  of: string;
  searchIn?: string;
  /** after a kun reading's stem `r`, shown but not searched: る of ひか.る */
  okurigana?: string;
};
/** a row of readings of one kind: On, Pinyin, Hangul, a dictionary's own */
type ReadingRow = {
  key: string;
  label: string;
  title?: string;
  lang?: string;
  sep: string;
  items: ReadingItem[];
};

/** the kinds a card's header lists first, in this order */
const READING_KINDS: Record<string, { label: string; lang: string }> = {
  on: { label: "On", lang: "ja" },
  pinyin: { label: "Pinyin", lang: "zh" },
  hangul: { label: "Hangul", lang: "ko" },
};
/** readings in the on and kun fields of a kind above, by their language */
const FIELD_KINDS: Record<string, string> = {
  "ja-JP": "on",
  "ko-KR": "hangul",
};
/** reading stats of a kind above */
const STAT_KINDS: Record<string, string> = {
  pinyin: "pinyin",
  korean_h: "hangul",
};
/** readings left inside the card: kun and name readings, often many */
const INNER_STATS = new Set(["nanori"]);

const kindRow = (key: string, items: ReadingItem[]): ReadingRow => ({
  key,
  ...READING_KINDS[key],
  sep: key === "on" ? "、" : ", ",
  items,
});

/**
 * An entry's readings, split into those of the card's header (`outer`) and those left
 * inside (`inner`): kun readings, and name readings
 */
function entryReadings(e: IKanjiEntry, of: string) {
  const outer: ReadingRow[] = [];
  const inner: ReadingRow[] = [];
  for (const field of [e.onyomi, e.kunyomi]) {
    const rs = splitReadings(field);
    if (!rs.length) continue;
    const items = rs.map((r) => ({ r, of, searchIn: readingLang(r) }));
    const label = readingLabel(field);
    if (label === "Kun") {
      inner.push({ key: "kun", label, lang: e.lang || "ja", sep: "、", items });
      continue;
    }
    // on, or another language's in those fields: CC-CEDICT.Hanzi's pinyin, Hanja's Hangul
    const kind = FIELD_KINDS[items[0].searchIn] ?? "pinyin";
    outer.push(kindRow(kind, items));
  }
  for (const [name, value] of readingStats(e)) {
    const searchIn = READING_STAT_LANGS[name];
    const items = value
      .split(/,\s*/)
      .filter(Boolean)
      .map((r) => ({ r, of, searchIn }));
    const kind = STAT_KINDS[name];
    if (kind) outer.push(kindRow(kind, items));
    else
      (INNER_STATS.has(name) ? inner : outer).push({
        key: name,
        label: e.tag_meta[name]?.notes || name,
        title: name,
        lang: STAT_LANGS[name],
        sep: ", ",
        items,
      });
  }
  return { outer, inner };
}

/** rows by kind: on, kun, pinyin, Hangul, the dictionary's others, name readings last */
const ROW_ORDER = ["on", "kun", "pinyin", "hangul"];
const rowRank = (key: string) =>
  INNER_STATS.has(key)
    ? ROW_ORDER.length + 1
    : ROW_ORDER.includes(key)
      ? ROW_ORDER.indexOf(key)
      : ROW_ORDER.length;
/** stable: the dictionary's others as they come */
const byRowKind = (rows: ReadingRow[]) =>
  rows.sort((a, b) => rowRank(a.key) - rowRank(b.key));

/**
 * All an entry's readings, shown in it on an open card, as its dictionary gives them: its
 * on and kun fields, labelled by their script, then its reading stats by the tag bank's
 * order, with the dictionary's own notes
 */
function allReadings(e: IKanjiEntry, of: string): ReadingRow[] {
  const rows: ReadingRow[] = [];
  [e.onyomi, e.kunyomi].forEach((field, i) => {
    const rs = splitReadings(field);
    if (!rs.length) return;
    rows.push({
      key: i ? "kunyomi" : "onyomi",
      label: readingLabel(field),
      lang: e.lang || undefined,
      sep: "、",
      items: rs.map((r) => ({ r, of, searchIn: readingLang(r) })),
    });
  });
  // other languages' readings, e.g. KANJIDIC's pinyin and Korean
  for (const [name, value] of readingStats(e)) {
    const searchIn = READING_STAT_LANGS[name];
    rows.push({
      key: name,
      label: e.tag_meta[name]?.notes || name,
      title: name,
      lang: STAT_LANGS[name],
      sep: ", ",
      items: value.split(/,\s*/).map((r) => ({ r, of, searchIn })),
    });
  }
  return rows;
}

/**
 * A collapsed card's reading kinds, by the search language, to tell the characters of a
 * list apart by the readings one knows them by; every kind with Auto-detect
 */
const HEADER_KINDS: Record<string, string[]> = {
  "ja-JP": ["on", "kun"],
  zh: ["pinyin"],
  "ko-KR": ["hangul"],
};
/** kun readings a collapsed card shows: the first few, without okurigana */
const SHORT_KUN = 3;

/**
 * Kun readings, short: the first few, each once, without `-`; the okurigana apart from
 * the stem, which is what a reading's link searches (ひか of ひか.る)
 */
function shortKun(row: ReadingRow): ReadingRow {
  const items: ReadingItem[] = [];
  for (const it of row.items) {
    const [r, okurigana = ""] = it.r.replace(/^[-‐]+|[-‐]+$/gu, "").split(".");
    if (
      r &&
      !items.some(
        (x) => x.r === r && x.okurigana === okurigana && x.of === it.of,
      )
    )
      items.push({ ...it, r, okurigana });
  }
  const more = items.length > SHORT_KUN;
  return {
    ...row,
    items: [
      ...items.slice(0, SHORT_KUN),
      ...(more ? [{ r: "…", of: "" }] : []),
    ],
  };
}

/**
 * A collapsed card's readings, of every entry shown, merged, one row per kind: On, short
 * kun, Pinyin, Hangul (its own Hangul readings too), then the dictionaries' others; those
 * of the search language only (see HEADER_KINDS), or with none, all; never name readings,
 * often many
 */
function cardReadings(c: IHanChar): ReadingRow[] {
  const all = mergedReadings(c);
  const kinds = HEADER_KINDS[lang.value];
  if (!kinds) return all;
  const ofLang = all.filter((row) => kinds.includes(row.key));
  // none of the search language's, e.g. a character only CC-CEDICT.Hanzi has, with
  // Japanese: whatever it has, rather than nothing
  return ofLang.length ? ofLang : all;
}

/** every reading kind of a card, merged, but name readings */
function mergedReadings(c: IHanChar): ReadingRow[] {
  const rows = new Map<string, ReadingRow>();
  const add = (row: ReadingRow) => {
    if (INNER_STATS.has(row.key)) return;
    // one way of writing pinyin, so that each reading is listed once: KANJIDIC's xīng
    // and CC-CEDICT.Hanzi's xing1 are both xīng
    if (row.key === "pinyin")
      row = {
        ...row,
        items: row.items.map((it) => ({
          ...it,
          r: toneMarks(it.r).toLowerCase(),
        })),
      };
    const had = rows.get(row.key);
    if (!had) return rows.set(row.key, { ...row, items: [...row.items] });
    for (const it of row.items) {
      if (!had.items.some((x) => x.r === it.r && x.of === it.of))
        had.items.push(it);
    }
  };
  if (c.hangul.length)
    add(
      kindRow(
        "hangul",
        c.hangul.map((r) => ({ r, of: c.ch, searchIn: "ko-KR" })),
      ),
    );
  for (const { e, of } of cardEntries(c)) {
    const { outer, inner } = entryReadings(e, of);
    [...outer, ...inner].forEach(add);
  }
  const kun = rows.get("kun");
  if (kun) rows.set("kun", shortKun(kun));
  return byRowKind([...rows.values()]);
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
  navigateTo(String(route.query.q ?? ""));
  removeRouterListener = router.afterEach((to) => {
    if (to.path === "/kanji") navigateTo(String(to.query.q ?? ""));
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
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 0 0.75rem;
  align-items: start;
  cursor: pointer;
}

/* as Words' accordion header: › then the glyph */
.char-toggle {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--app-text);
  font: inherit;
  cursor: pointer;
}

.char-toggle::before {
  content: "›";
  transition: transform 150ms ease;
}

.char-toggle[aria-expanded="true"]::before {
  transform: rotate(90deg);
}

.glyph {
  font-size: 4rem;
  line-height: 1;
  width: 4.5rem;
  text-align: center;
}

.char-summary {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  min-width: 0;
  /* in line with the glyph's top */
  padding-top: 0.25rem;
}

.char-summary p {
  margin: 0 0 0.125rem;
}

.gist-meanings {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.char-toggle[aria-expanded="true"] + .char-summary .gist-meanings {
  white-space: normal;
}

.hangul {
  font-size: 1.125rem;
}

.card-readings {
  margin: 0.125rem 0 0.25rem;
}

.card-readings.single {
  grid-template-columns: 1fr;
}

.link-button.uses {
  font-size: 0.875rem;
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

/* the whole row, not only its words */
.link-button.used-in-heading {
  display: block;
  width: 100%;
  font-weight: inherit;
  text-align: start;
  color: var(--app-link);
}

/* a small button, in text size beside the characters */
.link-button.used-in-more {
  margin-inline-start: 0.375rem;
  padding: 0.125rem 0.5rem;
  border: 1px solid var(--app-input-border);
  border-radius: 0.25rem;
  background: var(--app-control-background);
  font-size: 0.8125rem;
  vertical-align: middle;
  white-space: nowrap;
}

.link-button.used-in-more:hover {
  background: var(--app-selected-background);
}

/* in the grid of characters, in line with them */
.link-button.grid-more {
  align-self: center;
}

.link-button.okurigana,
.okurigana {
  color: var(--app-text-muted);
}

.super-more {
  margin: 0 0.5rem 0.5rem;
}

.kanji-entry {
  margin-top: 0.75rem;
}

/* labels and readings on one baseline, whatever the fonts of their scripts (#75) */
.readings {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 0.125rem 0.75rem;
  align-items: baseline;
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

.stats summary {
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
  /* each at the top of its row, with or without a count under it */
  align-items: flex-start;
  gap: 0.25rem;
  font-size: 1.75rem;
}

.char-button {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-start;
  min-width: 2.5rem;
  min-height: 2.5rem;
}

/* one height whatever font draws it, so that the counts under line up (#75) */
.char-glyph {
  display: block;
  height: 1.25em;
  line-height: 1.25em;
}

/* how many words use it, under it */
.char-uses {
  display: block;
  font-size: 0.6875rem;
  line-height: 1;
  color: var(--app-text-muted);
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

.variant-list {
  display: flex;
  flex-wrap: wrap;
  /* the variants on one baseline, whatever fonts draw them (#75) */
  align-items: baseline;
  gap: 0.25rem 1rem;
  margin: 0;
  padding: 0;
  list-style: none;
}

/* a variant and its kind on one baseline (#75) */
.variant-list li {
  display: flex;
  align-items: baseline;
}

.variant-list .component {
  font-size: 1.5rem;
}

.variant-kind {
  margin-inline-start: 0.25rem;
  font-size: 0.8125rem;
  color: var(--app-text-muted);
}

.via-note {
  margin: 0.75rem 0 0;
  font-size: 0.875rem;
}

.via-variant {
  margin-inline-end: 0.25rem;
  padding: 0 0.375rem;
  border: 1px solid var(--app-input-border);
  border-radius: 0.25rem;
  background: transparent;
  color: var(--app-link);
  font: inherit;
  font-size: 0.875rem;
  cursor: pointer;
}

.via-variant:hover {
  background: var(--app-selected-background);
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
