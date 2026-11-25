import { describe, expect, it } from "vitest";

import {
  toneMarks,
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
    expect(sortIds(ids, "zh")[0].ids).toBe("⿹②一");
    // Chinese: Taiwan's too
    const tw = [ids[1], { ids: "⿰②一", regions: "T" }];
    expect(sortIds(tw, "zh")[0].ids).toBe("⿰②一");
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
  });

  it("searches a suffix or prefix with a character before or after it", () => {
    // KANJIDIC's dash: other characters go there
    expect(readingQuery("日", "-び")).toBe("*?<日>{び}");
    expect(readingQuery("果", "-は.たす")).toBe("*?<果>{は}");
    expect(readingQuery("大", "おお-")).toBe("<大>{おお}?*");
  });

  it("tells a reading's language by its script", () => {
    expect(readingLang("キョウ")).toBe("ja-JP");
    expect(readingLang("おこ.る")).toBe("ja-JP");
    expect(readingLang("흥")).toBe("ko-KR");
    expect(readingLang("xīng")).toBe("zh");
  });
});

describe("toneMarks", () => {
  it("puts the tone mark on a or e, else the o of ou, else the last vowel", () => {
    expect(toneMarks("xing1")).toBe("xīng");
    expect(toneMarks("hao3")).toBe("hǎo");
    expect(toneMarks("xie4")).toBe("xiè");
    expect(toneMarks("gou3")).toBe("gǒu");
    expect(toneMarks("gui4")).toBe("guì");
    expect(toneMarks("liu2")).toBe("liú");
  });

  it("writes ü for u: and v, and leaves the neutral tone unmarked", () => {
    expect(toneMarks("lu:e4")).toBe("lüè");
    expect(toneMarks("nv3")).toBe("nǚ");
    expect(toneMarks("ma5")).toBe("ma");
  });

  it("keeps capitals, and pinyin already marked or without a number", () => {
    expect(toneMarks("Xing1")).toBe("Xīng");
    expect(toneMarks("xīng")).toBe("xīng");
    expect(toneMarks("xing")).toBe("xing");
  });
});
