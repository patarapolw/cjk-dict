import { clearMocks, mockConvertFileSrc, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  FONT_FAMILY,
  HAN_FONTS,
  fontFaces,
  installFonts,
  makeFontDownloadList,
} from "./fonts";

let present: string[] = [];

beforeEach(() => {
  present = [];
  mockConvertFileSrc("windows");
  mockIPC((cmd, args) => {
    if (cmd === "plugin:fs|exists") {
      return present.includes(String((args as { path: string }).path));
    }
    throw new Error(`unexpected command: ${cmd}`);
  });
});

afterEach(() => {
  document.getElementById("han-fonts")?.remove();
  clearMocks();
});

const styleText = () => document.getElementById("han-fonts")?.textContent;

describe("fonts", () => {
  it("downloads only the fonts not downloaded yet", async () => {
    present = ["fonts/HanaMinA.otf"];
    expect(await makeFontDownloadList()).toEqual([
      {
        filename: "fonts/HanaMinB.otf",
        url: "https://github.com/cjkvi/HanaMinAFDKO/releases/download/8.030/HanaMinB.otf",
        font: true,
      },
      {
        filename: "fonts/HanaMinC.otf",
        url: "https://github.com/cjkvi/HanaMinAFDKO/releases/download/8.030/HanaMinC.otf",
        font: true,
      },
    ]);
  });

  it("covers each range with one font, each only for its range", () => {
    const css = fontFaces(HAN_FONTS, (f) => `font://localhost/${f}`);
    expect(css.match(/@font-face/g)).toHaveLength(3);
    expect(css).toContain(`font-family: "${FONT_FAMILY}"`);
    expect(css).toContain('src: url("font://localhost/HanaMinC.otf")');
    // 〢 U+3022, a component in IDS, is in HanaMinA
    expect(HAN_FONTS[0].unicodeRange).toContain("U+3021-3029");
    // common characters are left to the system's CJK fonts, in a sans-serif style
    const covered = (cp: number) =>
      HAN_FONTS.some((f) =>
        f.unicodeRange.split(",").some((r) => {
          const [a, b = a] = r.trim().slice(2).split("-");
          return parseInt(a, 16) <= cp && cp <= parseInt(b, 16);
        }),
      );
    for (const ch of "学學々〇")
      expect(covered(ch.codePointAt(0)!)).toBe(false);
    // 鿐 U+9FD0 (Unicode 9) and 㐀 U+3400 (Extension A) are
    for (const ch of "鿐㐀") expect(covered(ch.codePointAt(0)!)).toBe(true);
    // 𬄋 U+2C10B (Extension E) is in HanaMinC
    expect(
      HAN_FONTS.find((f) => f.file === "HanaMinC.otf")?.unicodeRange,
    ).toMatch(/U\+2A700-2EBEF/);
  });

  it("uses only the fonts downloaded, and updates as they arrive", async () => {
    await installFonts();
    expect(styleText()).toBeUndefined();

    present = ["fonts/HanaMinC.otf"];
    await installFonts();
    expect(styleText()).toContain("HanaMinC.otf");
    expect(styleText()).not.toContain("HanaMinA.otf");

    present = HAN_FONTS.map((f) => `fonts/${f.file}`);
    await installFonts();
    expect(styleText()?.match(/@font-face/g)).toHaveLength(3);
    // one style element, replaced
    expect(document.querySelectorAll("#han-fonts")).toHaveLength(1);
  });
});
