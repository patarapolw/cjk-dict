import { describe, expect, it } from "vitest";

import { parseQuery, toFurigana } from "./furigana";

describe("parseQuery", () => {
  it("reads furigana syntax as a term with a reading pattern", () => {
    expect(parseQuery("<日本>{にほん}語")).toEqual({
      term: "日本語",
      reading: "",
      readingGlob: "にほん*",
      rest: "",
    });
  });

  it("searches by reading only for {reading} alone", () => {
    expect(parseQuery("{にほんご}")).toEqual({
      term: "",
      reading: "にほんご",
      readingGlob: "",
      rest: "",
    });
    // pinyin readings contain spaces
    expect(parseQuery("{ni3 hao3}")).toMatchObject({
      term: "",
      reading: "ni3 hao3",
      rest: "",
    });
  });

  it("accepts full-width brackets, as typed with a Japanese IME", () => {
    expect(parseQuery("＜日本＞｛にほん｝語")).toEqual(
      parseQuery("<日本>{にほん}語"),
    );
  });

  it("splits at the first space outside {}", () => {
    expect(parseQuery("日本語 の 勉強")).toEqual({
      term: "日本語",
      reading: "",
      readingGlob: "",
      rest: "の 勉強",
    });
    expect(parseQuery("{ni3 hao3} 世界")).toMatchObject({
      reading: "ni3 hao3",
      rest: "世界",
    });
  });

  it("keeps wildcards in the reading pattern", () => {
    expect(parseQuery("<日本>{に*}語").readingGlob).toBe("に*");
    expect(parseQuery("<日本>{に?ん}語").readingGlob).toBe("に?ん*");
    // a reading-only search has no glob: wildcards split it into tokens
    expect(parseQuery("{に*ご}")).toMatchObject({ reading: "に ご" });
  });

  it("has no reading for a plain term, or a reading of only wildcards", () => {
    expect(parseQuery("日本語")).toMatchObject({
      term: "日本語",
      reading: "",
      readingGlob: "",
    });
    expect(parseQuery("たべる")).toMatchObject({ term: "たべる", reading: "" });
    expect(parseQuery("<日本>{*}語")).toMatchObject({
      term: "日本語",
      reading: "",
      readingGlob: "",
    });
  });

  it("matches a non-kana reading (Hangul) as reading tokens", () => {
    expect(parseQuery("<光>{광}나다")).toMatchObject({
      term: "光나다",
      reading: "광나다",
      readingGlob: "",
    });
  });

  it("reads full-width wildcards as typed with a Japanese IME", () => {
    expect(parseQuery("＊＜興＞｛＊キョウ｝")).toMatchObject({
      term: "*興",
      readingGlob: "*キョウ",
    });
    expect(parseQuery("［日月］曜日")).toMatchObject({ term: "[日月]曜日" });
  });

  it("reads *<興>{キョウ} as a character's reading", () => {
    expect(parseQuery("*<興>{キョウ}")).toMatchObject({
      ch: "興",
      chReading: "キョウ",
    });
    expect(parseQuery("*<興>{キョウ}*")).toMatchObject({ ch: "興" });
    expect(parseQuery("*<興>{xīng}")).toMatchObject({
      ch: "興",
      chReading: "xīng",
    });
    // not one character, or a pattern: wildcards
    expect(parseQuery("*<興味>{きょうみ}").ch).toBeUndefined();
    expect(parseQuery("*<興>{キョ*}").ch).toBeUndefined();
    expect(parseQuery("<興>{キョウ}").ch).toBeUndefined();
    expect(parseQuery("*<興>{おこ}る").ch).toBeUndefined();
  });

  it("matches a reading anywhere in the word after a *", () => {
    expect(parseQuery("*<興>{キョウ}")).toMatchObject({
      term: "*興",
      reading: "",
      readingGlob: "*キョウ",
    });
    expect(parseQuery("*<興>{おこ}る")).toMatchObject({
      term: "*興る",
      readingGlob: "*おこる",
    });
    // Hangul too, as a pattern over the reading, not a reading token
    expect(parseQuery("*<興>{흥}")).toMatchObject({
      term: "*興",
      reading: "",
      readingGlob: "*흥",
    });
    // pinyin: per-syllable reading tokens already match anywhere
    expect(parseQuery("*<興>{xīng}")).toMatchObject({
      term: "*興",
      reading: "xīng",
      readingGlob: "",
    });
  });
});

describe("toFurigana", () => {
  it("puts furigana over the kanji only, keeping okurigana", () => {
    expect(toFurigana("食べる", "たべる")).toBe("<食>{た}べる");
    expect(toFurigana("日本語", "にほんご")).toBe("<日本語>{にほんご}");
    expect(toFurigana("取り扱い", "とりあつかい")).toBe(
      "<取>{と}り<扱>{あつか}い",
    );
  });

  it("keeps a katakana reading as given", () => {
    expect(toFurigana("食べる", "タベル")).toBe("<食>{タ}べる");
  });

  it("leaves text without kanji, or without a reading, unchanged", () => {
    expect(toFurigana("たべる", "たべる")).toBe("たべる");
    expect(toFurigana("テスト", "てすと")).toBe("テスト");
    expect(toFurigana("食べる", "")).toBe("食べる");
  });

  it("wraps the whole text when the reading doesn't fit the okurigana", () => {
    expect(toFurigana("食べる", "のむ")).toBe("<食べる>{のむ}");
  });
});

describe("parseQuery with kanaRow", () => {
  const parse = (q: string) => parseQuery(q, { kanaRow: true });

  it("searches the kana row of incomplete romaji at the end of the term", () => {
    expect(parse("にほんg")).toEqual({
      term: "にほん[がぎぐげご]",
      reading: "",
      readingGlob: "",
      rest: "",
      incomplete: true,
    });
    expect(parse("<日本>{にほん}g")).toMatchObject({
      term: "日本[がぎぐげご]",
      readingGlob: "にほん*",
      incomplete: true,
    });
  });

  it("searches the kana row of incomplete romaji at the end of the reading", () => {
    // an unclosed `{` is a reading too
    expect(parse("{にほn")).toEqual({
      term: "",
      reading: "にほ[なにぬねのん]",
      readingGlob: "",
      rest: "",
      incomplete: true,
    });
    expect(parse("<日本>{にほn")).toEqual({
      term: "日本",
      reading: "",
      readingGlob: "にほ[なにぬねのん]",
      rest: "",
      incomplete: true,
    });
    expect(parse("<日本>{にほn}")).toMatchObject({
      readingGlob: "にほ[なにぬねのん]",
    });
  });

  it("expands trailing romaji before every }", () => {
    // okurigana or other characters after the reading
    expect(parse("<食>{たb}べる")).toMatchObject({
      term: "食べる",
      readingGlob: "た[ばびぶべぼ]べる",
      incomplete: true,
    });
    expect(parse("<日本>{にほn}語")).toMatchObject({
      term: "日本語",
      readingGlob: "にほ[なにぬねのん]*",
      incomplete: true,
    });
    expect(parse("{にほn}{ご}")).toMatchObject({
      reading: "にほ[なにぬねのん]ご",
      incomplete: true,
    });
  });

  it("expands the term's trailing romaji only at the end of the word", () => {
    // romaji in the base, not at the end of the word
    expect(parse("<日本g>{にほん}")).not.toHaveProperty("incomplete");
    // the term after a complete reading
    expect(parse("<食>{た}b")).toMatchObject({
      term: "食[ばびぶべぼ]",
      incomplete: true,
    });
  });

  it("leaves complete queries, pinyin and other options alone", () => {
    expect(parse("にほん")).not.toHaveProperty("incomplete");
    expect(parse("{ni3 hao}")).toMatchObject({ reading: "ni3 hao" });
    expect(parseQuery("にほんg")).toMatchObject({ term: "にほんg" });
  });
});
