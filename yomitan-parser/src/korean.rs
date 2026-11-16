//! Searching Korean by romanization: `hangugeo` finds 한국어.
//!
//! Hangul terms are romanized at query time with the Revised Romanization, which follows
//! pronunciation (국물 → gungmul, 신라 → Silla, 같이 → gachi), and also letter by letter
//! (국물 → gukmul), since people type either. Both sides are then [`loosen`]ed, so that
//! `eo`/`o`, `eu`/`u`, `ae`/`e`, `k`/`g`, `t`/`d`, `p`/`b` and `r`/`l` match each other.

const SYLLABLE_FIRST: u32 = 0xAC00;
const SYLLABLE_LAST: u32 = 0xD7A3;
/// syllables per initial consonant (21 vowels × 28 finals)
const PER_INITIAL: u32 = 588;

const INITIALS: [&str; 19] = [
    "g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p",
    "h",
];
const VOWELS: [&str; 21] = [
    "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we",
    "wi", "yu", "eu", "ui", "i",
];

// initial consonants (choseong), by index
const G: u8 = 0;
const N: u8 = 2;
const D: u8 = 3;
const R: u8 = 5;
const M: u8 = 6;
const B: u8 = 7;
const S: u8 = 9;
const NULL: u8 = 11;
const J: u8 = 12;
const CH: u8 = 14;
const K: u8 = 15;
const T: u8 = 16;
const P: u8 = 17;
const H: u8 = 18;
/// vowel ㅣ
const I: u8 = 20;

// final consonants (jongseong), by index; 0 = none
const F_K: u8 = 1;
const F_N: u8 = 4;
const F_NH: u8 = 6;
const F_T: u8 = 7;
const F_L: u8 = 8;
const F_LH: u8 = 15;
const F_M: u8 = 16;
const F_P: u8 = 17;
const F_NG: u8 = 21;
const F_H: u8 = 27;

#[derive(Clone, Copy)]
struct Syllable {
    initial: u8,
    vowel: u8,
    fin: u8,
}

fn decompose(c: char) -> Option<Syllable> {
    let u = c as u32;
    if !(SYLLABLE_FIRST..=SYLLABLE_LAST).contains(&u) {
        return None;
    }
    let s = u - SYLLABLE_FIRST;
    Some(Syllable {
        initial: (s / PER_INITIAL) as u8,
        vowel: (s % PER_INITIAL / 28) as u8,
        fin: (s % 28) as u8,
    })
}

/// The sound a final consonant is pronounced as before another consonant or at the end:
/// one of none, ㄱ, ㄴ, ㄷ, ㄹ, ㅁ, ㅂ, ㅇ.
fn neutralize(fin: u8) -> u8 {
    match fin {
        1 | 2 | 3 | 9 | 24 => F_K,              // ㄱ ㄲ ㄳ ㄺ ㅋ
        4..=6 => F_N,                           // ㄴ ㄵ ㄶ
        7 | 19 | 20 | 22 | 23 | 25 | 27 => F_T, // ㄷ ㅅ ㅆ ㅈ ㅊ ㅌ ㅎ
        8 | 11 | 12 | 13 | 15 => F_L,           // ㄹ ㄼ ㄽ ㄾ ㅀ
        10 | 16 => F_M,                         // ㄻ ㅁ
        14 | 17 | 18 | 26 => F_P,               // ㄿ ㅂ ㅄ ㅍ
        21 => F_NG,
        _ => 0,
    }
}

/// A final consonant as (what stays, the initial consonant that moves to a following
/// vowel). `None`: it has no sound of its own there (ㅇ, or nothing).
fn split_final(fin: u8) -> (u8, Option<u8>) {
    match fin {
        1 => (0, Some(G)),
        2 => (0, Some(1)), // ㄲ
        3 => (F_K, Some(S)),
        4 => (0, Some(N)),
        5 => (F_N, Some(J)),
        F_NH => (F_N, Some(H)),
        F_T => (0, Some(D)),
        F_L => (0, Some(R)),
        9 => (F_L, Some(G)),
        10 => (F_L, Some(M)),
        11 => (F_L, Some(B)),
        12 => (F_L, Some(S)),
        13 => (F_L, Some(T)),
        14 => (F_L, Some(P)),
        F_LH => (F_L, Some(H)),
        F_M => (0, Some(M)),
        F_P => (0, Some(B)),
        18 => (F_P, Some(S)),
        19 => (0, Some(S)),
        20 => (0, Some(10)), // ㅆ
        22 => (0, Some(J)),
        23 => (0, Some(CH)),
        24 => (0, Some(K)),
        25 => (0, Some(T)),
        26 => (0, Some(P)),
        F_H => (0, Some(H)),
        _ => (fin, None),
    }
}

fn aspirate(initial: u8) -> u8 {
    match initial {
        G => K,
        D => T,
        B => P,
        J => CH,
        x => x,
    }
}

/// Sound changes between a syllable's final consonant and the next syllable's initial,
/// as far as the Revised Romanization writes them (not tensing).
fn sound_change(a: &mut Syllable, b: &mut Syllable) {
    if a.fin == 0 {
        return;
    }
    if b.initial == NULL {
        // liaison: 한국어 → 한구거, 앉아 → 안자; ㅎ is silent: 좋아 → 조아
        if let (keep, Some(mut moved)) = split_final(a.fin) {
            a.fin = keep;
            if moved != H {
                // palatalization: 굳이 → 구지, 같이 → 가치
                if b.vowel == I {
                    moved = match moved {
                        D => J,
                        T => CH,
                        x => x,
                    };
                }
                b.initial = moved;
            }
        }
        return;
    }
    if b.initial == H {
        // aspiration: 축하 → 추카, 굳히다 → 구치다, 읽히다 → 일키다
        let (keep, moved) = split_final(a.fin);
        let aspirated = match moved {
            Some(G | 1 | K) => Some(K),
            Some(B | P) => Some(P),
            Some(J | CH) => Some(CH),
            Some(D | S | 10 | T) => Some(if b.vowel == I { CH } else { T }),
            _ => None,
        };
        if let Some(x) = aspirated {
            a.fin = keep;
            b.initial = x;
        }
        return;
    }
    if matches!(a.fin, F_H | F_NH | F_LH) && [G, D, J].contains(&b.initial) {
        // 좋다 → 조타, 않고 → 안코
        a.fin = if a.fin == F_H {
            0
        } else {
            split_final(a.fin).0
        };
        b.initial = aspirate(b.initial);
        return;
    }
    let mut fin = match a.fin {
        F_NH => F_N,
        F_LH => F_L,
        f => neutralize(f),
    };
    match (fin, b.initial) {
        (F_N, R) => fin = F_L,                 // 신라 → 실라
        (F_L, N) => b.initial = R,             // 설날 → 설랄
        (F_M | F_NG, R) => b.initial = N,      // 종로 → 종노
        (F_K | F_T | F_P, R) => b.initial = N, // 독립 → 독닙 → 동닙
        _ => {}
    }
    if [N, M].contains(&b.initial) {
        // nasalization: 국물 → 궁물, 입니다 → 임니다
        fin = match fin {
            F_K => F_NG,
            F_T => F_N,
            F_P => F_M,
            f => f,
        };
    }
    a.fin = fin;
}

fn final_roman(fin: u8) -> &'static str {
    match neutralize(fin) {
        F_K => "k",
        F_N => "n",
        F_T => "t",
        F_L => "l",
        F_M => "m",
        F_P => "p",
        F_NG => "ng",
        _ => "",
    }
}

fn push_word(out: &mut String, word: &[Syllable], sound_changes: bool) {
    let mut word = word.to_vec();
    if sound_changes {
        for i in 1..word.len() {
            let (a, b) = word.split_at_mut(i);
            sound_change(&mut a[i - 1], &mut b[0]);
        }
    }
    let mut prev_fin = 0;
    for s in &word {
        // ㄹㄹ is written ll: 실라 → silla
        if s.initial == R && neutralize(prev_fin) == F_L {
            out.push('l');
        } else {
            out.push_str(INITIALS[s.initial as usize]);
        }
        out.push_str(VOWELS[s.vowel as usize]);
        out.push_str(final_roman(s.fin));
        prev_fin = s.fin;
    }
}

/// Romanize the Hangul in `text`. `sound_changes`: as pronounced, the way the Revised
/// Romanization writes it (국물 → gungmul); otherwise letter by letter (국물 → gukmul).
/// Other characters are kept, lowercased.
pub fn romanize(text: &str, sound_changes: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = vec![];
    for c in text.chars() {
        if let Some(s) = decompose(c) {
            word.push(s);
            continue;
        }
        push_word(&mut out, &word, sound_changes);
        word.clear();
        out.extend(c.to_lowercase());
    }
    push_word(&mut out, &word, sound_changes);
    out
}

/// Romanization with the spelling differences people don't keep apart merged, and anything
/// but letters dropped: `eo`→`o`, `eu`→`u`, `ae`→`e`, `k`→`g`, `t`→`d`, `p`→`b`, `r`→`l`.
pub fn loosen(roman: &str) -> String {
    let b: Vec<u8> = roman
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let mut out = String::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let pair = (b[i], b.get(i + 1).copied().unwrap_or(0));
        let (c, len) = match pair {
            (b'e', b'o') => ('o', 2),
            (b'e', b'u') => ('u', 2),
            (b'a', b'e') => ('e', 2),
            (b'k', _) => ('g', 1),
            (b't', _) => ('d', 1),
            (b'p', _) => ('b', 1),
            (b'r', _) => ('l', 1),
            (c, _) => (c as char, 1),
        };
        out.push(c);
        i += len;
    }
    out
}

/// The keys a term is found by in `search.db`'s `term_roman`: its romanization as
/// pronounced and letter by letter, [`loosen`]ed. None if it has no Hangul.
pub fn keys(term: &str) -> Vec<String> {
    if !term.chars().any(|c| decompose(c).is_some()) {
        return vec![];
    }
    let pronounced = loosen(&romanize(term, true));
    let spelled = loosen(&romanize(term, false));
    if pronounced == spelled {
        vec![pronounced]
    } else {
        vec![pronounced, spelled]
    }
}

/// Typed text is romanization: Latin letters, with optional `-`, `'`, `.` or spaces.
pub fn is_romanization(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_alphabetic())
        && text
            .chars()
            .all(|c| c.is_ascii_alphabetic() || matches!(c, '-' | '\'' | '’' | '.' | ' '))
}

/// A romanized query, ready to match against Hangul terms.
#[derive(Debug, Clone)]
pub struct RomanQuery {
    /// [`loosen`]ed query. For a prefix search, also with its last vowel completed, since
    /// it may be cut off in the middle of `eo`, `eu` or `ae` (`se` for 세우다, `seuda`).
    patterns: Vec<String>,
    pub prefix: bool,
}

impl RomanQuery {
    pub fn new(typed: &str, prefix: bool) -> Option<Self> {
        if !is_romanization(typed) {
            return None;
        }
        let loose = loosen(typed);
        let mut patterns = vec![loose.clone()];
        if prefix {
            let raw = typed.trim_end_matches(|c: char| !c.is_ascii_alphabetic());
            let stem = &loose[..loose.len() - 1];
            match raw.chars().last().map(|c| c.to_ascii_lowercase()) {
                Some('e') => patterns.extend([format!("{stem}o"), format!("{stem}u")]),
                Some('a') => patterns.push(format!("{stem}e")),
                _ => {}
            }
        }
        Some(Self { patterns, prefix })
    }

    /// For [`RomanQuery::decode`]: the patterns and mode in one string, as an SQL parameter.
    /// [`loosen`]ed patterns: a key equal to one (or, for a prefix search, starting with one)
    /// matches. See [`keys`].
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    pub fn encode(&self) -> String {
        format!(
            "{}:{}",
            if self.prefix { "*" } else { "=" },
            self.patterns.join("|")
        )
    }

    pub fn decode(s: &str) -> Self {
        let (mode, patterns) = s.split_once(':').unwrap_or(("=", s));
        Self {
            patterns: patterns.split('|').map(String::from).collect(),
            prefix: mode == "*",
        }
    }

    /// 2: `term` romanizes to the query; 1: to something starting with it (prefix search);
    /// 0: no match.
    pub fn rank(&self, term: &str) -> u8 {
        let mut best = 0;
        for key in keys(term) {
            for p in &self.patterns {
                if key == *p {
                    return 2;
                }
                if self.prefix && key.starts_with(p.as_str()) {
                    best = 1;
                }
            }
        }
        best
    }

    /// Ranges of first characters `[start, end)` that a matching term can start with: the
    /// Hangul blocks of each initial consonant the query can start with.
    pub fn first_char_ranges(&self) -> Vec<(char, char)> {
        let mut ranges: Vec<(u32, u32)> = vec![];
        for (i, initial) in INITIALS.iter().enumerate() {
            let key = loosen(initial);
            let ok = self.patterns.iter().any(|p| {
                if key.is_empty() {
                    p.starts_with(['a', 'e', 'i', 'o', 'u', 'w', 'y'])
                } else {
                    p.starts_with(key.as_str()) || key.starts_with(p.as_str())
                }
            });
            if !ok {
                continue;
            }
            let start = SYLLABLE_FIRST + i as u32 * PER_INITIAL;
            let end = start + PER_INITIAL;
            match ranges.last_mut() {
                Some(last) if last.1 == start => last.1 = end,
                _ => ranges.push((start, end)),
            }
        }
        ranges
            .into_iter()
            .filter_map(|(a, b)| Some((char::from_u32(a)?, char::from_u32(b)?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn romanizes_with_sound_changes() {
        for (hangul, roman) in [
            ("한국어", "hangugeo"),
            ("서울", "seoul"),
            ("국물", "gungmul"),
            ("신라", "silla"),
            ("설날", "seollal"),
            ("같이", "gachi"),
            ("굳이", "guji"),
            ("좋다", "jota"),
            ("좋아", "joa"),
            ("축하", "chuka"),
            ("독립", "dongnip"),
            ("백리", "baengni"),
            ("종로", "jongno"),
            ("입니다", "imnida"),
            ("앉아", "anja"),
            ("많이", "mani"),
            ("닭", "dak"),
            ("읽다", "ikda"),
            ("전화", "jeonhwa"),
            ("라면", "ramyeon"),
            ("한국 사람", "hanguk saram"),
        ] {
            assert_eq!(romanize(hangul, true), roman, "{hangul}");
        }
        assert_eq!(romanize("국물", false), "gukmul");
        assert_eq!(romanize("한국어", false), "hangukeo");
    }

    #[test]
    fn loose_spellings_match() {
        let q = |s: &str, prefix| RomanQuery::new(s, prefix).unwrap();
        for typed in ["hangugeo", "hangukeo", "hangug-eo", "Hangugo", "han guk eo"] {
            assert_eq!(q(typed, false).rank("한국어"), 2, "{typed}");
        }
        assert_eq!(q("gukmul", false).rank("국물"), 2);
        assert_eq!(q("gungmul", false).rank("국물"), 2);
        assert_eq!(q("silla", false).rank("신라"), 2);
        assert_eq!(q("kachi", false).rank("같이"), 2);
        assert_eq!(q("seul", false).rank("서울"), 0);
        assert_eq!(q("soul", false).rank("서울"), 2);
        // prefix search, cut off in the middle of a vowel
        assert_eq!(q("hangug", true).rank("한국어"), 1);
        assert_eq!(q("hanguge", true).rank("한국어"), 2);
        assert_eq!(q("se", true).rank("세우다"), 1);
        assert_eq!(q("ha", true).rank("해"), 2);
        assert_eq!(q("ha", true).rank("하늘"), 1);
        assert_eq!(q("hangug", false).rank("한국어"), 0);
    }

    #[test]
    fn narrows_by_initial_consonant() {
        let ranges = RomanQuery::new("hangugeo", true)
            .unwrap()
            .first_char_ranges();
        assert_eq!(ranges, vec![('하', char::from_u32(0xD7A4).unwrap())]);
        // g: ㄱ, ㄲ (adjacent) and ㅋ
        let ranges = RomanQuery::new("g", true).unwrap().first_char_ranges();
        assert_eq!(ranges.len(), 2);
        assert!(
            ranges
                .iter()
                .any(|(a, b)| ('가'..*b).contains(a) && *b > '까')
        );
        assert!(ranges.iter().any(|(a, _)| *a == '카'));
        // a vowel: ㅇ
        let ranges = RomanQuery::new("eo", true).unwrap().first_char_ranges();
        assert_eq!(ranges, vec![('아', '자')]);
        assert!(RomanQuery::new("한국어", true).is_none());
    }
}
