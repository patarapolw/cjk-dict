//! Each Han character's reading within a term, to find the words with a character read a
//! given way anywhere in them (`*<興>{キョウ}`: 興味, 余興, ...) by an index lookup.
//!
//! - Japanese: the term's reading is split among its kanji by the readings kanji dictionaries
//!   give them (KANJIDIC's on, kun and nanori: [`KanaReadings`]), allowing for the sound
//!   changes of compounds: voicing (か → が), gemination (つ → っ), and 々 repeating the
//!   kanji before it. A character's reading is kept as the dictionary has it (興 in 余興 is
//!   きょう; in a word reading it ぎょう, still きょう), and a kun reading up to its okurigana
//!   (おこ.る: おこ). Terms whose reading belongs to the whole word (馬酔木: あせび) are left
//!   out.
//! - Chinese: one pinyin syllable per character.
//! - Korean: one Hangul syllable per Hanja. The initial-sound rule (李 is 이 at the start of a
//!   word, 리 elsewhere) is left to the query: [`query_forms`] gives both.
//!
//! Which language a term's reading is in is told by its script, not by its dictionary.

use std::collections::{HashMap, HashSet};

use crate::{
    ids::is_han,
    pinyin::{expand_pinyin, normalize_pinyin},
    util::fold_kana,
};

/// Kanji readings in hiragana, from kanji dictionaries, with the forms they take in
/// compounds: (form, reading it is a form of), longest first.
#[derive(Default)]
pub struct KanaReadings {
    forms: HashMap<char, Vec<(String, String)>>,
}

impl KanaReadings {
    /// Add a kanji's readings as a kanji dictionary gives them, e.g. `コウ キョウ` (on) and
    /// `おこ.る おこ.す` (kun). Readings that are not kana (pinyin, Hangul) are ignored.
    pub fn add(&mut self, kanji: &str, readings: &str) {
        let mut cs = kanji.chars();
        let (Some(k), None) = (cs.next(), cs.next()) else {
            return;
        };
        for r in readings.split([' ', ',', '、']) {
            let base = fold_kana(r.trim().trim_matches(['-', '‐']))
                .split('.')
                .next()
                .unwrap_or("")
                .to_string();
            if base.is_empty() || !base.chars().all(is_kana) {
                continue;
            }
            let forms = self.forms.entry(k).or_default();
            for f in compound_forms(&base) {
                if !forms.iter().any(|(x, b)| *x == f && *b == base) {
                    forms.push((f, base.clone()));
                }
            }
            forms.sort_by_key(|f| std::cmp::Reverse(f.0.chars().count()));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.forms.is_empty()
    }

    /// kanji with readings
    pub fn len(&self) -> usize {
        self.forms.len()
    }
}

fn is_hangul_syllable(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{3096}' | 'ー')
}

const UNVOICED: &str = "かきくけこさしすせそたちつてとはひふへほはひふへほ";
const VOICED: &str = "がぎぐげござじずぜぞだぢづでどばびぶべぼぱぴぷぺぽ";

/// `base` and the forms it takes in compounds: voiced (か → が, は → ば, ぱ) and geminated
/// (つ, ち, く, き at the end → っ)
fn compound_forms(base: &str) -> Vec<String> {
    let mut out = vec![base.to_string()];
    let mut cs = base.chars();
    if let Some(first) = cs.next() {
        let rest: String = cs.collect();
        for (u, v) in UNVOICED.chars().zip(VOICED.chars()) {
            if u == first {
                out.push(format!("{v}{rest}"));
            }
        }
    }
    for f in out.clone() {
        let n = f.chars().count();
        if n > 1 && f.ends_with(['つ', 'ち', 'く', 'き']) {
            let stem: String = f.chars().take(n - 1).collect();
            out.push(format!("{stem}っ"));
        }
    }
    out
}

/// The characters of `term` with their readings, from the term's `reading`; empty if they
/// can't be told apart. `term` and `reading` as search.db has them (katakana as hiragana,
/// pinyin as `XING1 QU4`).
pub fn align(term: &str, reading: &str, kana: &KanaReadings) -> Vec<(String, String)> {
    let reading = reading.trim();
    if reading.is_empty() || !term.chars().any(is_han) {
        return vec![];
    }
    let r = if reading.chars().any(is_hangul_syllable) {
        align_ko(term, reading)
    } else if reading.chars().any(is_kana) {
        align_ja(term, reading, kana)
    } else {
        align_zh(term, reading)
    };
    r.unwrap_or_default()
}

fn align_zh(term: &str, reading: &str) -> Option<Vec<(String, String)>> {
    let syllables: Vec<&str> = reading.split_whitespace().collect();
    let chars: Vec<char> = term.chars().collect();
    if syllables.len() != chars.len() || !chars.iter().all(|&c| is_han(c)) {
        return None;
    }
    Some(
        chars
            .iter()
            .zip(syllables)
            .map(|(c, s)| (c.to_string(), s.to_string()))
            .collect(),
    )
}

fn align_ko(term: &str, reading: &str) -> Option<Vec<(String, String)>> {
    let syllables: Vec<char> = reading.chars().filter(|c| !c.is_whitespace()).collect();
    let chars: Vec<char> = term.chars().filter(|c| !c.is_whitespace()).collect();
    if syllables.len() != chars.len() {
        return None;
    }
    let mut out = vec![];
    for (&c, &s) in chars.iter().zip(&syllables) {
        if is_han(c) {
            if !is_hangul_syllable(s) {
                return None;
            }
            out.push((c.to_string(), s.to_string()));
        } else if c != s {
            return None;
        }
    }
    Some(out)
}

fn align_ja(term: &str, reading: &str, kana: &KanaReadings) -> Option<Vec<(String, String)>> {
    let term: Vec<char> = term.chars().collect();
    let reading: Vec<char> = reading.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = vec![];
    let mut failed = HashSet::new();
    go(&term, &reading, 0, 0, None, kana, &mut out, &mut failed).then_some(out)
}

/// Match `term[i..]` to `reading[j..]`, depth first, the longest readings first; `failed`
/// remembers the positions that can't be matched, which keeps long terms linear.
#[allow(clippy::too_many_arguments)]
fn go(
    term: &[char],
    reading: &[char],
    i: usize,
    j: usize,
    prev: Option<char>,
    kana: &KanaReadings,
    out: &mut Vec<(String, String)>,
    failed: &mut HashSet<(usize, usize)>,
) -> bool {
    if i == term.len() {
        return j == reading.len();
    }
    if failed.contains(&(i, j)) {
        return false;
    }
    let c = term[i];
    if !is_han(c) {
        if reading.get(j) == Some(&c) && go(term, reading, i + 1, j + 1, prev, kana, out, failed) {
            return true;
        }
        failed.insert((i, j));
        return false;
    }
    // 々 reads as the kanji before it
    let k = if c == '々' { prev } else { Some(c) };
    if let Some(forms) = k.and_then(|k| kana.forms.get(&k)) {
        for (form, base) in forms {
            let n = form.chars().count();
            if j + n <= reading.len() && reading[j..j + n].iter().copied().eq(form.chars()) {
                out.push((k.unwrap().to_string(), base.clone()));
                if go(term, reading, i + 1, j + n, k, kana, out, failed) {
                    return true;
                }
                out.pop();
            }
        }
    }
    failed.insert((i, j));
    false
}

/// The readings to look a character up by, for a reading typed or clicked: its own, as
/// search.db keeps characters' readings.
///
/// - kana: in hiragana, and as the dictionary form of a compound's sound change (ぎょう → きょう,
///   がっ → がく ...)
/// - Hangul: with the initial-sound rule both ways (리 → 이, 이 → 리 ...)
/// - pinyin: as `XING1`, every tone if it has none
pub fn query_forms(reading: &str) -> Vec<String> {
    let r = reading.trim();
    let mut out: Vec<String> = vec![];
    if r.chars().any(is_hangul_syllable) {
        let s: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        let mut cs = s.chars();
        match (cs.next(), cs.next()) {
            (Some(c), None) => {
                for f in initial_sound_forms(c) {
                    push(&mut out, f.to_string());
                }
            }
            _ => push(&mut out, s),
        }
    } else if r.chars().any(is_kana_or_katakana) {
        let h = fold_kana(r).split('.').next().unwrap_or("").to_string();
        push(&mut out, h.clone());
        // a compound's form back to the dictionary's: voiced first kana, っ at the end
        let mut cs = h.chars();
        if let Some(first) = cs.next() {
            let rest: String = cs.collect();
            for (u, v) in UNVOICED.chars().zip(VOICED.chars()) {
                if v == first {
                    push(&mut out, format!("{u}{rest}"));
                }
            }
        }
        for f in out.clone() {
            if let Some(stem) = f.strip_suffix('っ') {
                for k in ['つ', 'ち', 'く', 'き'] {
                    push(&mut out, format!("{stem}{k}"));
                }
            }
        }
    } else {
        let expanded = expand_pinyin(r);
        if expanded.is_empty() {
            push(&mut out, r.to_uppercase());
        }
        for e in expanded {
            push(&mut out, e.to_uppercase());
        }
    }
    out
}

/// A character's readings as a kanji dictionary lists them (`コウ キョウ`, `おこ.る`, `yǔ, yù`,
/// `흥`), as `han_reading` keeps them: kana in hiragana, a kun reading both up to its
/// okurigana and whole (おこ and おこる), pinyin as `YU3`, Hangul as it is. Readings that are
/// none of these are left out.
pub fn stored_forms(readings: &str) -> Vec<String> {
    let mut out = vec![];
    for r in readings.split([' ', ',', '、']) {
        let r = r.trim().trim_matches(['-', '‐']);
        if r.is_empty() {
            continue;
        }
        if r.chars().all(is_hangul_syllable) {
            push(&mut out, r.to_string());
        } else if r.chars().all(|c| is_kana_or_katakana(c) || c == '.') {
            let h = fold_kana(r);
            push(&mut out, h.split('.').next().unwrap_or("").to_string());
            push(&mut out, h.replace('.', ""));
        } else if let Some(p) = normalize_pinyin(r).filter(|p| !p.contains(' ')) {
            push(&mut out, p.to_uppercase());
        }
    }
    out
}

/// The readings to look characters up by, for a reading typed: as [`stored_forms`] keeps
/// them. Unlike [`query_forms`], kana are taken as typed, not as a compound's sound change
/// (ぎょう finds 行, not 京); Hangul and pinyin are as there. With `romanized`, Latin letters
/// are Korean romanization rather than pinyin: `heung` is 흥, `i` is 이 and 리
/// ([`crate::korean::syllables_of`]).
pub fn lookup_forms(reading: &str, romanized: bool) -> Vec<String> {
    let r = reading.trim().trim_matches(['-', '‐']);
    if romanized && crate::korean::is_romanization(r) {
        let mut out = vec![];
        for s in crate::korean::syllables_of(r) {
            for f in initial_sound_forms(s) {
                push(&mut out, f.to_string());
            }
        }
        return out;
    }
    if !r.chars().any(is_hangul_syllable) && r.chars().any(is_kana_or_katakana) {
        let h: String = fold_kana(r)
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '.')
            .collect();
        return if h.is_empty() { vec![] } else { vec![h] };
    }
    query_forms(r)
}

fn push(out: &mut Vec<String>, s: String) {
    if !s.is_empty() && !out.contains(&s) {
        out.push(s);
    }
}

fn is_kana_or_katakana(c: char) -> bool {
    is_kana(c) || matches!(c, '\u{30A1}'..='\u{30FA}')
}

/// A Hangul syllable and the forms the initial-sound rule (두음법칙) makes of it or it is made
/// from: ㄹ → ㄴ, and ㄹ, ㄴ → ㅇ before ㅣ and the y- vowels (리 ↔ 이, 락 ↔ 낙, 녀 ↔ 여 ↔ 려).
fn initial_sound_forms(c: char) -> Vec<char> {
    const L: u32 = 5; // ㄹ
    const N: u32 = 2; // ㄴ
    const NG: u32 = 11; // ㅇ
    let code = c as u32;
    if !(0xAC00..=0xD7A3).contains(&code) {
        return vec![c];
    }
    let s = code - 0xAC00;
    let (lead, vowel, tail) = (s / (21 * 28), (s / 28) % 21, s % 28);
    // ㅑ ㅕ ㅖ ㅛ ㅠ ㅣ
    let iy = matches!(vowel, 2 | 6 | 7 | 12 | 17 | 20);
    let leads: &[u32] = match (lead, iy) {
        (L | N | NG, true) => &[L, N, NG],
        (L | N, false) => &[L, N],
        _ => &[lead],
    };
    let mut out = vec![c];
    for &l in leads {
        let f = char::from_u32(0xAC00 + (l * 21 + vowel) * 28 + tail).unwrap();
        if !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kana() -> KanaReadings {
        let mut k = KanaReadings::default();
        k.add("興", "コウ キョウ おこ.る おこ.す");
        k.add("味", "ミ あじ あじ.わう");
        k.add("余", "ヨ あま.る");
        k.add("学", "ガク まな.ぶ");
        k.add("校", "コウ キョウ");
        k.add("人", "ジン ニン ひと");
        k.add("日", "ニチ ジツ ひ -び -か");
        k.add("本", "ホン もと");
        k.add("時", "ジ とき");
        k.add("馬", "バ うま");
        k.add("酔", "スイ よ.う");
        k.add("木", "ボク モク き こ-");
        k.add("中", "チュウ なか");
        k.add("国", "コク くに");
        k.add("林", "pinyin lin2"); // not kana: ignored
        k
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn japanese_readings_split_among_the_kanji() {
        let k = kana();
        assert_eq!(
            align("興味", "きょうみ", &k),
            pairs(&[("興", "きょう"), ("味", "み")])
        );
        assert_eq!(
            align("余興", " よきょう ", &k),
            pairs(&[("余", "よ"), ("興", "きょう")])
        );
        // okurigana, and a kun reading kept up to it
        assert_eq!(align("興る", "おこる", &k), pairs(&[("興", "おこ")]));
        // gemination and voicing, kept as the dictionary's readings
        assert_eq!(
            align("学校", "がっこう", &k),
            pairs(&[("学", "がく"), ("校", "こう")])
        );
        assert_eq!(
            align("時々", "ときどき", &k),
            pairs(&[("時", "とき"), ("時", "とき")])
        );
        assert_eq!(
            align("人々", "ひとびと", &k),
            pairs(&[("人", "ひと"), ("人", "ひと")])
        );
        assert_eq!(
            align("中国", "ちゅうごく", &k),
            pairs(&[("中", "ちゅう"), ("国", "こく")])
        );
        // the reading belongs to the whole word
        assert!(align("馬酔木", "あせび", &k).is_empty());
        // no reading, or no kanji
        assert!(align("興味", "", &k).is_empty());
        assert!(align("こーひー", "こーひー", &k).is_empty());
        // a kanji the dictionaries don't have
        assert!(align("鬱", "うつ", &k).is_empty());
    }

    #[test]
    fn chinese_has_a_syllable_per_character() {
        let k = KanaReadings::default();
        assert_eq!(
            align("興趣", " XING4 QU4 ", &k),
            pairs(&[("興", "XING4"), ("趣", "QU4")])
        );
        assert!(align("家雀兒", " JIA1 QIAO3 R5 ", &k).len() == 3);
        assert!(align("家雀兒", " JIA1 QIAO3R ", &k).is_empty());
        assert!(align("3D打印", " SAN1 D DA3 YIN4 ", &k).is_empty());
    }

    #[test]
    fn korean_has_a_syllable_per_hanja() {
        let k = KanaReadings::default();
        assert_eq!(
            align("復興", " 부흥 ", &k),
            pairs(&[("復", "부"), ("興", "흥")])
        );
        assert_eq!(align("마당劇", " 마당극 ", &k), pairs(&[("劇", "극")]));
        assert!(align("麻袋", " 마 ", &k).is_empty());
    }

    #[test]
    fn a_characters_readings_are_stored_by_script() {
        assert_eq!(stored_forms("コウ キョウ"), ["こう", "きょう"]);
        assert_eq!(stored_forms("おこ.る -び"), ["おこ", "おこる", "び"]);
        assert_eq!(stored_forms("yǔ, yù"), ["YU3", "YU4"]);
        assert_eq!(stored_forms("xing1"), ["XING1"]);
        assert_eq!(stored_forms("악 락"), ["악", "락"]);
        // neither kana, Hangul nor pinyin
        assert!(stored_forms("jouyou").is_empty());
    }

    #[test]
    fn a_typed_reading_looks_characters_up_as_stored() {
        assert_eq!(lookup_forms("キョウ", false), ["きょう"]);
        assert_eq!(lookup_forms("ぎょう", false), ["ぎょう"]);
        assert_eq!(lookup_forms("おこ.る", false), ["おこる"]);
        assert_eq!(lookup_forms("xīng", false), ["XING1"]);
        assert_eq!(lookup_forms("xing", false).len(), 5);
        assert!(lookup_forms("이", false).contains(&"리".to_string()));
        assert!(lookup_forms("", false).is_empty());
        // Korean romanization, loosely as in Words, and the initial-sound rule
        // eu and u alike
        assert_eq!(lookup_forms("heung", true), ["훙", "흥"]);
        assert_eq!(lookup_forms("hung", true), ["훙", "흥"]);
        for (typed, hangul) in [("lim", "림"), ("rim", "림"), ("i", "리"), ("ri", "이")] {
            assert!(
                lookup_forms(typed, true).contains(&hangul.to_string()),
                "{typed}: {hangul}"
            );
        }
        assert!(lookup_forms("qqq", true).is_empty());
        // kana and Hangul are themselves
        assert_eq!(lookup_forms("きょう", true), ["きょう"]);
    }

    #[test]
    fn query_forms_cover_how_a_reading_is_stored() {
        assert_eq!(query_forms("キョウ"), ["きょう"]);
        assert_eq!(query_forms("ぎょう"), ["ぎょう", "きょう"]);
        assert_eq!(
            query_forms("がっ"),
            [
                "がっ", "かっ", "がつ", "がち", "がく", "がき", "かつ", "かち", "かく", "かき"
            ]
        );
        assert_eq!(query_forms("おこ.る"), ["おこ"]);
        assert_eq!(query_forms("리"), ["리", "니", "이"]);
        assert_eq!(query_forms("이"), ["이", "리", "니"]);
        assert_eq!(query_forms("락"), ["락", "낙"]);
        assert_eq!(query_forms("흥"), ["흥"]);
        assert_eq!(query_forms("xīng"), ["XING1"]);
        assert_eq!(query_forms("xing").len(), 5);
    }
}
