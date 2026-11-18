import { describe, expect, it } from "vitest";

import type { ITerm } from "../db/search";
import {
  glossLang,
  glossaryTermLang,
  headLang,
  refineHeadLangs,
  termLang,
} from "./lang";
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

  it("ignores a script without the other one next to it", () => {
    // CC-CEDICT's 直: the same in both scripts, written once
    const g = [
      {
        type: "structured-content",
        content: [
          {
            tag: "div",
            content: [
              "【",
              [{ tag: "span", content: "直", lang: "zh-Hans" }],
              "】",
            ],
          },
          // a reference in the definitions, in another script
          {
            tag: "ul",
            content: [
              {
                tag: "li",
                content: [{ tag: "span", content: "炎帝", lang: "zh" }],
              },
            ],
          },
        ],
      },
    ];
    expect(glossaryTermLang("直", g, "zh")).toBeNull();
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
    // Pixiv: no L1, but its target language is CJK
    expect(termLang(row({ term: "初音ミク", target_lang: "ja" }))).toBe("ja");
    // KANJIDIC: neither
    expect(termLang(row({ term: "直", target_lang: "en" }), "ko-KR")).toBe(
      "ko-KR",
    );
    expect(termLang(row({ term: "直", target_lang: "en" }))).toBeUndefined();
  });
});

describe("headLang", () => {
  const glossary = JSON.stringify(cedict("學習", "学习"));
  const wty = row({ term: "学习", lang: "zh" });
  const cedictRow = row({ term: "学习", lang: "zh", glossary });

  it("prefers an entry that names the term's script", () => {
    expect(headLang([wty, cedictRow])).toBe("zh-Hans");
  });

  it("prefers entries in the search language", () => {
    const ja = row({ term: "学习", lang: "ja-JP" });
    expect(headLang([ja, cedictRow], "ja-JP")).toBe("ja-JP");
    expect(headLang([ja, cedictRow], "zh")).toBe("zh-Hans");
    expect(headLang([ja, wty], "ko-KR")).toBe("ja-JP");
    expect(headLang([row({ term: "x" })], "ko-KR")).toBe("ko-KR");
    expect(headLang([])).toBeUndefined();
  });
});

describe("refineHeadLangs", () => {
  it("gives a bare language the script another result names", () => {
    const items = [
      { term: "学习", lang: "zh", declared: null },
      { term: "学习", lang: "zh-Hans", declared: "zh-Hans" },
      { term: "学习144号文件", lang: "zh", declared: null },
      { term: "学习", lang: "ja", declared: null },
    ];
    expect(
      refineHeadLangs(
        items,
        (i) => i.lang,
        (i) => i.term,
        (i) => i.declared,
      ),
    ).toEqual(["zh-Hans", "zh-Hans", "zh", "ja"]);
  });
});

describe("glossLang", () => {
  it("is the dictionary's L2 if CJK, else its L1", () => {
    // Pixiv
    expect(glossLang({ lang: null, target_lang: "ja" })).toBe("ja");
    expect(glossLang({ lang: "ko-KR", target_lang: "ko" })).toBe("ko");
    // wty-ja-en: Han characters in English definitions are Japanese
    expect(glossLang({ lang: "ja-JP", target_lang: "en" })).toBe("ja-JP");
    expect(glossLang({ lang: "zh", target_lang: null })).toBe("zh");
    // KANJIDIC: no L1
    expect(glossLang({ lang: null, target_lang: "en" })).toBe("en");
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
