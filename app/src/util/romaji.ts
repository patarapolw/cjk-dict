import { toKana, toKatakana } from "wanakana";

const VOWELS = ["a", "i", "u", "e", "o"];
const KANA = /^[\p{scx=Hiragana}\p{scx=Katakana}ー]+$/u;

/**
 * The kana an incomplete romaji syllable can become: `k` -> か, き, く, け, こ. Typing with
 * auto-convert to kana leaves it as typed until its vowel comes (`にほんg`), so it would
 * match no term. Upper case, as wanakana's IME mode does, is katakana. Empty if `romaji`
 * starts no syllable, e.g. `st`.
 */
export function kanaRow(romaji: string): string[] {
  const upper = romaji === romaji.toUpperCase();
  const lower = romaji.toLowerCase();
  const row = VOWELS.map((v) =>
    toKana(lower + v, { useObsoleteKana: true, IMEMode: true }),
  );
  // a lone `n` can also be ん, before a consonant
  if (lower === "n") row.push("ん");
  if (!row.every((k) => KANA.test(k))) return [];
  return upper ? row.map((k) => toKatakana(k)) : row;
}

const escapeGlob = (c: string) => (/[*?[\]^]/.test(c) ? `[${c}]` : c);

/** a GLOB class of `chars`, or the character itself if there is only one */
function globClass(chars: Set<string>): string {
  const cs = [...chars];
  return cs.length === 1 ? escapeGlob(cs[0]) : `[${cs.join("")}]`;
}

/**
 * The trailing incomplete romaji of `term` as a GLOB pattern of its kana row, for a prefix
 * search: `にほんg` -> `にほん[がぎぐげご]`. A class per character when the row's kana are
 * as long (`ky` -> `き[ゃぃゅぇょ]`), else of their first character only (`sh` -> `し`,
 * which the prefix search extends). Null if `term` doesn't end in romaji that
 * starts a syllable.
 */
export function romajiTailGlob(term: string): string | null {
  const m = /[a-z]+$/i.exec(term);
  if (!m) return null;
  const row = kanaRow(m[0]).map((k) => [...k]);
  if (!row.length) return null;

  const len = row[0].length;
  const positions = row.every((k) => k.length === len) ? len : 1;
  let glob = "";
  for (let i = 0; i < positions; i++) {
    glob += globClass(new Set(row.map((k) => k[i])));
  }
  return term.slice(0, m.index) + glob;
}
