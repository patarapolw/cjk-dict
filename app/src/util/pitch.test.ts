import { describe, expect, it } from "vitest";

import {
  downsteps,
  groupByDictionary,
  isVerbOrAdjective,
  morae,
  pitchKind,
  pitchMorae,
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

describe("groupByDictionary", () => {
  it("lists each dictionary's pitches together", () => {
    const e = (dict_path: string, reading: string, ...ps: number[]) => ({
      dict_id: 1,
      dict_path,
      reading,
      pitches: ps.map((position) => ({ position })),
    });
    const groups = groupByDictionary([
      e("A", "はし", 0),
      e("A", "はた", 0, 1),
      e("B", "はし", 0),
    ]);
    expect(
      groups.map((g) => [
        g.dict_path,
        g.pitches.map((p) => `${p.reading}${p.pitch.position}`),
      ]),
    ).toEqual([
      ["A", ["はし0", "はた0", "はた1"]],
      ["B", ["はし0"]],
    ]);
  });
});
