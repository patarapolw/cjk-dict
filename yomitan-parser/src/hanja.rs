//! Hangul readings for Korean headwords written in Hanja: 光速 → 광속, 光나다 → 광나다.
//!
//! Korean dictionaries give no readings, so `search.db` fills them in
//! ([`crate::search`]'s build). Readings come from [`hanja_hangul`]: a reading per character,
//! the initial-sound rule (두음법칙: 李 → 이, 女子 → 여자), 렬/률 → 열/율 after a vowel or ㄴ,
//! 不 → 부 before ㄷ/ㅈ, and a table of dictionary words that read otherwise (樂園 → 낙원).

use hanja_hangul::{Converter, is_hanja};

/// Readings to try for a word with characters of several readings (樂: 락, 낙, 악, 요)
const MAX_CANDIDATES: usize = 8;

/// Hangul readings of `term`, the most likely first; other candidates differ in characters
/// with several readings, so that the build can keep the one that is a Hangul headword too.
/// Characters other than Hanja (Hangul, digits, ...) are kept as they are. Empty if `term`
/// has no Hanja, or Hanja without a known reading.
pub fn hangul_readings(term: &str) -> Vec<String> {
    let chars: Vec<char> = term.chars().collect();
    if !chars.iter().copied().any(is_hanja) {
        return vec![];
    }
    let conv = Converter::global();
    // each piece is a run of Hanja (with its readings) or of anything else (itself)
    let mut pieces: Vec<Vec<String>> = vec![];
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        let hanja = is_hanja(chars[i]);
        while i < chars.len() && is_hanja(chars[i]) == hanja {
            i += 1;
        }
        let run: String = chars[start..i].iter().collect();
        if hanja {
            // the initial-sound rule applies at the start of a word, not after Hangul or
            // digits: 色깔論 → 색깔론
            let word_start = start == 0 || !chars[start - 1].is_alphanumeric();
            let readings = conv.readings(&run, word_start, MAX_CANDIDATES);
            if readings.is_empty() {
                return vec![];
            }
            pieces.push(readings);
        } else {
            pieces.push(vec![run]);
        }
    }

    // combinations, the first readings of every piece first
    let mut out: Vec<String> = vec![];
    let mut idx = vec![0usize; pieces.len()];
    loop {
        let s: String = pieces
            .iter()
            .zip(&idx)
            .map(|(p, &k)| p[k].as_str())
            .collect();
        if !out.contains(&s) {
            out.push(s);
            if out.len() >= MAX_CANDIDATES {
                break;
            }
        }
        // next combination: carry from the last piece
        let mut k = pieces.len();
        loop {
            if k == 0 {
                return out;
            }
            k -= 1;
            idx[k] += 1;
            if idx[k] < pieces[k].len() {
                break;
            }
            idx[k] = 0;
        }
    }
    out
}

/// libhangul's readings of each Hanja with several (`scripts/hanja-eum.mjs`)
const EUM: &str = include_str!("../data/hanja_eum.txt");

/// A Hanja's Hangul readings (eum), as a character dictionary lists them: hanja_hangul's
/// main reading first, then libhangul's others (樂: 악 락 요), each without the
/// initial-sound rule (李: 리). Not hanja_hangul's other readings, which are how a
/// character reads in some words (道 in 使道 사또), not its eum. Empty for a character that
/// isn't a Hanja, or has no known reading.
pub fn char_readings(c: char) -> Vec<String> {
    let main = Converter::global().char_reading(c);
    let others = EUM
        .lines()
        .find_map(|l| {
            let (h, rs) = l.split_once('\t')?;
            (h.chars().eq([c])).then_some(rs)
        })
        .unwrap_or("");

    // readings that differ only by the initial-sound rule (樂: 낙 and 락) are one: the main
    // reading, else the one without the rule
    let mut out: Vec<char> = main.into_iter().collect();
    for r in others.split(' ').filter_map(|r| r.chars().next()) {
        match out.iter().position(|&o| dueum(o) == dueum(r)) {
            None => out.push(r),
            Some(i) if Some(out[i]) != main && out[i] == dueum(out[i]) && r != dueum(r) => {
                out[i] = r
            }
            Some(_) => {}
        }
    }
    out.into_iter().map(String::from).collect()
}

/// A syllable at the start of a word, by the initial-sound rule (두음법칙): 락 → 낙,
/// 리 → 이, 녀 → 여
fn dueum(s: char) -> char {
    const L: u32 = 5; // ㄹ
    const N: u32 = 2; // ㄴ
    const ZERO: u32 = 11; // ㅇ
    let Some(o) = (s as u32).checked_sub(0xAC00).filter(|&o| o < 11172) else {
        return s;
    };
    let (initial, medial) = (o / 588, o % 588 / 28);
    // ㅑ ㅕ ㅖ ㅛ ㅠ ㅣ
    let y = matches!(medial, 2 | 6 | 7 | 12 | 17 | 20);
    let initial = match initial {
        L if y => ZERO,
        L => N,
        N if y => ZERO,
        i => i,
    };
    char::from_u32(0xAC00 + initial * 588 + o % 588).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eum(c: char) -> Vec<String> {
        char_readings(c)
    }

    #[test]
    fn character_readings_are_eum() {
        // not 또 (使道 사또), nor 니, 리 (word-medial forms)
        assert_eq!(eum('道'), ["도"]);
        assert_eq!(eum('二'), ["이"]);
        assert_eq!(eum('具'), ["구"]);
        // several readings, the main one first, without the initial-sound rule
        assert_eq!(eum('樂'), ["악", "락", "요"]);
        assert_eq!(eum('率'), ["률", "솔"]);
        assert_eq!(eum('李'), ["리"]);
        assert_eq!(eum('女'), ["녀"]);
        assert_eq!(eum('金'), ["금", "김"]);
        assert_eq!(eum('茶'), ["차", "다"]);
        // the main reading wins over a rarer one it shares a form with: 異 is 이, not 리
        assert_eq!(eum('異'), ["이"]);
        assert!(eum('a').is_empty());
        assert!(eum('한').is_empty());
    }

    #[test]
    fn initial_sound_rule() {
        assert_eq!(dueum('락'), '낙');
        assert_eq!(dueum('리'), '이');
        assert_eq!(dueum('률'), '율');
        assert_eq!(dueum('녀'), '여');
        assert_eq!(dueum('노'), '노');
        assert_eq!(dueum('도'), '도');
        assert_eq!(dueum('a'), 'a');
    }

    fn first(term: &str) -> Option<String> {
        hangul_readings(term).into_iter().next()
    }

    #[test]
    fn reads_hanja_as_hangul() {
        for (hanja, hangul) in [
            ("光速", "광속"),
            ("克己", "극기"),
            ("經濟", "경제"),
            // initial-sound rule, at the start of a word only
            ("李", "이"),
            ("女子", "여자"),
            ("勞動", "노동"),
            ("男女", "남녀"),
            // 렬/률 after a vowel or ㄴ
            ("分裂", "분열"),
            ("比率", "비율"),
            // dictionary words
            ("樂園", "낙원"),
            ("音樂", "음악"),
        ] {
            assert_eq!(first(hanja).as_deref(), Some(hangul), "{hanja}");
        }
        // compatibility Hanja read as their unified forms: U+F900 豈 = U+8C48 豈
        assert_eq!(first("\u{F900}"), first("\u{8C48}"));
        assert!(first("\u{8C48}").is_some());
    }

    #[test]
    fn keeps_other_characters() {
        assert_eq!(first("光나다").as_deref(), Some("광나다"));
        assert_eq!(first("色깔論").as_deref(), Some("색깔론"));
        assert_eq!(first("1995年度").as_deref(), Some("1995년도"));
        assert_eq!(first("新 女性").as_deref(), Some("신 여성"));
    }

    #[test]
    fn no_hanja_no_reading() {
        assert!(hangul_readings("한국어").is_empty());
        assert!(hangul_readings("abc").is_empty());
        assert!(hangul_readings("").is_empty());
    }

    #[test]
    fn several_readings_are_candidates() {
        let r = hangul_readings("樂");
        assert!(r.len() > 1, "{r:?}");
        assert!(r.contains(&"악".to_string()), "{r:?}");
        assert!(r.contains(&"요".to_string()), "{r:?}");
        // a mixed word: the Hangul part stays in every candidate
        for r in hangul_readings("樂하다") {
            assert!(r.ends_with("하다"), "{r}");
        }
    }
}
