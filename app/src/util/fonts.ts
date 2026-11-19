import { convertFileSrc } from "@tauri-apps/api/core";
import { BaseDirectory, exists } from "@tauri-apps/plugin-fs";

import { DownloadingProps } from "./loading";

/**
 * Fonts for the Han characters the system's fonts lack, such as rare components in IDS
 * (Extensions B to F). CJKVI IDS recommends fonts that cover every encoded ideograph:
 * Hanazono Mincho, here as HanaMinAFDKO (GlyphWiki license), downloaded on first launch
 * to `<app data>/fonts/` and served by the `font://` protocol (src-tauri/src/fonts.rs).
 *
 * They come after the system's fonts (`font-family` in index.css), so they only draw
 * what those don't have, and each file is only read for the ranges it covers.
 *
 * @see https://github.com/cjkvi/cjkvi-ids#description
 * @see https://github.com/cjkvi/HanaMinAFDKO
 */

export const FONT_FAMILY = "HanaMin";
const VERSION = "8.030";
const DIR = "fonts";

export interface HanFont {
  file: string;
  /** what the file covers (its cmap), as `@font-face` `unicode-range` */
  unicodeRange: string;
}

export const HAN_FONTS: HanFont[] = [
  {
    // 31.6 MB
    file: "HanaMinA.otf",
    unicodeRange: [
      "U+2E80-2FDF", // radicals
      "U+3005-3007, U+3021-3029, U+3038-303B", // 々〆〇, Hangzhou numerals (〢 in IDS)
      "U+31C0-31EF", // strokes
      "U+3400-4DBF", // Extension A
      "U+4E00-9FFF",
      "U+F900-FAFF", // compatibility ideographs
    ].join(", "),
  },
  {
    // 32.2 MB
    file: "HanaMinB.otf",
    unicodeRange: "U+20000-2A6DF", // Extension B
  },
  {
    // 13.0 MB
    file: "HanaMinC.otf",
    unicodeRange: [
      "U+2A700-2EBEF", // Extensions C to F
      "U+2F800-2FA1F", // compatibility ideographs supplement
    ].join(", "),
  },
];

const downloadUrl = (file: string) =>
  `https://github.com/cjkvi/HanaMinAFDKO/releases/download/${VERSION}/${file}`;

const downloaded = (file: string) =>
  exists(`${DIR}/${file}`, { baseDir: BaseDirectory.AppData });

/** The fonts not downloaded yet. */
export async function makeFontDownloadList(): Promise<DownloadingProps[]> {
  const out: DownloadingProps[] = [];
  for (const { file } of HAN_FONTS) {
    if (await downloaded(file)) continue;
    out.push({
      filename: `${DIR}/${file}`,
      url: downloadUrl(file),
      font: true,
    });
  }
  return out;
}

/** `@font-face` rules for `fonts`, served from `src(file)`. */
export function fontFaces(
  fonts: HanFont[],
  src: (file: string) => string,
): string {
  return fonts
    .map(
      (f) => `@font-face {
  font-family: "${FONT_FAMILY}";
  src: url("${src(f.file)}") format("opentype");
  unicode-range: ${f.unicodeRange};
  font-display: swap;
}`,
    )
    .join("\n");
}

const STYLE_ID = "han-fonts";

/** Use the fonts downloaded so far: on start, and after each one arrives. */
export async function installFonts() {
  const have: HanFont[] = [];
  for (const f of HAN_FONTS) {
    try {
      if (await downloaded(f.file)) have.push(f);
    } catch (e) {
      console.warn(`font ${f.file}`, e);
    }
  }
  let style = document.getElementById(STYLE_ID);
  if (!have.length) {
    style?.remove();
    return;
  }
  if (!style) {
    style = document.createElement("style");
    style.id = STYLE_ID;
    document.head.append(style);
  }
  style.textContent = fontFaces(have, (file) => convertFileSrc(file, "font"));
}
