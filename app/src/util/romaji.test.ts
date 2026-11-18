import { describe, expect, it } from "vitest";

import { kanaRow, romajiTailGlob } from "./romaji";

describe("kanaRow", () => {
  it("gives every kana an incomplete syllable can become, by extended rules", () => {
    expect(kanaRow("ky")).toEqual(["きぃ", "きぇ", "きゃ", "きゅ", "きょ"]);
    expect(kanaRow("kw")).toEqual(["くぁ", "くぃ", "くぅ", "くぇ", "くぉ"]);
    expect(kanaRow("n")).toEqual(expect.arrayContaining(["な", "にゃ", "ん"]));
    // `kk` doubles the consonant
    expect(kanaRow("k")).toEqual(
      expect.arrayContaining(["か", "きゃ", "くぁ", "っか", "っきゃ"]),
    );
    expect(kanaRow("q")).toEqual(
      expect.arrayContaining(["くぁ", "くゃ", "っくぁ"]),
    );
    expect(kanaRow("x")).toEqual(expect.arrayContaining(["ぁ", "っ", "ゎ"]));
  });

  it("includes obsolete kana as well as modern ones", () => {
    expect(kanaRow("w")).toEqual(
      expect.arrayContaining(["ゐ", "うぃ", "ゑ", "うぇ", "を"]),
    );
  });

  it("is katakana in upper case", () => {
    expect(kanaRow("KY")).toEqual(["キィ", "キェ", "キャ", "キュ", "キョ"]);
  });

  it("is empty for letters that start no syllable", () => {
    expect(kanaRow("st")).toEqual([]);
    expect(kanaRow("kq")).toEqual([]);
  });
});

describe("romajiTailGlob", () => {
  it("replaces trailing romaji with its kana row", () => {
    expect(romajiTailGlob("にほんg")).toBe("にほん[がぎぐげごっ]");
    expect(romajiTailGlob("にほn")).toBe("にほ[なにぬねのん]");
    expect(romajiTailGlob("k")).toBe("[かきくけこっ]");
    expect(romajiTailGlob("とky")).toBe("とき[ぃぇゃゅょ]");
    expect(romajiTailGlob("とkw")).toBe("とく[ぁぃぅぇぉ]");
    expect(romajiTailGlob("いっs")).toBe("いっ[さしすせそっ]");
  });

  it("has a class per character as far as the row's shortest kana", () => {
    expect(romajiTailGlob("わたsh")).toBe("わたし");
    expect(romajiTailGlob("とq")).toBe("と[くっ][ぁぃぅぇぉくゃゅょ]");
  });

  it("is null without trailing romaji that starts a syllable", () => {
    expect(romajiTailGlob("にほん")).toBeNull();
    expect(romajiTailGlob("てst")).toBeNull();
    // pinyin, or other words with vowels
    expect(romajiTailGlob("zhong")).toBeNull();
    expect(romajiTailGlob("ni3 hao")).toBeNull();
  });
});
