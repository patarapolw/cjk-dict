import { toKana, toKatakana } from "wanakana";

const LETTERS = [..."abcdefghijklmnopqrstuvwxyz"];
const VOWELS = [..."aiueo"];
/** the kana a conversion starts with */
const LEADING_KANA = /^[\p{scx=Hiragana}\p{scx=Katakana}ー]+/u;
/** letters wanakana may still convert; it leaves the rest of the input alone */
const ROMAJI = /^[a-z]*$/;
/** extra letters to look ahead: `l` + `tsu` -> っ */
const MAX_LOOKAHEAD = 4;

const rows = new Map<string, string[]>();

/**
 * The kana an incomplete romaji syllable can become, by wanakana's IME-mode rules, extended
 * ones included: `k` -> か, き, きゃ…, く, くぁ…, っか (`kka`)…; `q` -> くぁ, くゃ…; `x` -> ぁ…,
 * っ, ゎ. Obsolete kana as well as modern ones: `w` -> ゐ and うぃ. Typing with auto-convert
 * to kana leaves the syllable as typed until it is complete (`にほんg`), so it would match
 * no term. Upper case, as wanakana's IME mode does, is katakana. Sorted; empty if `romaji`
 * starts no syllable, e.g. `st`.
 */
export function kanaRow(romaji: string): string[] {
  const lower = romaji.toLowerCase();
  let row = rows.get(lower);
  if (!row) {
    const found = new Set<string>();
    for (const useObsoleteKana of [true, false]) {
      const conv = (s: string) => toKana(s, { IMEMode: true, useObsoleteKana });
      /** some vowel completes `s` into kana: `s` is still a syllable being typed */
      const live = (s: string) =>
        VOWELS.some((v) => LEADING_KANA.test(conv(s + v)));
      const walk = (s: string, depth: number) => {
        for (const c of LETTERS) {
          const kana = conv(s + c);
          // the syllable is complete: what it became, up to any letters after it
          const m = LEADING_KANA.exec(kana);
          if (m) found.add(m[0]);
          else if (depth < MAX_LOOKAHEAD && ROMAJI.test(kana) && live(s + c)) {
            walk(s + c, depth + 1);
          }
        }
      };
      // `romaji` must convert as a whole: `kq` + `a` is k + くぁ
      if (ROMAJI.test(conv(lower)) && live(lower)) walk(lower, 1);
    }
    row = [...found].sort();
    rows.set(lower, row);
  }
  return romaji === romaji.toUpperCase() ? row.map((k) => toKatakana(k)) : row;
}

const escapeGlob = (c: string) => (/[*?[\]^]/.test(c) ? `[${c}]` : c);

/** a GLOB class of `chars`, or the character itself if there is only one */
function globClass(chars: string[]): string {
  const cs = [...new Set(chars)].sort();
  return cs.length === 1 ? escapeGlob(cs[0]) : `[${cs.join("")}]`;
}

/**
 * The trailing incomplete romaji of `term` as a GLOB pattern of its kana row, for a prefix
 * search: `にほんg` -> `にほん[がぎぐげごっ]`. A class per character, as far as the row's
 * shortest kana (`ky` -> `き[ぃぇゃゅょ]`, `k` -> `[かきくけこっ]`), which the prefix search
 * extends. Null if `term` doesn't end in romaji that starts a syllable.
 */
export function romajiTailGlob(term: string): string | null {
  // consonants only: wanakana converts a syllable as soon as its vowel comes, so letters
  // with a vowel (pinyin's `hao`, `zhong`) aren't one being typed
  const m = /(?<![a-z])[bcdfghjklmnpqrstvwxyz]+$/i.exec(term);
  if (!m) return null;
  const row = kanaRow(m[0]).map((k) => [...k]);
  if (!row.length) return null;

  const len = Math.min(...row.map((k) => k.length));
  let glob = "";
  for (let i = 0; i < len; i++) glob += globClass(row.map((k) => k[i]));
  return term.slice(0, m.index) + glob;
}
