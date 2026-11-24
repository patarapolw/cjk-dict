import { describe, expect, it } from "vitest";

import { type IDictInfo, charDicts, noneIncluded, wordDicts } from "./dicts";

const dict = (name: string, lang: string | null, term_count = 10) =>
  ({
    name,
    title: name,
    revision: "1",
    lang,
    target_lang: "en",
    term_count,
  }) satisfies IDictInfo;

const names = (list: IDictInfo[]) => list.map((d) => d.name);

describe("wordDicts", () => {
  const list = [
    dict("ZhWiki", "zh"),
    dict("Kanji", "ja-JP", 0),
    dict("JaA", "ja-JP"),
    dict("MyDict", null),
    dict("KoA", "ko-KR"),
    dict("ZhTw", "zh-TW"),
    dict("JaB", "ja-JP"),
  ];

  it("orders by language, those without one first, each in build order", () => {
    expect(names(wordDicts(list, ""))).toEqual([
      "MyDict",
      "JaA",
      "JaB",
      "KoA",
      "ZhWiki",
      "ZhTw",
    ]);
  });

  it("lists pitch accent dictionaries, to be left out as in Yomitan", () => {
    const pitch = { ...dict("Pitch", "ja-JP", 0), pitch_count: 5 };
    expect(names(wordDicts([...list, pitch], "ja-JP"))).toEqual([
      "MyDict",
      "JaA",
      "JaB",
      "Pitch",
    ]);
    expect(names(wordDicts([...list, pitch], "zh"))).not.toContain("Pitch");
  });

  it("lists the search language's, and those without a language", () => {
    expect(names(wordDicts(list, "zh"))).toEqual(["MyDict", "ZhWiki", "ZhTw"]);
    expect(names(wordDicts(list, "ja-JP"))).toEqual(["MyDict", "JaA", "JaB"]);
  });
});

describe("charDicts", () => {
  const kanji = (name: string, lang: string | null, char_count = 10) => ({
    ...dict(name, lang, 0),
    char_count,
  });
  const list = [
    dict("Words", "ja-JP"),
    kanji("KANJIDIC", null),
    kanji("Hanzi", "zh"),
    kanji("Hanja", "ko-KR"),
    { ...dict("Both", "ja-JP"), char_count: 3 },
  ];

  it("lists the kanji dictionaries of the search language, and those of none", () => {
    expect(names(charDicts(list, ""))).toEqual([
      "KANJIDIC",
      "Both",
      "Hanja",
      "Hanzi",
    ]);
    expect(names(charDicts(list, "zh"))).toEqual(["KANJIDIC", "Hanzi"]);
    expect(names(charDicts(list, "ja-JP"))).toEqual(["KANJIDIC", "Both"]);
    // from an app built before char_count: none
    expect(charDicts([dict("Old", null, 0)], "")).toEqual([]);
  });
});

describe("noneIncluded", () => {
  it("is true only when every dictionary listed is left out", () => {
    const list = [dict("A", null), dict("B", null)];
    expect(noneIncluded(list, ["A"])).toBe(false);
    expect(noneIncluded(list, ["A", "B", "C"])).toBe(true);
    expect(noneIncluded([], [])).toBe(false);
  });
});
