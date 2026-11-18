import { describe, expect, it } from "vitest";

import { kanaRow, romajiTailGlob } from "./romaji";

describe("kanaRow", () => {
  it("gives the kana row an incomplete syllable starts", () => {
    expect(kanaRow("k")).toEqual(["か", "き", "く", "け", "こ"]);
    expect(kanaRow("n")).toEqual(["な", "に", "ぬ", "ね", "の", "ん"]);
    expect(kanaRow("ky")).toEqual(["きゃ", "きぃ", "きゅ", "きぇ", "きょ"]);
  });

  it("is katakana in upper case", () => {
    expect(kanaRow("K")).toEqual(["カ", "キ", "ク", "ケ", "コ"]);
  });

  it("is empty for letters that start no syllable", () => {
    expect(kanaRow("st")).toEqual([]);
  });
});

describe("romajiTailGlob", () => {
  it("replaces trailing romaji with its kana row", () => {
    expect(romajiTailGlob("にほんg")).toBe("にほん[がぎぐげご]");
    expect(romajiTailGlob("にほn")).toBe("にほ[なにぬねのん]");
    expect(romajiTailGlob("k")).toBe("[かきくけこ]");
    expect(romajiTailGlob("とky")).toBe("とき[ゃぃゅぇょ]");
    expect(romajiTailGlob("がっk")).toBe("がっ[かきくけこ]");
    expect(romajiTailGlob("かk")).toBe("か[かきくけこ]");
    expect(romajiTailGlob("いっs")).toBe("いっ[さしすせそ]");
  });

  it("keeps only the first kana when the row's kana differ in length", () => {
    expect(romajiTailGlob("わたsh")).toBe("わたし");
  });

  it("is null without trailing romaji that starts a syllable", () => {
    expect(romajiTailGlob("にほん")).toBeNull();
    expect(romajiTailGlob("てst")).toBeNull();
  });
});
