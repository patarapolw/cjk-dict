/**
 * Han characters and their IDS (Ideographic Description Sequences), as the `han_chars`
 * command returns them from the CJKVI IDS database. Mirrors yomitan-parser's `ids` module.
 */

/** CJK ideographs, radicals and strokes, including extensions and compatibility ideographs */
const HAN = /[⺀-⿟々〇㇀-㇯㐀-䶿一-鿿豈-﫿\u{20000}-\u{323AF}]/u;

/** The distinct Han characters of `text`, in order. */
export function hanChars(text: string): string[] {
  const out: string[] = [];
  for (const m of text.matchAll(/(\P{M})([︀-️\u{E0100}-\u{E01EF}]*)/gu)) {
    if (HAN.test(m[1]) && !out.includes(m[0])) out.push(m[0]);
  }
  return out;
}

/** Ideographic Description Characters: how the components are laid out */
export const IDC_NAMES: Record<string, string> = {
  "⿰": "left to right",
  "⿱": "above to below",
  "⿲": "left to middle and right",
  "⿳": "above to middle and below",
  "⿴": "full surround",
  "⿵": "surround from above",
  "⿶": "surround from below",
  "⿷": "surround from left",
  "⿸": "surround from upper left",
  "⿹": "surround from upper right",
  "⿺": "surround from lower left",
  "⿻": "overlaid",
  "⿼": "surround from right",
  "⿽": "surround from lower right",
  "⿾": "horizontal reflection",
  "⿿": "rotation",
  "㇯": "subtraction",
};

export type IdsToken =
  | { kind: "idc"; text: string; title: string }
  | { kind: "component"; text: string }
  /** an unencoded component: ① to ⑳ (that many strokes), &CDP-xxxx;, ？ */
  | { kind: "unknown"; text: string; title: string };

/** Split an IDS into layout characters and components, e.g. `⿰木木`. */
export function idsTokens(ids: string): IdsToken[] {
  const out: IdsToken[] = [];
  for (const m of ids.matchAll(/&[^;]*;|(\P{M})[︀-️\u{E0100}-\u{E01EF}]*/gu)) {
    const text = m[0];
    if (text.startsWith("&")) {
      out.push({ kind: "unknown", text: "？", title: text });
    } else if (IDC_NAMES[text]) {
      out.push({ kind: "idc", text, title: IDC_NAMES[text] });
    } else if (/^[①-⑳]$/u.test(text)) {
      const strokes = text.codePointAt(0)! - 0x2460 + 1;
      out.push({
        kind: "unknown",
        text,
        title: `a component with no code point, ${strokes} stroke${strokes > 1 ? "s" : ""}`,
      });
    } else if (text === "？" || text === "?") {
      out.push({ kind: "unknown", text: "？", title: "unknown component" });
    } else {
      out.push({ kind: "component", text });
    }
  }
  return out;
}

/** IDS region letters (G: China, T: Taiwan, J: Japan, K: Korea, ...) */
export const REGION_NAMES: Record<string, string> = {
  G: "China",
  H: "Hong Kong",
  M: "Macau",
  T: "Taiwan",
  J: "Japan",
  K: "Korea",
  P: "North Korea",
  V: "Vietnam",
  U: "Unicode",
  S: "SAT",
  X: "alternative",
};

const LANG_REGION: Record<string, string> = {
  "ja-JP": "J",
  "zh-CN": "G",
  "ko-KR": "K",
};

/**
 * The IDS for `lang` first: a character may be written differently in China, Japan and
 * Korea, e.g. 与 is `⿹②一` in China and `⿻②一` in Japan.
 */
export function sortIds<T extends { regions: string }>(
  ids: T[],
  lang: string,
): T[] {
  const region = LANG_REGION[lang];
  if (!region) return ids;
  const rank = (i: T) => (!i.regions || i.regions.includes(region) ? 0 : 1);
  return [...ids].sort((a, b) => rank(a) - rank(b));
}

/** Entries of dictionaries in `lang` first, else in their order */
export function sortByLang<T extends { lang: string | null }>(
  entries: T[],
  lang: string,
): T[] {
  if (!lang) return entries;
  const short = lang.split("-")[0];
  const rank = (e: T) => (e.lang?.split("-")[0] === short ? 0 : 1);
  return [...entries].sort((a, b) => rank(a) - rank(b));
}
