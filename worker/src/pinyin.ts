/**
 * Port of yomitan-parser/src/pinyin.rs (`normalize_pinyin` / `expand_pinyin`).
 * Keep the two in sync: dict-admin normalizes stored readings with the Rust version,
 * the worker expands query readings with this one.
 */

type Letter = [string, number]; // [plain letter, tone 0..4 (0 = unmarked)]

const MARKS: Record<string, Letter> = {
  ā: ["a", 1], á: ["a", 2], ǎ: ["a", 3], à: ["a", 4],
  ē: ["e", 1], é: ["e", 2], ě: ["e", 3], è: ["e", 4],
  ī: ["i", 1], í: ["i", 2], ǐ: ["i", 3], ì: ["i", 4],
  ō: ["o", 1], ó: ["o", 2], ǒ: ["o", 3], ò: ["o", 4],
  ū: ["u", 1], ú: ["u", 2], ǔ: ["u", 3], ù: ["u", 4],
  ǖ: ["ü", 1], ǘ: ["ü", 2], ǚ: ["ü", 3], ǜ: ["ü", 4],
  ɡ: ["g", 0], // single-story g, used in some pinyin text
};

const INITIALS = [
  "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "r",
  "z", "c", "s", "y", "w", "",
];
const FINALS = new Set([
  "a", "o", "e", "i", "u", "ü", "ai", "ei", "ui", "ao", "ou", "iu", "ie", "üe", "ue", "er", "an",
  "en", "in", "un", "ün", "ang", "eng", "ing", "ong", "ia", "iao", "ian", "iang", "iong", "ua",
  "uo", "uai", "uan", "uang", "ueng", "üan", "ê",
]);

function isSyllable(s: string) {
  return INITIALS.some((i) => s.startsWith(i) && FINALS.has(s.slice(i.length)));
}

function segment(run: Letter[]): Letter[][] | null {
  if (!run.length) return [];
  for (let len = Math.min(run.length, 6); len >= 1; len--) {
    const head = run.slice(0, len);
    const s = head.map(([c]) => c).join("");
    const marks = head.filter(([, t]) => t > 0).length;
    if (marks <= 1 && isSyllable(s)) {
      const rest = segment(run.slice(len));
      if (rest) return [head, ...rest];
    }
  }
  return null;
}

function markedTone(syl: Letter[]): number | undefined {
  return syl.find(([, t]) => t > 0)?.[1];
}

function render(syl: Letter[], explicit?: number) {
  const letters = syl.map(([c]) => c).join("").replace(/ü/g, "v");
  return `${letters}${explicit ?? markedTone(syl) ?? 5}`;
}

function parsePinyin(input: string): [Letter[], number | undefined][] | null {
  const lower = input.normalize("NFC").toLowerCase().replace(/u:/g, "ü").replace(/v/g, "ü");
  // a digit always ends a syllable: "pin1yin1" -> "pin1 yin1 "
  const spaced = lower.replace(/[0-9]/g, "$& ");

  const out: [Letter[], number | undefined][] = [];
  for (const word of spaced.split(/[\s'’\-·]/u).filter(Boolean)) {
    const chars = [...word];
    const last = chars[chars.length - 1];
    const digit = /[0-9]/.test(last) ? last : undefined;
    const letters = digit ? chars.slice(0, -1) : chars;
    const run: Letter[] = letters.map((c) => MARKS[c] ?? [c, 0]);
    if (!run.length) return null;

    if (digit !== undefined) {
      const d = Number(digit);
      if (d > 5) return null;
      if (!run.every(([l]) => /\p{Alphabetic}/u.test(l))) return null;
      out.push([run, d === 0 ? 5 : d]);
    } else {
      const syls = segment(run);
      if (!syls) return null;
      for (const syl of syls) out.push([syl, undefined]);
    }
  }
  return out.length ? out : null;
}

/** Lowercase, tone digit after each syllable, `0` -> 5, `ü` -> `v`. null if not pinyin. */
export function normalizePinyin(input: string): string | null {
  const syllables = parsePinyin(input);
  if (!syllables) return null;
  return syllables.map(([syl, tone]) => render(syl, tone)).join(" ");
}

/**
 * One result per tone combination of toneless syllables (5^n).
 * Empty when the input cannot be read as pinyin.
 */
export function expandPinyin(input: string): string[] {
  const syllables = parsePinyin(input);
  if (!syllables) return [];

  let expanded = [""];
  for (const [syl, explicit] of syllables) {
    const tone = explicit ?? markedTone(syl);
    const alternatives = (tone !== undefined ? [tone] : [1, 2, 3, 4, 5]).map((t) =>
      render(syl, t),
    );
    expanded = expanded.flatMap((prefix) =>
      alternatives.map((alt) => (prefix ? `${prefix} ${alt}` : alt)),
    );
  }
  return expanded;
}
