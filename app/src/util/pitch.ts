/**
 * Japanese pitch accents, as pitch accent dictionaries (e.g. Kanjium) give them: Yomitan
 * term meta of mode `pitch`.
 *
 * @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-meta-bank-v3-schema.json
 */

/** One pitch accent of a reading */
export interface IPitch {
  /** the mora after which the pitch drops (0: it doesn't), or a pattern of H and L */
  position: number | string;
  /** morae that are nasal */
  nasal?: number | number[];
  /** morae that are devoiced */
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
const SMALL = /[ゃゅょぁぃぅぇぉゎャュョァィゥェォヮ]/u;

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

export interface IMora {
  text: string;
  high: boolean;
  /** the pitch drops after it */
  drop: boolean;
}

export type PitchKind = "heiban" | "atamadaka" | "nakadaka" | "odaka";

/** The downstep as a number, if `position` gives one: `2`, `"2"`; not `"LHHL"`. */
export function downstep(position: number | string): number | null {
  if (typeof position === "number") return position;
  return /^\d+$/.test(position.trim()) ? Number(position) : null;
}

/**
 * Each mora's pitch, high or low, and where it drops, for a downstep (`2`) or a pattern of
 * H and L (`LHHL`, which may give the particle after the word one more letter).
 */
export function pitchMorae(
  reading: string,
  position: number | string,
): IMora[] {
  const ms = morae(reading);
  const n = downstep(position);
  let highs: boolean[];
  /** the pitch after the word, on a particle: whether it is low after a high mora */
  let dropsAfter: boolean;
  if (n !== null) {
    // 0: low, then high; 1: high, then low; n: low, high up to n, then low
    highs = ms.map((_, i) => (n === 1 ? i === 0 : i > 0 && (n === 0 || i < n)));
    dropsAfter = n > 0 && n >= ms.length;
  } else {
    const p = String(position).trim().toUpperCase();
    highs = ms.map((_, i) => p[i] === "H");
    dropsAfter = p[ms.length] === "L";
  }
  return ms.map((text, i) => ({
    text,
    high: highs[i],
    drop: highs[i] && (i + 1 < ms.length ? !highs[i + 1] : dropsAfter),
  }));
}

/** heiban (no drop), atamadaka (after the first mora), odaka (after the last), else nakadaka */
export function pitchKind(
  reading: string,
  position: number | string,
): PitchKind | null {
  const n = downstep(position);
  if (n === null) return null;
  const count = morae(reading).length;
  if (n === 0) return "heiban";
  if (n === 1) return "atamadaka";
  if (n >= count) return "odaka";
  return "nakadaka";
}

const KIND_NAMES: Record<PitchKind, string> = {
  heiban: "Heiban (平板): no drop",
  atamadaka: "Atamadaka (頭高): drops after the first mora",
  nakadaka: "Nakadaka (中高): drops inside the word",
  odaka: "Odaka (尾高): drops after the last mora, before a particle",
};

/** What the pitch is, for a tooltip: `Nakadaka (中高): drops inside the word` */
export function pitchDescription(
  reading: string,
  position: number | string,
): string {
  const kind = pitchKind(reading, position);
  if (!kind) return `Pitch: ${position}`;
  return KIND_NAMES[kind];
}
