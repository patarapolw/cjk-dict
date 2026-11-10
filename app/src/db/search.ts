import { convertFileSrc, invoke } from "@tauri-apps/api/core";

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
  reading: string;
  limit: number;
  offset: number;
  type: string | null;
}

export interface ISearchTerm {
  id: number;
  term: string;
}

export interface ITerm {
  dict_id: number;
  /** dictionary name on the server */
  dict_path: string;
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

const ALL_LANGS = ["ja-JP", "zh-CN", "ko-KR"];

class SearchDatabase {
  async search(opts: SearchOpts): Promise<SearchResult> {
    const start = new Date();
    try {
      const lang = langSetting.value;

      let type = opts.type;
      let tokens: Record<string, string> | undefined;
      if (type === "fts") {
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
        tokens,
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

  /** URL of a media file inside a dictionary zip, for a plain `<img src>` (dict:// protocol). */
  assetSrc(dict: string, path: string): string {
    return convertFileSrc(`${dict}/${path}`, "dict");
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
