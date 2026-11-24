import { invoke } from "@tauri-apps/api/core";
import { type Ref, ref, watch } from "vue";

import { primaryLang } from "./lang";

/** A dictionary in the downloaded library (`search.db`'s `dict` table) */
export interface IDictInfo {
  /** the zip's name, e.g. `CC-CEDICT`; the key */
  name: string;
  /** index.json's title */
  title: string;
  revision: string;
  /** the terms' language, e.g. `ja-JP` */
  lang: string | null;
  /** the definitions' language, e.g. `en` */
  target_lang: string | null;
  term_count: number;
  /** kanji bank entries: single characters, e.g. KANJIDIC's; absent from an app built before */
  char_count?: number;
  /** pitch accent entries; absent from an app built before */
  pitch_count?: number;
  /** who made it and how to credit it, from its index.json; absent from an app built before */
  credits?: {
    author: string | null;
    /** its source, e.g. a project page */
    url: string | null;
    description: string | null;
    /** the credit it asks for */
    attribution: string | null;
  };
}

/** A list of names persisted per device under `key` */
function storedNames(key: string): Ref<string[]> {
  let init: string[] = [];
  try {
    const v: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    if (Array.isArray(v)) init = v.filter((s) => typeof s === "string");
  } catch {
    // unreadable or unavailable: none
  }
  const names = ref(init);
  watch(
    names,
    (v) => {
      try {
        localStorage.setItem(key, JSON.stringify(v));
      } catch {
        // storage unavailable: the choice just isn't remembered
      }
    },
    { deep: true },
  );
  return names;
}

/**
 * Dictionaries left out of searches (Words), by name; persisted per device. Kept as the
 * ones left out, so a dictionary added to the library later is searched.
 */
export const excludedDicts = storedNames("cjk-dict.excludedDicts");

/** Kanji dictionaries left out of the Characters view, by name, as `excludedDicts` */
export const excludedCharDicts = storedNames("cjk-dict.excludedCharDicts");

/** The downloaded dictionaries, in build order; empty until some are downloaded. */
export const dictList = ref<IDictInfo[]>([]);

export async function loadDicts() {
  try {
    dictList.value = await invoke<IDictInfo[]>("dicts");
  } catch (e) {
    // e.g. "no dictionaries downloaded yet" on first launch
    console.warn(e);
    dictList.value = [];
  }
}

/**
 * The dictionaries with words to choose from for `lang` (the search language, "" for
 * all): kanji dictionaries (no terms) are left out, and so are those of another
 * language. Pitch accent dictionaries are in, so they can be left out as in Yomitan. A
 * dictionary that doesn't say its language may have words of any.
 *
 * Ordered by language: those without one first, then by language tag (`ja-JP`, `ko-KR`,
 * `zh`, `zh-TW`), each language's in build order.
 */
export function wordDicts(list: IDictInfo[], lang: string): IDictInfo[] {
  return ofLang(
    list.filter((d) => d.term_count > 0 || (d.pitch_count ?? 0) > 0),
    lang,
  );
}

/**
 * The kanji dictionaries to choose from for `lang`, for the Characters view: those of
 * the search language, and those that don't say theirs, e.g. KANJIDIC, which has
 * Japanese, Chinese and Korean readings. Ordered as `wordDicts`.
 */
export function charDicts(list: IDictInfo[], lang: string): IDictInfo[] {
  return ofLang(
    list.filter((d) => (d.char_count ?? 0) > 0),
    lang,
  );
}

/** Whether a dictionary of `dictLang` (null: unsaid) is of the search language `lang` */
export function isOfLang(dictLang: string | null, lang: string): boolean {
  return !lang || !dictLang || primaryLang(dictLang) === primaryLang(lang);
}

function ofLang(list: IDictInfo[], lang: string): IDictInfo[] {
  return list
    .filter((d) => isOfLang(d.lang, lang))
    .sort((a, b) => (a.lang ?? "").localeCompare(b.lang ?? "", "en"));
}

/**
 * Whether every dictionary of `dicts` (those to choose from) is left out, e.g. after
 * switching the search language
 */
export function noneIncluded(dicts: IDictInfo[], excluded: string[]): boolean {
  return dicts.length > 0 && dicts.every((d) => excluded.includes(d.name));
}
