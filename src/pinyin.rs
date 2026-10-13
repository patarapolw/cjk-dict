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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Toneless {
    /// toneless syllable = neutral tone 5 (dictionary data)
    Neutral,
    /// user queries, expanded in a bracket connected with OR (user queries)
    /// e.g. (yin1 OR yin2 OR yin3 OR yin4 OR yin5)
    Expand,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Umlaut {
    /// `nu:3`, CC-CEDICT style
    Colon,
    /// `nv3`, one plain-letter FTS token
    V,
}

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub toneless: Toneless,
    pub umlaut: Umlaut,
}

impl Opts {
    pub const DICT: Opts = Opts {
        toneless: Toneless::Neutral,
        umlaut: Umlaut::V,
    };
    pub const QUERY: Opts = Opts {
        toneless: Toneless::Expand,
        umlaut: Umlaut::V,
    };
}

fn render(syl: &[Letters], explicit: Option<u8>, o: Opts) -> String {
    let letters: String = syl.iter().map(|&(c, _)| c).collect();
    let letters = match o.umlaut {
        Umlaut::Colon => letters.replace('ü', "u:"),
        Umlaut::V => letters.replace('ü', "v"),
    };
    let tone = explicit.or_else(|| syl.iter().map(|&(_, t)| t).find(|&t| t > 0));
    match tone {
        Some(t) => format!("{letters}{t}"),
        None if o.toneless == Toneless::Expand => {
            format!("({s}1 OR {s}2 OR {s}3 OR {s}4 OR {s}5)", s = letters)
        }
        None => format!("{letters}5"),
    }
}

/// Normalize pinyin: lowercase, tone digit after each syllable, `0` -> 5, ü per `opts.umlaut`.
/// Accepts tone marks, digits, `u:`, `ü` and `v`. Returns None if it can't be read as pinyin.
pub fn normalize_pinyin(input: &str, opts: Opts) -> Option<String> {
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

    let mut out: Vec<String> = Vec::new();
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
                out.push(render(&run, Some(tone), opts)); // letters taken as written
            }
            None => {
                for syl in segment(&run)? {
                    out.push(render(syl, None, opts));
                }
            }
        }
    }
    (!out.is_empty()).then(|| out.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLON: Opts = Opts {
        toneless: Toneless::Neutral,
        umlaut: Umlaut::Colon,
    };

    fn check(opts: Opts, cases: &[(&str, Option<&str>)]) {
        for &(input, expected) in cases {
            assert_eq!(
                normalize_pinyin(input, opts).as_deref(),
                expected,
                "input: {input:?}"
            );
        }
    }

    #[test]
    fn dict_tone_marks_and_boundaries() {
        check(
            Opts::DICT,
            &[
                ("Chángān", Some("chang2 an1")),
                ("Cháng'ān", Some("chang2 an1")),
                ("Xīān", Some("xi1 an1")),
                ("Tiān'ānmén", Some("tian1 an1 men2")),
                ("Zhōngguó", Some("zhong1 guo2")),
                ("māma", Some("ma1 ma5")),
            ],
        );
    }

    #[test]
    fn dict_toneless_is_neutral() {
        check(
            Opts::DICT,
            &[
                ("pin yin", Some("pin5 yin5")),
                ("pin0 yin5", Some("pin5 yin5")),
                ("pinyin", Some("pin5 yin5")),
            ],
        );
    }

    #[test]
    fn query_toneless_stays_bare_meaning_any_tone() {
        check(
            Opts::QUERY,
            &[
                (
                    "pin yin",
                    Some(
                        "(pin1 OR pin2 OR pin3 OR pin4 OR pin5) (yin1 OR yin2 OR yin3 OR yin4 OR yin5)",
                    ),
                ),
                (
                    "pin1 yin",
                    Some("pin1 (yin1 OR yin2 OR yin3 OR yin4 OR yin5)"),
                ),
                (
                    "pin0 yin",
                    Some("pin5 (yin1 OR yin2 OR yin3 OR yin4 OR yin5)"),
                ), // explicit 0 is a real neutral tone
                (
                    "pinyin",
                    Some(
                        "(pin1 OR pin2 OR pin3 OR pin4 OR pin5) (yin1 OR yin2 OR yin3 OR yin4 OR yin5)",
                    ),
                ),
                ("nü", Some("(nv1 OR nv2 OR nv3 OR nv4 OR nv5)")),
            ],
        );
    }

    #[test]
    fn query_with_marks_behaves_like_dict() {
        check(
            Opts::QUERY,
            &[
                ("māma", Some("ma1 (ma1 OR ma2 OR ma3 OR ma4 OR ma5)")),
                (
                    "xǐhuan",
                    Some("xi3 (huan1 OR huan2 OR huan3 OR huan4 OR huan5)"),
                ),
                ("Chángān", Some("chang2 an1")),
            ],
        );
    }

    #[test]
    fn umlaut_spellings_all_accepted_output_follows_option() {
        for input in ["nǚ", "nü3", "nu:3", "nv3", "NÜ3"] {
            check(Opts::DICT, &[(input, Some("nv3"))]);
            check(COLON, &[(input, Some("nu:3"))]);
        }
        check(Opts::DICT, &[("lüè", Some("lve4")), ("lǜe", Some("lve4"))]); // mark may sit on ü or e
    }

    #[test]
    fn digits_always_end_a_syllable() {
        check(
            Opts::DICT,
            &[
                ("pin1yin1", Some("pin1 yin1")),
                ("Bei3 jing1", Some("bei3 jing1")),
                ("yi1 hui4 r5", Some("yi1 hui4 r5")),
            ],
        );
    }

    #[test]
    fn rejects_what_is_not_pinyin() {
        for opts in [Opts::DICT, Opts::QUERY] {
            check(
                opts,
                &[
                    ("hello", None),
                    ("", None),
                    ("pin6", None),
                    ("1", None),
                    ("12", None),
                    ("AND", None),
                    ("OR", None),
                ],
            );
        }
    }

    #[test]
    fn idempotent_in_both_modes() {
        for opts in [Opts::DICT, Opts::QUERY] {
            for s in [
                "Chángān",
                "nǚ",
                "Xī'ān",
                "Xī ān",
                "lv4",
                "pin0 yin5",
                "pin1 yin5",
            ] {
                let once = normalize_pinyin(s, opts).unwrap();
                assert_eq!(
                    normalize_pinyin(&once, opts).as_deref(),
                    Some(once.as_str()),
                    "input: {s:?} ({opts:?})"
                );
            }
        }
    }
}
