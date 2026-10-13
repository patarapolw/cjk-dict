/// tone-marked vowel -> (plain vowel, tone). Expects NFC (precomposed) input.
fn mark(c: char) -> Option<(char, u8)> {
    Some(match c {
        'ā' => ('a', 1),
        'á' => ('a', 2),
        'ǎ' => ('a', 3),
        'à' => ('a', 4),
        'ē' => ('e', 1),
        'é' => ('e', 2),
        'ě' => ('e', 3),
        'è' => ('e', 4),
        'ī' => ('i', 1),
        'í' => ('i', 2),
        'ǐ' => ('i', 3),
        'ì' => ('i', 4),
        'ō' => ('o', 1),
        'ó' => ('o', 2),
        'ǒ' => ('o', 3),
        'ò' => ('o', 4),
        'ū' => ('u', 1),
        'ú' => ('u', 2),
        'ǔ' => ('u', 3),
        'ù' => ('u', 4),
        'ǖ' => ('ü', 1),
        'ǘ' => ('ü', 2),
        'ǚ' => ('ü', 3),
        'ǜ' => ('ü', 4),
        'ɡ' => ('g', 0), // single-story g, used in some pinyin text
        _ => return None,
    })
}

const INITIALS: [&str; 24] = [
    "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "r",
    "z", "c", "s", "y", "w", "",
];
const FINALS: [&str; 38] = [
    "a", "o", "e", "i", "u", "ü", "ai", "ei", "ui", "ao", "ou", "iu", "ie", "üe", "ue", "er", "an",
    "en", "in", "un", "ün", "ang", "eng", "ing", "ong", "ia", "iao", "ian", "iang", "iong", "ua",
    "uo", "uai", "uan", "uang", "ueng", "üan", "ê",
];

fn is_syllable(s: &str) -> bool {
    INITIALS
        .iter()
        .any(|i| s.strip_prefix(i).is_some_and(|rest| FINALS.contains(&rest)))
}

type Letters = (char, u8);

fn segment(run: &[Letters]) -> Option<Vec<&[Letters]>> {
    if run.is_empty() {
        return Some(vec![]);
    }
    for len in (1..=run.len().min(6)).rev() {
        let (head, tail) = run.split_at(len);
        let s: String = head.iter().map(|&(c, _)| c).collect();
        let marks = head.iter().filter(|&&(_, t)| t > 0).count();
        if marks <= 1 && is_syllable(&s) {
            if let Some(mut rest) = segment(tail) {
                rest.insert(0, head);
                return Some(rest);
            }
        }
    }
    None
}

fn render(syl: &[Letters], explicit: Option<u8>) -> String {
    let letters: String = syl.iter().map(|&(c, _)| c).collect();
    let letters = letters.replace('ü', "v");
    let tone = explicit
        .or_else(|| syl.iter().map(|&(_, t)| t).find(|&t| t > 0))
        .unwrap_or(5);
    format!("{letters}{tone}")
}

fn parse_pinyin(input: &str) -> Option<Vec<(Vec<Letters>, Option<u8>)>> {
    let lower = input.to_lowercase().replace("u:", "ü").replace('v', "ü");
    // a digit always ends a syllable: add a space after every digit ("pin1yin1" -> "pin1 yin1 ")
    let mut spaced = String::with_capacity(lower.len() + 8);
    for c in lower.chars() {
        spaced.push(c);
        if c.is_ascii_digit() {
            spaced.push(' ');
        }
    }
    let is_sep = |c: char| c.is_whitespace() || matches!(c, '\'' | '’' | '-' | '·');

    let mut out = Vec::new();
    for word in spaced.split(is_sep).filter(|w| !w.is_empty()) {
        // after the pre-pass, a digit can only be the last character of a word
        let (letters, digit) = match word.char_indices().last() {
            Some((i, d)) if d.is_ascii_digit() => (&word[..i], Some(d)),
            _ => (word, None),
        };
        let run: Vec<Letters> = letters.chars().map(|c| mark(c).unwrap_or((c, 0))).collect();
        if run.is_empty() {
            return None;
        }
        match digit {
            Some(d) => {
                let tone = match d {
                    '0' => 5,
                    '1'..='5' => d as u8 - b'0',
                    _ => return None,
                };
                if !run.iter().all(|&(l, _)| l.is_alphabetic()) {
                    return None;
                }
                out.push((run, Some(tone)));
            }
            None => {
                for syl in segment(&run)? {
                    out.push((syl.to_vec(), None));
                }
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Normalize pinyin: lowercase, tone digit after each syllable, `0` -> 5, and `ü` -> `v`.
/// Accepts tone marks, digits, `u:`, `ü` and `v`. Returns None if it can't be read as pinyin.
pub fn normalize_pinyin(input: &str) -> Option<String> {
    let syllables = parse_pinyin(input)?;
    Some(
        syllables
            .iter()
            .map(|(syl, tone)| render(syl, *tone))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Expand toneless syllables into one result for each possible tone combination.
/// Returns an empty vector when the input cannot be read as pinyin.
pub fn expand_pinyin(input: &str) -> Vec<String> {
    let Some(syllables) = parse_pinyin(input) else {
        return Vec::new();
    };

    let mut expanded = vec![String::new()];
    for (syl, explicit) in syllables {
        let tone = explicit.or_else(|| syl.iter().map(|&(_, t)| t).find(|&t| t > 0));
        let tones: Vec<u8> = match tone {
            Some(tone) => vec![tone],
            None => (1..=5).collect(),
        };
        let alternatives: Vec<String> =
            tones.iter().map(|&tone| render(&syl, Some(tone))).collect();
        let mut next = Vec::with_capacity(expanded.len() * alternatives.len());
        for prefix in expanded {
            for alternative in &alternatives {
                if prefix.is_empty() {
                    next.push(alternative.clone());
                } else {
                    next.push(format!("{prefix} {alternative}"));
                }
            }
        }
        expanded = next;
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(input: &str, expected: &[&str]) {
        assert_eq!(
            expand_pinyin(input),
            expected
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>(),
            "input: {input:?}"
        );
    }

    #[test]
    fn tone_marks_and_boundaries() {
        check("Chángān", &["chang2 an1"]);
        check("Cháng'ān", &["chang2 an1"]);
        check("Xīān", &["xi1 an1"]);
        check("Tiān'ānmén", &["tian1 an1 men2"]);
        check("Zhōngguó", &["zhong1 guo2"]);
        check(
            "māma",
            &["ma1 ma1", "ma1 ma2", "ma1 ma3", "ma1 ma4", "ma1 ma5"],
        );
    }

    #[test]
    fn toneless_and_explicit_digits() {
        let all_combinations = expand_pinyin("pin yin");
        assert_eq!(all_combinations.len(), 25);
        assert_eq!(
            all_combinations.first().map(String::as_str),
            Some("pin1 yin1")
        );
        assert_eq!(
            all_combinations.last().map(String::as_str),
            Some("pin5 yin5")
        );
        check("pin0 yin5", &["pin5 yin5"]);
        check(
            "pin0 yin",
            &[
                "pin5 yin1",
                "pin5 yin2",
                "pin5 yin3",
                "pin5 yin4",
                "pin5 yin5",
            ],
        );
        check(
            "pin1 yin",
            &[
                "pin1 yin1",
                "pin1 yin2",
                "pin1 yin3",
                "pin1 yin4",
                "pin1 yin5",
            ],
        );
        check("pin1yin1", &["pin1 yin1"]);
        check("Bei3 jing1", &["bei3 jing1"]);
        check("yi1 hui4 r5", &["yi1 hui4 r5"]);
        assert_eq!(expand_pinyin("pinyin"), all_combinations);
    }

    #[test]
    fn umlaut_spellings_are_accepted_and_normalized() {
        for input in ["nǚ", "nü3", "nu:3", "nv3", "NÜ3"] {
            check(input, &["nv3"]);
        }
        check("nü", &["nv1", "nv2", "nv3", "nv4", "nv5"]);
        check("lüè", &["lve4"]);
        check("lǜe", &["lve4"]);
    }

    #[test]
    fn rejects_non_pinyin_including_query_operators() {
        for input in ["hello", "", "pin6", "1", "12", "AND", "OR"] {
            check(input, &[]);
        }
    }

    #[test]
    fn expanded_results_are_stable_when_expanded_again() {
        for input in [
            "Chángān",
            "nǚ",
            "Xī'ān",
            "Xī ān",
            "lv4",
            "pin0 yin5",
            "pin1 yin5",
        ] {
            for once in expand_pinyin(input) {
                assert_eq!(
                    expand_pinyin(&once),
                    [once.clone()],
                    "input: {input:?}, expanded: {once:?}"
                );
            }
        }
    }

    #[test]
    fn expands_pinyin_tone_combinations() {
        assert_eq!(expand_pinyin("pin yin").len(), 25);
    }
}
