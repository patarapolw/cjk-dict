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

/** A pitch accent dictionary's entry for a word */
export interface IPitchEntry {
  dict_id: number;
  dict_path: string;
  /** as the dictionary has it */
  reading: string;
  pitches: IPitch[];
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

const KIND_NAMES: Record<PitchKind, string> = {
  heiban: "Heiban (平板): no drop",
  atamadaka: "Atamadaka (頭高): drops after the first mora",
  nakadaka: "Nakadaka (中高): drops inside the word",
  odaka: "Odaka (尾高): drops after the last mora, before a particle",
  kifuku: "Kifuku (起伏): drops",
};

/** What the pitch is, for a tooltip: `Nakadaka (中高): drops inside the word` */
export function pitchDescription(
  reading: string,
  pitch: IPitch,
  verbOrAdjective = false,
): string {
  const kind = pitchKind(reading, pitch.position, verbOrAdjective);
  const parts = [kind ? KIND_NAMES[kind] : `Pitch: ${pitch.position}`];
  if (positions(pitch.nasal).length) parts.push("○: nasal (鼻濁音)");
  if (positions(pitch.devoice).length) parts.push("dotted circle: devoiced");
  return parts.join("; ");
}

/** A dictionary's pitch accents for a word, one per reading and pitch */
export interface IPitchGroup {
  dict_path: string;
  pitches: { reading: string; pitch: IPitch }[];
}

/** The entries grouped by dictionary, in the order they come */
export function groupByDictionary(entries: IPitchEntry[]): IPitchGroup[] {
  const out: IPitchGroup[] = [];
  for (const e of entries) {
    let g = out.find((x) => x.dict_path === e.dict_path);
    if (!g) {
      g = { dict_path: e.dict_path, pitches: [] };
      out.push(g);
    }
    for (const pitch of e.pitches)
      g.pitches.push({ reading: e.reading, pitch });
  }
  return out;
}
