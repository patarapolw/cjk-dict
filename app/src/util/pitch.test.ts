import { describe, expect, it } from "vitest";

import { downstep, morae, pitchKind, pitchMorae } from "./pitch";

/** `ha↑shi↓` as text: high morae in capitals-like brackets, a drop as ↓ */
function draw(reading: string, position: number | string): string {
  return pitchMorae(reading, position)
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

  it("takes a pattern of H and L", () => {
    expect(draw("はし", "LHL")).toBe("は[し]↓");
    expect(draw("はし", "LH")).toBe("は[し]");
    expect(draw("はし", "HL")).toBe("[は]↓し");
    expect(draw("はし", "2")).toBe("は[し]↓");
  });
});

describe("pitchKind", () => {
  it("names the pattern", () => {
    expect(pitchKind("はし", 0)).toBe("heiban");
    expect(pitchKind("はし", 1)).toBe("atamadaka");
    expect(pitchKind("はし", 2)).toBe("odaka");
    expect(pitchKind("みずうみ", 3)).toBe("nakadaka");
    expect(pitchKind("はし", "LHL")).toBeNull();
    expect(downstep("LHL")).toBeNull();
  });
});
