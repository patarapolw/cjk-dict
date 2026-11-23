import { describe, expect, it } from "vitest";

import { type IDictInfo, wordDicts } from "./dicts";

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
