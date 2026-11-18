import { describe, expect, it } from "vitest";

import type { ITerm } from "../db/search";
import { glossLang, glossaryTermLang, termLang } from "./lang";
import { normalizeLang } from "./settings";

/** CC-CEDICT's glossary: the headword in both scripts, each with its `lang` */
const cedict = (trad: string, simp: string) => [
  {
    type: "structured-content",
    content: [
      {
        tag: "div",
        lang: "en",
        content: [
          {
            tag: "div",
            content: [
              "【",
              [
                { tag: "span", content: trad, lang: "zh-Hant" },
                "・",
                { tag: "span", content: simp, lang: "zh-Hans" },
              ],
              "】",
            ],
          },
          { tag: "ul", content: [{ tag: "li", content: ["to learn"] }] },
        ],
      },
    ],
  },
];

const row = (o: Partial<ITerm>): ITerm => ({
  dict_id: 1,
  dict_path: "d",
  term_id: 1,
  term: "",
  reading: "",
  def_tags: "",
  rules: "",
  score: null,
  glossary: "[]",
  sequence: null,
  tags: "",
  ...o,
});

describe("glossaryTermLang", () => {
  it("finds the script a dictionary writes the term in", () => {
    const g = cedict("學習", "学习");
    expect(glossaryTermLang("學習", g, "zh")).toBe("zh-Hant");
    expect(glossaryTermLang("学习", g, "zh")).toBe("zh-Hans");
    expect(glossaryTermLang("学", g, "zh")).toBeNull();
  });

  it("ignores another language than the dictionary's", () => {
    const g = cedict("學習", "学习");
    expect(glossaryTermLang("學習", g, "ja-JP")).toBeNull();
    // the definitions' language is not the term's
    expect(glossaryTermLang("to learn", g, null)).toBeNull();
  });
});

describe("termLang", () => {
  it("prefers the glossary, then the dictionary's L1, then the fallback", () => {
    const glossary = JSON.stringify(cedict("學習", "学习"));
    expect(termLang(row({ term: "學習", glossary, lang: "zh" }))).toBe(
      "zh-Hant",
    );
    expect(termLang(row({ term: "学", lang: "zh" }), "ja-JP")).toBe("zh");
    // Pixiv: no L1
    expect(termLang(row({ term: "初音ミク" }), "ja-JP")).toBe("ja-JP");
    expect(termLang(row({ term: "初音ミク" }))).toBeUndefined();
  });
});

describe("glossLang", () => {
  it("is the dictionary's L2", () => {
    expect(glossLang({ target_lang: "ja" })).toBe("ja");
    expect(glossLang({ target_lang: null })).toBeUndefined();
  });
});

describe("normalizeLang", () => {
  it("makes any Chinese setting zh", () => {
    expect(normalizeLang("zh-CN")).toBe("zh");
    expect(normalizeLang("zh-TW")).toBe("zh");
    expect(normalizeLang("ja-JP")).toBe("ja-JP");
    expect(normalizeLang("")).toBe("");
  });
});
