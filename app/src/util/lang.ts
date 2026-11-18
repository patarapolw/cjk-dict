/**
 * `lang` attributes for dictionary entries. Han characters are unified in Unicode, so the
 * same code point is drawn differently for Japanese, Chinese (simplified and traditional)
 * and Korean: 直, 骨, 写, ... Only the `lang` of the text picks the right form.
 *
 * Each dictionary has a source language (L1, its terms') and a target language (L2, its
 * definitions'), e.g. wty-zh-en `zh` → `en`, Pixiv (none) → `ja`.
 */

import type { ITerm } from "../db/search";

/** the primary language subtag: `zh` of `zh-Hant-TW` */
export const primaryLang = (tag: string) => tag.split("-")[0].toLowerCase();

const CJK = ["ja", "zh", "ko"];

/** the text of a structured-content node, or null if it has other elements than text */
function textOf(node: unknown): string | null {
  if (typeof node === "string") return node;
  if (Array.isArray(node)) {
    let s = "";
    for (const n of node) {
      const t = textOf(n);
      if (t === null) return null;
      s += t;
    }
    return s;
  }
  if (node && typeof node === "object") {
    const el = node as { tag?: string; content?: unknown };
    if (el.tag === "img" || el.tag === "br") return null;
    return el.content === undefined ? "" : textOf(el.content);
  }
  return null;
}

/** the `lang` of the element in `node` that holds just `term` */
function findLang(node: unknown, term: string, depth = 0): string | null {
  if (depth > 32 || !node || typeof node !== "object") return null;
  if (Array.isArray(node)) {
    for (const n of node) {
      const l = findLang(n, term, depth + 1);
      if (l) return l;
    }
    return null;
  }
  const el = node as { lang?: unknown; content?: unknown };
  if (typeof el.lang === "string" && el.lang && textOf(el.content) === term) {
    return el.lang;
  }
  return findLang(el.content, term, depth + 1);
}

/** every `lang` in `node` */
function allLangs(node: unknown, out = new Set<string>(), depth = 0) {
  if (depth > 32 || !node || typeof node !== "object") return out;
  if (Array.isArray(node)) {
    for (const n of node) allLangs(n, out, depth + 1);
    return out;
  }
  const el = node as { lang?: unknown; content?: unknown };
  if (typeof el.lang === "string" && el.lang) out.add(el.lang);
  return allLangs(el.content, out, depth + 1);
}

/**
 * The language a term is written in, from its glossary: a dictionary may say which script
 * a headword is in, e.g. CC-CEDICT's `<span lang="zh-Hant">學習</span>` and
 * `<span lang="zh-Hans">学习</span>`, as Mandarin isn't always written as in China. Only a
 * language matching `l1` (or any of Japanese, Chinese and Korean without one) counts, and
 * only next to another form of it: CC-CEDICT writes a term the same in both scripts (直)
 * once, as `zh-Hans`, which then says nothing of its script.
 */
export function glossaryTermLang(
  term: string,
  glossary: unknown,
  l1?: string | null,
): string | null {
  if (!term) return null;
  const found = findLang(glossary, term);
  if (!found) return null;
  const p = primaryLang(found);
  if (l1 ? p !== primaryLang(l1) : !CJK.includes(p)) return null;
  const other = [...allLangs(glossary)].some(
    (l) => l !== found && primaryLang(l) === p,
  );
  return other ? found : null;
}

const cache = new WeakMap<ITerm, string | null>();

/** the script the entry's glossary says its term is in (cached), e.g. `zh-Hant` */
function declaredLang(row: ITerm): string | null {
  let found = cache.get(row);
  if (found === undefined) {
    let glossary: unknown;
    try {
      glossary = JSON.parse(row.glossary);
    } catch {
      glossary = null;
    }
    found = glossaryTermLang(row.term, glossary, row.lang);
    cache.set(row, found);
  }
  return found;
}

const cjkOrNull = (tag?: string | null) =>
  tag && CJK.includes(primaryLang(tag)) ? tag : null;

/**
 * The `lang` of an entry's term: as its glossary says, else the dictionary's source
 * language (L1), else its target language if Japanese, Chinese or Korean (Pixiv: `ja`,
 * no L1), else `fallback` (e.g. the search language), else none (inherited).
 */
export function termLang(row: ITerm, fallback = ""): string | undefined {
  return (
    declaredLang(row) ||
    row.lang ||
    cjkOrNull(row.target_lang) ||
    fallback ||
    undefined
  );
}

/** the script an entry of `entries` names the term to be in, e.g. CC-CEDICT's `zh-Hans` */
export function declaredHeadLang(entries: ITerm[]): string | null {
  for (const e of entries) {
    const l = declaredLang(e);
    if (l) return l;
  }
  return null;
}

/**
 * Results' heading languages, a bare language (`zh`) made the script another result with
 * the same term names (`zh-Hans`): wty-zh-en's 学习 is a result of its own, without the
 * script CC-CEDICT's 学习 names. `lang(i)` and `term(i)` are a result's.
 */
export function refineHeadLangs<T>(
  items: T[],
  lang: (item: T) => string | undefined,
  term: (item: T) => string,
  declared: (item: T) => string | null,
): (string | undefined)[] {
  const known = new Map<string, string>();
  for (const it of items) {
    const d = declared(it);
    const key = d && `${term(it)}\t${primaryLang(d)}`;
    if (key && !known.has(key)) known.set(key, d);
  }
  return items.map((it) => {
    const l = lang(it);
    if (!l || l.includes("-")) return l;
    return known.get(`${term(it)}\t${primaryLang(l)}`) ?? l;
  });
}

/**
 * The `lang` of a result's heading, from its entries, which may be from several
 * dictionaries: of those in the search language (any, with none chosen), one whose
 * glossary names the term's script (CC-CEDICT's `zh-Hans`) first, else the first entry's.
 */
export function headLang(entries: ITerm[], fallback = ""): string | undefined {
  const known = entries.flatMap((e) => {
    const lang = termLang(e);
    return lang ? [{ lang, declared: !!declaredLang(e) }] : [];
  });
  const inSearchLang = fallback
    ? known.filter((c) => primaryLang(c.lang) === primaryLang(fallback))
    : [];
  const pool = inSearchLang.length ? inSearchLang : known;
  return (
    (pool.find((c) => c.declared) ?? pool[0])?.lang ?? (fallback || undefined)
  );
}

/**
 * The `lang` of an entry's definitions, for the forms of the Han characters in them: the
 * dictionary's target language (L2) when it is Japanese, Chinese or Korean (Pixiv's `ja`),
 * else its source language (L1): the Han characters in wty-ja-en's English definitions are
 * Japanese, and a dictionary without an L2 is taken to be written in L1 (ja.Wikipedia).
 * Elements the dictionary marks itself keep their `lang`.
 */
export function glossLang(
  row: Pick<ITerm, "lang" | "target_lang">,
): string | undefined {
  const l2 = row.target_lang;
  if (l2 && CJK.includes(primaryLang(l2))) return l2;
  return row.lang || l2 || undefined;
}
