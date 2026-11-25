/**
 * Han characters and their IDS (Ideographic Description Sequences), as the `han_chars`
 * command returns them from the CJKVI IDS database. Mirrors yomitan-parser's `ids` module.
 */

import type { IHanVariant, VariantKind } from "../db/search";

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

/** a search language's IDS regions: Chinese is China's and Taiwan's, Hong Kong's, ... */
const LANG_REGIONS: Record<string, string> = {
  ja: "J",
  zh: "GTHM",
  ko: "K",
};

/**
 * The IDS for `lang` first: a character may be written differently in China, Japan and
 * Korea, e.g. 与 is `⿹②一` in China and `⿻②一` in Japan.
 */
export function sortIds<T extends { regions: string }>(
  ids: T[],
  lang: string,
): T[] {
  const regions = LANG_REGIONS[lang.split("-")[0]];
  if (!regions) return ids;
  const rank = (i: T) =>
    !i.regions || [...regions].some((r) => i.regions.includes(r)) ? 0 : 1;
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

/** How a variant relates to its character: a short label, and what it means */
export const VARIANT_KINDS: Record<
  VariantKind,
  { label: string; title: string }
> = {
  traditional: { label: "traditional", title: "traditional Chinese form" },
  simplified: { label: "simplified", title: "simplified Chinese form" },
  kyujitai: { label: "kyūjitai", title: "old Japanese form (kyūjitai)" },
  shinjitai: { label: "shinjitai", title: "new Japanese form (shinjitai)" },
  semantic: {
    label: "semantic",
    title: "same meaning, though not always interchangeable",
  },
  other: {
    label: "other",
    title: "same character drawn differently, or same meaning in some senses",
  },
};

/** loose kinds, named only when a variant is nothing more definite */
const LOOSE_KINDS: VariantKind[] = ["semantic", "other"];

/** The kinds to name of a variant: 萬 is 万's traditional form and kyūjitai, not "semantic" */
export function variantKinds(kinds: VariantKind[]): VariantKind[] {
  const definite = kinds.filter((k) => !LOOSE_KINDS.includes(k));
  return definite.length ? definite : kinds;
}

/** e.g. `traditional, kyūjitai` */
export const variantLabel = (kinds: VariantKind[]) =>
  variantKinds(kinds)
    .map((k) => VARIANT_KINDS[k].label)
    .join(", ");

/** e.g. `學: traditional Chinese form; old Japanese form (kyūjitai)` */
export const variantTitle = (v: IHanVariant) =>
  `${v.ch}: ${variantKinds(v.kinds)
    .map((k) => VARIANT_KINDS[k].title)
    .join("; ")}`;

/** KANJIDIC stats that are readings to search by, and their language */
export const READING_STAT_LANGS: Record<string, string> = {
  pinyin: "zh",
  korean_h: "ko-KR",
  nanori: "ja-JP",
};

/** the search language a reading is written for, from its script */
export function readingLang(reading: string): string {
  if (/\p{sc=Hangul}/u.test(reading)) return "ko-KR";
  if (/[\p{scx=Hiragana}\p{scx=Katakana}]/u.test(reading)) return "ja-JP";
  return "zh";
}

/**
 * A character's reading as a search for the words with it read so, anywhere in the word:
 * `*<興>{キョウ}` (興味, 余興, ...), `*<興>{xīng}`, `*<興>{흥}`. A kun reading counts up to
 * its okurigana, which its dot marks (`おこ.る`: `*<興>{おこ}`, finding 興る and 興す), and
 * its dash marks where other kana go (`-び`).
 */
export function readingQuery(ch: string, reading: string): string {
  const r = reading.replace(/^[-‐]+|[-‐]+$/gu, "").trim();
  return `*<${ch}>{${r.split(".")[0]}}`;
}

const TONES: Record<string, string[]> = {
  a: ["ā", "á", "ǎ", "à"],
  e: ["ē", "é", "ě", "è"],
  i: ["ī", "í", "ǐ", "ì"],
  o: ["ō", "ó", "ǒ", "ò"],
  u: ["ū", "ú", "ǔ", "ù"],
  ü: ["ǖ", "ǘ", "ǚ", "ǜ"],
};

/**
 * Numbered pinyin with tone marks, as KANJIDIC writes it: `xing1` → `xīng`, `lu:e4` or
 * `lve4` → `lüè`, `ma5` → `ma`. The mark goes on a or e, else the o of ou, else the last
 * vowel. Pinyin with marks already, or without a number, is kept as it is.
 */
export function toneMarks(pinyin: string): string {
  return pinyin.replace(/([a-zü:]+)([1-5])/giu, (_, syl: string, n: string) => {
    const s = syl.replace(/u:|v/giu, (m) =>
      m[0] === m[0].toUpperCase() ? "Ü" : "ü",
    );
    const tone = Number(n);
    if (tone === 5) return s;
    const lower = s.toLowerCase();
    let at = lower.search(/[ae]/u);
    if (at < 0) at = lower.indexOf("ou");
    if (at < 0) {
      const vowels = [...lower.matchAll(/[iouü]/gu)];
      at = vowels.length ? vowels[vowels.length - 1].index! : -1;
    }
    if (at < 0) return s;
    const v = lower[at];
    const marked = TONES[v][tone - 1];
    const upper = s[at] !== v;
    return (
      s.slice(0, at) + (upper ? marked.toUpperCase() : marked) + s.slice(at + 1)
    );
  });
}
