import { invoke } from "@tauri-apps/api/core";
import { ref, watch } from "vue";

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

/**
 * Dictionaries left out of searches, by name; persisted per device. Kept as the ones left
 * out, so a dictionary added to the library later is searched.
 */
const KEY = "cjk-dict.excludedDicts";

function read(): string[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((s) => typeof s === "string") : [];
  } catch {
    return [];
  }
}

export const excludedDicts = ref<string[]>(read());

watch(
  excludedDicts,
  (v) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(v));
    } catch {
      // storage unavailable: the choice just isn't remembered
    }
  },
  { deep: true },
);

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
  return list
    .filter(
      (d) =>
        (d.term_count > 0 || (d.pitch_count ?? 0) > 0) &&
        (!lang || !d.lang || primaryLang(d.lang) === primaryLang(lang)),
    )
    .sort((a, b) => (a.lang ?? "").localeCompare(b.lang ?? "", "en"));
}

/** Search `name`, or leave it out. */
export function setDictIncluded(name: string, included: boolean) {
  const rest = excludedDicts.value.filter((n) => n !== name);
  excludedDicts.value = included ? rest : [...rest, name];
}
