import { describe, expect, it } from "vitest";

import {
  downsteps,
  groupPronunciations,
  headwordsOf,
  isVerbOrAdjective,
  morae,
  pitchKind,
  pitchMorae,
  pronunciationCategories,
  withoutDakuten,
} from "./pitch";

/** high morae in brackets, a drop as ↓ */
function draw(reading: string, position: number | string): string {
  return pitchMorae(reading, { position })
    .map((m) => (m.high ? `[${m.text}]` : m.text) + (m.drop ? "↓" : ""))
    .join("");
}

describe("morae", () => {
  it("joins small kana to the kana before", () => {
    expect(morae("きゃく")).toEqual(["きゃ", "く"]);
    expect(morae("ファッション")).toEqual(["ファ", "ッ", "ショ", "ン"]);
    expect(morae("コーヒー")).toEqual(["コ", "ー", "ヒ", "ー"]);
  });
});

describe("pitchMorae", () => {
  it("follows the downstep", () => {
    // 箸 [1], 橋 [2], 端 [0]
    expect(draw("はし", 1)).toBe("[は]↓し");
    expect(draw("はし", 2)).toBe("は[し]↓");
    expect(draw("はし", 0)).toBe("は[し]");
    expect(draw("にほんご", 0)).toBe("に[ほ][ん][ご]");
    expect(draw("おとうと", 4)).toBe("お[と][う][と]↓");
    expect(draw("みずうみ", 3)).toBe("み[ず][う]↓み");
    expect(draw("きゃく", 0)).toBe("きゃ[く]");
  });

  it("takes a pattern of H and L, whose last letter may be the particle's", () => {
    expect(draw("はし", "LHL")).toBe("は[し]↓");
    expect(draw("はし", "LHH")).toBe("は[し]");
    expect(draw("はし", "HL")).toBe("[は]↓し");
    expect(draw("はし", "lhl")).toBe("は[し]↓");
    expect(draw("はし", "2")).toBe("は[し]↓");
  });

  it("marks nasal and devoiced morae, counted from 1", () => {
    const ms = pitchMorae("かがく", { position: 1, nasal: 2, devoice: [1] });
    expect(ms.map((m) => [m.nasal, m.devoice])).toEqual([
      [false, true],
      [true, false],
      [false, false],
    ]);
    expect(withoutDakuten("が")).toBe("か");
    expect(withoutDakuten("ぎょ")).toBe("きょ");
    expect(withoutDakuten("か")).toBe("か");
  });
});

describe("downsteps", () => {
  it("reads them from a pattern", () => {
    expect(downsteps(2)).toEqual([2]);
    expect(downsteps("LHHL")).toEqual([3]);
    expect(downsteps("HLHL")).toEqual([1, 3]);
    expect(downsteps("LHH")).toEqual([0]);
    expect(downsteps("HHH")).toEqual([-1]);
  });
});

describe("pitchKind", () => {
  it("names the pattern", () => {
    expect(pitchKind("はし", 0)).toBe("heiban");
    expect(pitchKind("はし", 1)).toBe("atamadaka");
    expect(pitchKind("はし", 2)).toBe("odaka");
    expect(pitchKind("みずうみ", 3)).toBe("nakadaka");
    expect(pitchKind("はし", "LHL")).toBe("odaka");
  });

  it("calls a drop in a verb or i-adjective kifuku", () => {
    expect(isVerbOrAdjective("v5")).toBe(true);
    expect(isVerbOrAdjective("adj-i")).toBe(true);
    expect(isVerbOrAdjective("vs n")).toBe(false);
    expect(isVerbOrAdjective("")).toBe(false);
    expect(pitchKind("たべる", 2, true)).toBe("kifuku");
    expect(pitchKind("あそぶ", 0, true)).toBe("heiban");
  });
});

const entry = (
  dict_path: string,
  term: string,
  reading: string,
  ...pitches: { position: number; tags?: string[] }[]
) => ({ dict_id: 1, dict_path, term, reading, pitches, tag_meta: {} });

describe("headwordsOf", () => {
  it("reads an entry without a reading as written, each headword once", () => {
    expect(
      headwordsOf([
        { term: "箸", reading: "はし" },
        { term: "箸", reading: "はし" },
        { term: "コーヒー", reading: "" },
      ]),
    ).toEqual([
      { term: "箸", reading: "はし" },
      { term: "コーヒー", reading: "コーヒー" },
    ]);
  });
});

describe("groupPronunciations", () => {
  it("lists each dictionary's pitches together, the same pitch once", () => {
    const groups = groupPronunciations(
      [
        entry("A", "箸", "はし", { position: 1 }),
        entry("A", "箸", "はし", { position: 1 }, { position: 0 }),
        entry("B", "箸", "はし", { position: 1 }),
      ],
      [{ term: "箸", reading: "はし" }],
    );
    expect(
      groups.map((g) => [g.dict_path, g.pitches.map((p) => p.pitch.position)]),
    ).toEqual([
      ["A", [1, 0]],
      ["B", [1]],
    ]);
    expect(groups[0].pitches[0].exclusiveTerms).toEqual([]);
    expect(groups[0].pitches[0].exclusiveReadings).toEqual([]);
  });

  it("names the terms a pitch is for when it isn't for all of the word's", () => {
    const [g] = groupPronunciations(
      [
        entry("A", "日本", "にほん", { position: 2 }),
        entry("A", "日本", "にっぽん", { position: 3 }),
        entry("A", "ニッポン", "にっぽん", { position: 3 }),
      ],
      [
        { term: "日本", reading: "にほん" },
        { term: "日本", reading: "にっぽん" },
        { term: "ニッポン", reading: "にっぽん" },
      ],
    );
    expect(
      g.pitches.map((p) => [p.reading, p.exclusiveTerms, p.exclusiveReadings]),
    ).toEqual([
      ["にほん", ["日本"], ["にほん"]],
      ["にっぽん", [], ["にっぽん"]],
    ]);
  });

  it("takes katakana and hiragana spellings as one, as the app's results do", () => {
    // ちりちり from one dictionary, チリチリ from another: one result
    const [g] = groupPronunciations(
      [entry("A", "ちりちり", "ちりちり", { position: 0 })],
      headwordsOf([
        { term: "ちりちり", reading: "" },
        { term: "チリチリ", reading: "" },
      ]),
    );
    expect(g.pitches[0].exclusiveTerms).toEqual([]);
    expect(g.pitches[0].exclusiveReadings).toEqual([]);
    // the same pitch of ちりちり and チリチリ: once
    const [h] = groupPronunciations(
      [
        entry("A", "ちりちり", "ちりちり", { position: 0 }),
        entry("A", "チリチリ", "チリチリ", { position: 0 }),
      ],
      headwordsOf([
        { term: "ちりちり", reading: "" },
        { term: "チリチリ", reading: "" },
      ]),
    );
    expect(h.pitches).toHaveLength(1);
  });

  it("gives tags what the tag bank says", () => {
    const [g] = groupPronunciations(
      [
        {
          ...entry("A", "端", "はた", { position: 1, tags: ["名", "x"] }),
          tag_meta: {
            名: { category: "partOfSpeech", order: 0, notes: "noun" },
          },
        },
      ],
      [{ term: "端", reading: "はた" }],
    );
    expect(g.pitches[0].tags).toEqual([
      {
        name: "名",
        meta: { category: "partOfSpeech", order: 0, notes: "noun" },
      },
      { name: "x", meta: undefined },
    ]);
  });
});

describe("pronunciationCategories", () => {
  it("names the headword's patterns, as Yomitan does", () => {
    const entries = [
      entry("A", "端", "はし", { position: 0 }, { position: 2 }),
      entry("A", "端", "はた", { position: 1 }),
    ];
    expect(
      pronunciationCategories({ term: "端", reading: "はし" }, entries),
    ).toBe("heiban odaka");
    expect(
      pronunciationCategories({ term: "端", reading: "はた" }, entries, true),
    ).toBe("kifuku");
    expect(
      pronunciationCategories({ term: "橋", reading: "はし" }, entries),
    ).toBe("");
  });
});
