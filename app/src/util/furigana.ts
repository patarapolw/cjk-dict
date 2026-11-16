import { toHiragana } from "wanakana";

/**
 * Furigana syntax in the search box: `<日本>{にほん}語` searches the term 日本語 whose reading
 * starts with にほん. `{reading}` alone searches by reading only, e.g. `{ni3 hao3}`.
 */

const KANA = /^[\p{scx=Hiragana}\p{scx=Katakana}ー]+$/u;
/** spells its own reading outside `<>{}`: kana, and Hangul (`<光>{광}나다`) */
const SELF_READING = /^[\p{scx=Hiragana}\p{scx=Katakana}ー\p{sc=Hangul}]+$/u;
const KANA_OR_WILDCARD = /^[\p{scx=Hiragana}\p{scx=Katakana}ー*?]+$/u;
/** a run of characters whose reading the IME (or the dictionary) supplies */
const KANJI_RUN = /([\p{sc=Han}\p{N}々〆ヵヶ]+)/u;

const SEGMENT = /<([^<>{}]*)>\{([^{}]*)\}?|\{([^{}]*)\}?|([^{}]+)/gu;

export interface ParsedQuery {
  /** what to match against the term */
  term: string;
  /** reading-only (or pinyin) search: matched against reading tokens, pinyin tones expanded */
  reading: string;
  /** kana reading pattern for `term`, `*` where a reading is unknown, e.g. `にほん*` */
  readingGlob: string;
  /** text after the first space: the query was split at a word boundary */
  rest: string;
}

/** full-width brackets, as typed with a Japanese IME */
export function normalizeQuery(q: string): string {
  return q.replace(/[＜＞｛｝]/g, (c) =>
    String.fromCharCode(c.charCodeAt(0) - 0xfee0),
  );
}

/** Split at the first space outside `{}` (pinyin readings contain spaces). */
function splitFirstWord(q: string): [string, string] {
  let depth = 0;
  for (let i = 0; i < q.length; i++) {
    const c = q[i];
    if (c === "{") depth++;
    else if (c === "}") depth = Math.max(0, depth - 1);
    else if (depth === 0 && /\s/u.test(c)) {
      return [q.slice(0, i), q.slice(i + 1).trim()];
    }
  }
  return [q, ""];
}

export function parseQuery(raw: string): ParsedQuery {
  const [first, rest] = splitFirstWord(normalizeQuery(raw).trim());

  let term = "";
  let reading = "";
  let hasReading = false;

  for (const m of first.matchAll(SEGMENT)) {
    const [, base, rt, rtOnly, plain] = m;
    if (base !== undefined) {
      term += base;
      reading += rt;
      hasReading = true;
    } else if (rtOnly !== undefined) {
      reading += rtOnly;
      hasReading = true;
    } else if (plain !== undefined) {
      const text = plain.replace(/[<>]/g, "");
      term += text;
      // kana and Hangul spell themselves; anything else is a reading to fill in
      for (const c of text) reading += SELF_READING.test(c) ? c : "*";
    }
  }

  reading = reading.replace(/\*+/g, "*").trim();
  if (!hasReading || !reading.replace(/[*?\s]/g, "")) {
    return { term, reading: "", readingGlob: "", rest };
  }
  if (term && KANA_OR_WILDCARD.test(reading)) {
    return { term, reading: "", readingGlob: reading, rest };
  }
  return {
    term,
    reading: reading.replace(/[*?]/g, " ").replace(/\s+/g, " ").trim(),
    readingGlob: "",
    rest,
  };
}

const escapeRegExp = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const hira = (s: string) => toHiragana(s, { passRomaji: true });

/**
 * `食べる` + `たべる` -> `<食>{た}べる`: furigana only over the kanji, so that kana in the term
 * (okurigana) still match literally. Text without kanji, or without a reading, is unchanged.
 */
export function toFurigana(text: string, reading: string): string {
  if (!reading || reading === text) return text;
  const parts = text.split(KANJI_RUN);
  if (parts.length === 1) return text;

  const re = new RegExp(
    "^" +
      parts
        .map((p, i) => (i % 2 ? "(.+?)" : `(${escapeRegExp(hira(p))})`))
        .join("") +
      "$",
    "u",
  );
  const m = re.exec(hira(reading));
  if (!m) return `<${text}>{${reading}}`;

  // map back to the reading as given (it may be katakana)
  const chars = [...reading];
  let pos = 0;
  return parts
    .map((p, i) => {
      const len = [...m[i + 1]].length;
      const rt = chars.slice(pos, pos + len).join("");
      pos += len;
      return i % 2 ? `<${p}>{${rt}}` : p;
    })
    .join("");
}

/** the kana an IME shows while the user types, before conversion */
export function isImeReading(s: string): boolean {
  return KANA.test(s.replace(/ｎ/g, "ん"));
}

/** Latin letters, as typed for Korean romanization: `hangugeo`, `hangug-eo` */
export function isRomanization(s: string): boolean {
  return /[a-z]/i.test(s) && /^[a-z\-'’. ]+$/i.test(s);
}
