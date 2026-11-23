/**
 * Japanese pitch accents, as pitch accent dictionaries (e.g. Kanjium) give them: Yomitan
 * term meta of mode `pitch`. Drawn as Yomitan draws them (its `pronunciation-generator.js`
 * and `japanese.js`), reimplemented here.
 *
 * @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-meta-bank-v3-schema.json
 */

/** One pitch accent of a reading */
export interface IPitch {
  /** the mora after which the pitch drops (0: it doesn't), or a pattern of H and L */
  position: number | string;
  /** morae that are nasal, from 1 */
  nasal?: number | number[];
  /** morae that are devoiced, from 1 */
  devoice?: number | number[];
  /** parts of speech it is for, e.g. `副`; none: all */
  tags?: string[];
}

/** What a dictionary's tag banks say about a tag */
export interface IPitchTagMeta {
  category: string;
  order: number;
  notes: string;
}

/** A pitch accent dictionary's entry for a word: the same term and reading as an entry's */
export interface IPitchEntry {
  dict_id: number;
  dict_path: string;
  term: string;
  reading: string;
  pitches: IPitch[];
  /** what the dictionary's tag banks say about the pitches' tags */
  tag_meta: Record<string, IPitchTagMeta>;
}

/** A word's spelling and reading; an entry without a reading is read as it is written */
export interface IHeadword {
  term: string;
  reading: string;
}

/** The entries' headwords, each once: `reading` is the term where an entry has none */
export function headwordsOf(
  entries: { term: string; reading: string }[],
): IHeadword[] {
  const out: IHeadword[] = [];
  for (const e of entries) {
    const h = { term: e.term, reading: e.reading || e.term };
    if (!out.some((x) => x.term === h.term && x.reading === h.reading)) {
      out.push(h);
    }
  }
  return out;
}

/** small kana that share a mora with the kana before: きゃ, ファ */
const SMALL = /[ぁぃぅぇぉゃゅょゎァィゥェォャュョヮ]/u;

/** The morae of a kana reading: `きゃく` -> `きゃ`, `く`; っ, ん and ー are morae too. */
export function morae(reading: string): string[] {
  const out: string[] = [];
  for (const ch of reading) {
    if (out.length && SMALL.test(ch)) {
      out[out.length - 1] += ch;
    } else {
      out.push(ch);
    }
  }
  return out;
}

/**
 * Whether mora `i` (from 0) is high. `i` may be the word's length: the particle after it.
 * A downstep `n` is low then high (0), high then low (1), or low, high up to `n`, then low.
 */
export function isHigh(i: number, position: number | string): boolean {
  if (typeof position === "string") return position[i] === "H";
  switch (position) {
    case 0:
      return i > 0;
    case 1:
      return i < 1;
    default:
      return i > 0 && i < position;
  }
}

/**
 * A position as given (`2`), or a pattern of H and L made uppercase (`LHHL`); a downstep
 * written as a string (`"2"`) is a number.
 */
function normalPosition(position: number | string): number | string {
  if (typeof position === "number") return position;
  const p = position.trim();
  return /^\d+$/.test(p) ? Number(p) : p.toUpperCase();
}

/**
 * The downsteps: the number given, or where a pattern of H and L drops (`LHHL` -> 3);
 * a pattern that never drops is 0 if it starts low, else -1.
 */
export function downsteps(position: number | string): number[] {
  const p = normalPosition(position);
  if (typeof p === "number") return [p];
  const out: number[] = [];
  for (let i = 1; i < p.length; i++) {
    if (p[i - 1] === "H" && p[i] === "L") out.push(i);
  }
  return out.length ? out : [p.startsWith("L") ? 0 : -1];
}

export interface IMora {
  text: string;
  high: boolean;
  /** the pitch drops after it */
  drop: boolean;
  nasal: boolean;
  devoice: boolean;
}

function positions(v: number | number[] | undefined): number[] {
  return v === undefined ? [] : Array.isArray(v) ? v : [v];
}

/** Each mora's pitch, high or low, where it drops, and whether it is nasal or devoiced */
export function pitchMorae(reading: string, pitch: IPitch): IMora[] {
  const p = normalPosition(pitch.position);
  const nasal = new Set(positions(pitch.nasal));
  const devoice = new Set(positions(pitch.devoice));
  return morae(reading).map((text, i) => ({
    text,
    high: isHigh(i, p),
    drop: isHigh(i, p) && !isHigh(i + 1, p),
    nasal: nasal.has(i + 1),
    devoice: devoice.has(i + 1),
  }));
}

/**
 * A nasal mora's kana without its dakuten, which the nasal mark stands for: が -> か. Other
 * kana are left as they are.
 */
export function withoutDakuten(text: string): string {
  const [first, ...rest] = [...text];
  const nfd = first.normalize("NFD");
  return nfd.length === 2 && nfd[1] === "゙" ? nfd[0] + rest.join("") : text;
}

export type PitchKind =
  | "heiban"
  | "atamadaka"
  | "nakadaka"
  | "odaka"
  | "kifuku";

/**
 * Whether a word with these deinflection rules (space-separated, e.g. `v5`) is a verb or
 * an i-adjective rather than a noun, as Yomitan tells: a noun that is also a suru verb is
 * a noun.
 */
export function isVerbOrAdjective(rules: string | null | undefined): boolean {
  const r = new Set((rules ?? "").split(/\s+/).filter(Boolean));
  const verbOrAdjective = ["v1", "v5", "vk", "vz", "vs", "adj-i"].some((c) =>
    r.has(c),
  );
  return verbOrAdjective && !(r.has("vs") && r.has("n"));
}

/**
 * heiban (no drop); for verbs and i-adjectives, kifuku (a drop); else atamadaka (after the
 * first mora), odaka (after the last) or nakadaka (inside)
 */
export function pitchKind(
  reading: string,
  position: number | string,
  verbOrAdjective = false,
): PitchKind | null {
  const n = downsteps(position)[0];
  if (n === 0) return "heiban";
  if (verbOrAdjective) return n > 0 ? "kifuku" : null;
  if (n === 1) return "atamadaka";
  if (n > 1) return n >= morae(reading).length ? "odaka" : "nakadaka";
  return null;
}

/**
 * The pitch patterns of a headword, space-separated, as Yomitan puts them on it
 * (`data-pronunciation-categories`), e.g. `heiban odaka`; empty if it has none.
 */
export function pronunciationCategories(
  headword: IHeadword,
  entries: IPitchEntry[],
  verbOrAdjective = false,
): string {
  const kinds = new Set<PitchKind>();
  for (const e of entries) {
    if (e.term !== headword.term || e.reading !== headword.reading) continue;
    for (const p of e.pitches) {
      const kind = pitchKind(e.reading, p.position, verbOrAdjective);
      if (kind) kinds.add(kind);
    }
  }
  return [...kinds].join(" ");
}

/** One pitch of a dictionary, with the headwords it is for */
export interface IGroupedPitch {
  reading: string;
  pitch: IPitch;
  /** its tags, with what the tag banks say about them */
  tags: { name: string; meta?: IPitchTagMeta }[];
  /** the terms it is for, when not all of the word's: shown before it */
  exclusiveTerms: string[];
  /** its reading, when the word has more than one (shown by its text) */
  exclusiveReadings: string[];
}

/** A dictionary's pitch accents for a word */
export interface IPitchGroup {
  dict_path: string;
  pitches: IGroupedPitch[];
}

function samePositions(a?: number | number[], b?: number | number[]): boolean {
  const x = positions(a);
  const y = positions(b);
  return x.length === y.length && x.every((v, i) => v === y[i]);
}

function samePitch(a: IPitch, b: IPitch): boolean {
  const ta = a.tags ?? [];
  const tb = b.tags ?? [];
  return (
    a.position === b.position &&
    samePositions(a.nasal, b.nasal) &&
    samePositions(a.devoice, b.devoice) &&
    ta.length === tb.length &&
    ta.every((t) => tb.includes(t))
  );
}

/**
 * The entries grouped as Yomitan groups them: by dictionary, in the order they come; the
 * same pitch of the same reading once, with every term it is for.
 */
export function groupPronunciations(
  entries: IPitchEntry[],
  headwords: IHeadword[],
): IPitchGroup[] {
  const allTerms = new Set(headwords.map((h) => h.term));
  const multipleReadings = new Set(headwords.map((h) => h.reading)).size > 1;
  const groups: {
    dict_path: string;
    pitches: (Omit<IGroupedPitch, "exclusiveTerms" | "exclusiveReadings"> & {
      terms: Set<string>;
    })[];
  }[] = [];
  for (const e of entries) {
    let g = groups.find((x) => x.dict_path === e.dict_path);
    if (!g) {
      g = { dict_path: e.dict_path, pitches: [] };
      groups.push(g);
    }
    for (const pitch of e.pitches) {
      let found = g.pitches.find(
        (x) => x.reading === e.reading && samePitch(x.pitch, pitch),
      );
      if (!found) {
        found = {
          reading: e.reading,
          pitch,
          tags: (pitch.tags ?? []).map((name) => ({
            name,
            meta: e.tag_meta?.[name],
          })),
          terms: new Set(),
        };
        g.pitches.push(found);
      }
      found.terms.add(e.term);
    }
  }
  return groups.map((g) => ({
    dict_path: g.dict_path,
    pitches: g.pitches.map(({ terms, ...p }) => ({
      ...p,
      exclusiveTerms:
        terms.size === allTerms.size && [...terms].every((t) => allTerms.has(t))
          ? []
          : [...terms].filter((t) => allTerms.has(t)),
      exclusiveReadings: multipleReadings ? [p.reading] : [],
    })),
  }));
}
