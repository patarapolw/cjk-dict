import { describe, expect, it } from "vitest";

import {
  hanChars,
  idsTokens,
  readingLang,
  readingQuery,
  sortByLang,
  sortIds,
} from "./han";

describe("hanChars", () => {
  it("keeps distinct Han characters in order", () => {
    expect(hanChars("日本語の勉強、日本")).toEqual([
      "日",
      "本",
      "語",
      "勉",
      "強",
    ]);
    expect(hanChars("한국어 漢字 𠀋 氵⺡")).toEqual([
      "漢",
      "字",
      "𠀋",
      "氵",
      "⺡",
    ]);
    expect(hanChars("葛\u{E0100}城")).toEqual(["葛\u{E0100}", "城"]);
    expect(hanChars("kana かな")).toEqual([]);
  });
});

describe("idsTokens", () => {
  it("tells layout characters, components and unencoded components apart", () => {
    expect(idsTokens("⿰木木")).toEqual([
      { kind: "idc", text: "⿰", title: "left to right" },
      { kind: "component", text: "木" },
      { kind: "component", text: "木" },
    ]);
    expect(idsTokens("⿹②一&CDP-8C4E;").map((t) => t.kind)).toEqual([
      "idc",
      "unknown",
      "component",
      "unknown",
    ]);
    expect(idsTokens("⿹②一")[1]).toMatchObject({
      title: "a component with no code point, 2 strokes",
    });
  });
});

describe("sortIds", () => {
  const ids = [
    { ids: "⿹②一", regions: "GTKV" },
    { ids: "⿻②一", regions: "J" },
  ];
  it("puts the language's region first", () => {
    expect(sortIds(ids, "ja-JP")[0].ids).toBe("⿻②一");
    expect(sortIds(ids, "zh-CN")[0].ids).toBe("⿹②一");
    expect(sortIds(ids, "")).toBe(ids);
  });
});

describe("sortByLang", () => {
  it("puts the language's dictionaries first", () => {
    const es = [{ lang: "zh-CN" }, { lang: null }, { lang: "ja-JP" }];
    expect(sortByLang(es, "ja-JP").map((e) => e.lang)).toEqual([
      "ja-JP",
      "zh-CN",
      null,
    ]);
    expect(sortByLang(es, "")).toBe(es);
  });
});

describe("readingQuery", () => {
  it("searches words with the character read so, anywhere", () => {
    expect(readingQuery("興", "キョウ")).toBe("*<興>{キョウ}");
    expect(readingQuery("興", "xīng")).toBe("*<興>{xīng}");
    expect(readingQuery("興", "흥")).toBe("*<興>{흥}");
  });

  it("searches a kun reading up to its okurigana", () => {
    expect(readingQuery("興", "おこ.る")).toBe("*<興>{おこ}");
    expect(readingQuery("日", "-び")).toBe("*<日>{び}");
    expect(readingQuery("大", "おお-")).toBe("*<大>{おお}");
  });

  it("tells a reading's language by its script", () => {
    expect(readingLang("キョウ")).toBe("ja-JP");
    expect(readingLang("おこ.る")).toBe("ja-JP");
    expect(readingLang("흥")).toBe("ko-KR");
    expect(readingLang("xīng")).toBe("zh-CN");
  });
});
