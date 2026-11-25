import { convertFileSrc, invoke } from "@tauri-apps/api/core";

import { IPitchEntry } from "../util/pitch";
import { lang as langSetting } from "../util/settings";

/**
 * Search runs on the device: `search.db` (downloaded index) is queried by the Rust `search`
 * command, which reads the entries themselves from the Yomitan zips. Pinyin
 * readings are expanded there too. Only "~" (fts) queries are tokenized here first (lindera).
 *
 * @see https://github.com/patarapolw/cjk-dict
 */

interface SearchOpts {
  term: string;
  /** reading-only (or pinyin) search */
  reading: string;
  /** kana reading pattern for `term` (furigana syntax), e.g. `にほん*` */
  readingGlob: string;
  /** `term` may be Korean romanization: `hangugeo` finds 한국어 */
  romanized?: boolean;
  /** with `chReading`: the words with this character read so anywhere (`*<興>{キョウ}`) */
  ch?: string;
  chReading?: string;
  /** dictionary names to leave out */
  excludeDicts?: string[];
  limit: number;
  offset: number;
  type: string | null;
}

export interface ISearchTerm {
  id: number;
  term: string;
  /** space-padded; for Korean, generated from Hanja (光速: ` 광속 `) */
  reading: string;
  /**
   * the pitch accent dictionaries' entries for its entries' terms and readings, as Yomitan
   * finds them: the same term and reading (the term, for an entry without one)
   */
  pitches?: IPitchEntry[];
}

export interface ITerm {
  dict_id: number;
  /** dictionary name on the server */
  dict_path: string;
  /** the dictionary's source language (L1), the terms', e.g. `ja-JP` or `zh` */
  lang?: string | null;
  /** the dictionary's target language (L2), the definitions', e.g. `en` */
  target_lang?: string | null;
  term_id: number;

  term: string;
  reading: string;
  def_tags: string;
  rules: string;
  score: number | null;
  glossary: string;
  sequence: number | null;
  tags: string;
}

type SearchResult = {
  items: (ISearchTerm & { entries: ITerm[] })[];
  next: number | null;
};

/** One way to write a character, from the CJKVI IDS database: `⿰木木` for 林 */
export interface IIds {
  ids: string;
  /** region letters the IDS applies to, e.g. `GTKV`; empty for all */
  regions: string;
}

/** A kanji dictionary's note on a tag or stat, from its tag banks */
export interface ITagMeta {
  category: string;
  order: number;
  notes: string;
}

/** A kanji dictionary entry (Yomitan kanji bank), e.g. KANJIDIC's */
export interface IKanjiEntry {
  dict_id: number;
  dict_path: string;
  /** the dictionary's source language, e.g. `ja-JP` */
  lang: string | null;
  /** the dictionary's target language, the meanings', e.g. `en` */
  target_lang?: string | null;
  character: string;
  /** space-separated */
  onyomi: string;
  /** space-separated */
  kunyomi: string;
  /** space-separated */
  tags: string;
  meanings: string[];
  /** [name, value] by name */
  stats: [string, string][];
  tag_meta: Record<string, ITagMeta>;
}

export interface IHanChar {
  ch: string;
  /** empty if the library has no decomposition for it */
  ids: IIds[];
  /** what the IDS name directly */
  components: string[];
  /** every component, broken down all the way */
  all_components: string[];
  /** Hangul readings (eum) */
  hangul: string[];
  /** terms with the character: of the search language asked for, or in all */
  uses: number;
  /** its variant forms, from Unihan: 学's 學 (traditional, kyujitai) */
  variants: IHanVariant[];
  entries: IKanjiEntry[];
}

/** How a variant relates to its character, in this order (yomitan-parser's `VariantKind`) */
export type VariantKind =
  | "traditional"
  | "simplified"
  | "kyujitai"
  | "shinjitai"
  | "semantic"
  | "other";

export interface IHanVariant {
  ch: string;
  /** every way it relates to the character */
  kinds: VariantKind[];
}

export interface IHanList {
  items: { ch: string; uses: number }[];
  next: number | null;
  /** how many there are in all, on every page */
  total: number;
}

/** How a reading in Latin letters is read: as pinyin, as Korean romanization, or both */
export interface ILatin {
  pinyin: boolean;
  korean: boolean;
}

const ALL_LANGS = ["ja-JP", "zh", "ko-KR"];

class SearchDatabase {
  async search(opts: SearchOpts): Promise<SearchResult> {
    const start = new Date();
    try {
      const lang = langSetting.value;

      let type = opts.type;
      let tokens: Record<string, string> | undefined;
      // romanized Korean is matched by prefix instead, without tokens
      if (type === "fts" && !opts.romanized) {
        tokens = await this.tokens(opts.term, lang);
        if (opts.term && !Object.keys(tokens).length) {
          // tokenizer not downloaded (yet) or failed: fall back to prefix search
          type = null;
          tokens = undefined;
        }
      }

      const body = {
        type,
        lang,
        term: opts.term,
        reading: opts.reading,
        reading_glob: opts.readingGlob,
        romanized: opts.romanized ?? false,
        ch: opts.ch ?? "",
        ch_reading: opts.chReading ?? "",
        tokens,
        exclude_dicts: opts.excludeDicts ?? [],
        limit: opts.limit,
        offset: opts.offset,
      };

      return await invoke<SearchResult>("search", { req: body });
    } finally {
      const timeTaken = (+new Date() - +start) / 1000;
      if (timeTaken > 0.5) {
        console.log(
          `Searching ${JSON.stringify(opts)} takes ${timeTaken.toPrecision(2)} seconds`,
        );
      }
    }
  }

  /** The Han characters of `text`: how each breaks down, and its kanji entries. */
  async hanChars(text: string, lang = ""): Promise<IHanChar[]> {
    return await invoke<IHanChar[]>("han_chars", {
      text,
      lang: lang || undefined,
    });
  }

  /**
   * Characters made of every character of `components`, the most used first. With
   * `direct`, only those whose IDS name the components themselves: 口 gives 可 but not 河,
   * which is made of 可.
   */
  async hanContaining(
    components: string,
    limit: number,
    offset: number,
    direct = false,
    lang = "",
  ): Promise<IHanList> {
    return await invoke<IHanList>("han_containing", {
      components,
      limit,
      offset,
      direct,
      lang: lang || undefined,
    });
  }

  /**
   * Characters read any of `readings`, the most used first: kana (`きょう`; a kun reading
   * with or without its okurigana, `おこ` or `おこる`), pinyin (`xing`, every tone without
   * one) or Hangul (`흥`). Latin letters are read as `latin` says: pinyin, Korean
   * romanization (`heung`) as Words matches it, or both.
   */
  async hanByReading(
    readings: string[],
    latin: ILatin,
    limit: number,
    offset: number,
    lang = "",
  ): Promise<IHanList> {
    return await invoke<IHanList>("han_by_reading", {
      readings,
      ...latin,
      limit,
      offset,
      lang: lang || undefined,
    });
  }

  /** URL of a media file inside a dictionary zip, for a plain `<img src>` (dict:// protocol). */
  assetSrc(dict: string, path: string): string {
    return convertFileSrc(`${dict}/${path}`, "dict");
  }

  /** Surface forms of `text`'s words, or [] if the tokenizer isn't available (yet). */
  async segment(text: string, lang: string): Promise<string[]> {
    try {
      const segs = await invoke<[string, string[]][]>("segment", {
        lang,
        text,
      });
      return segs.map(([surface]) => surface).filter((s) => s.trim());
    } catch (e) {
      console.warn(`segment ${lang} unavailable`, e);
      return [];
    }
  }

  private async tokens(term: string, lang: string) {
    if (!term) return {};
    const kvs = await Promise.all(
      (lang ? [lang] : ALL_LANGS).map((subLang) =>
        invoke<string[]>("tokenize", { lang: subLang, text: term }).then(
          (ts) => [subLang.split("-")[0], ts.join(" ")] as const,
          (e) => {
            console.warn(`tokenize ${subLang} unavailable`, e);
            return [subLang.split("-")[0], ""] as const;
          },
        ),
      ),
    );
    return Object.fromEntries(kvs.filter(([, v]) => v));
  }
}

export const searchDB = new SearchDatabase();
