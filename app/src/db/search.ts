import { invoke } from "@tauri-apps/api/core";
import { fetch } from "@tauri-apps/plugin-http";

import { settingsState } from "../util/settings";

/**
 * Dictionary search is served by the cjk-dict worker (Cloudflare D1 + R2).
 * The worker expands pinyin readings. The client only tokenizes "~" (fts) queries locally
 * (lindera), since the tokenizer dictionaries are too large for a worker.
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

export interface IDict {
  id: number;
  name: string;
  title: string;
  revision: string | null;
  L1: string | null;
  L2: string | null;
  term_count: number;
}

type SearchResult = {
  items: (ISearchTerm & { entries: ITerm[] })[];
  next: number | null;
};

const ALL_LANGS = ["ja-JP", "zh-CN", "ko-KR"];

class SearchDatabase {
  get baseURL() {
    const url = settingsState.computed.dictApiURL.value;
    if (!url) {
      throw new Error("Dictionary server URL is not set (Settings → Dictionary)");
    }
    return url.replace(/\/+$/, "");
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const r = await fetch(this.baseURL + path, init);
    if (!r.ok) {
      throw new Error(`${init?.method || "GET"} ${path}: ${r.status} ${await r.text()}`);
    }
    return r.json();
  }

  async dicts() {
    return this.request<IDict[]>("/dicts");
  }

  async search(opts: SearchOpts): Promise<SearchResult> {
    const start = new Date();
    try {
      const lang = settingsState.computed.lang.value;

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

      return await this.request<SearchResult>("/search", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    } finally {
      const timeTaken = (+new Date() - +start) / 1000;
      if (timeTaken > 0.5) {
        console.log(
          `Searching ${JSON.stringify(opts)} takes ${timeTaken.toPrecision(2)} seconds`,
        );
      }
    }
  }

  /** URL of a media file (structured-content images), for a plain `<img src>`. */
  assetSrc(dict: string, path: string): string {
    return `${this.baseURL}/dicts/${encodeURIComponent(dict)}/assets/${path
      .split("/")
      .map(encodeURIComponent)
      .join("/")}`;
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
