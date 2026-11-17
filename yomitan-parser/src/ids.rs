//! Character decomposition from IDS (Ideographic Description Sequences), as in the
//! [CJKVI IDS database](https://github.com/cjkvi/cjkvi-ids)'s `ids.txt`:
//!
//! ```text
//! U+4E0E	与	⿹②一[GTKV]	⿻②一[J]
//! U+6797	林	⿰木木
//! ```
//!
//! A line is a code point, the character, and one or more IDS, each optionally followed by
//! the regions it applies to in brackets (G: China, T: Taiwan, J: Japan, K: Korea,
//! V: Vietnam, H: Hong Kong, ...). An IDS is IDCs (`⿰` left-right, `⿱` top-bottom, ...)
//! followed by their components; a character that does not break down is its own IDS.
//! Circled numbers (`①`-`⑳`) stand for unencoded components of that many strokes, and
//! `&CDP-8C4E;`-style entity references for components only some fonts have.

use serde::Serialize;

/// One way to write a character, e.g. `⿰木木` for 林.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ids {
    pub ids: String,
    /// region letters, e.g. `GTKV`; empty for all of them
    pub regions: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdsLine {
    pub ch: String,
    pub ids: Vec<Ids>,
}

/// An Ideographic Description Character, with the number of components it takes.
pub fn idc_arity(c: char) -> Option<usize> {
    match c {
        // ⿲ left-middle-right, ⿳ top-middle-bottom
        '\u{2FF2}' | '\u{2FF3}' => Some(3),
        // ⿰⿱⿴⿵⿶⿷⿸⿹⿺⿻, and Unicode 15.1's ⿼⿽⿾⿿ (⿾ ⿿ take one)
        '\u{2FFE}' | '\u{2FFF}' => Some(1),
        '\u{2FF0}'..='\u{2FFD}' => Some(2),
        // ㇯ (subtraction, Unicode 15.1)
        '\u{31EF}' => Some(2),
        _ => None,
    }
}

/// Parse one line of `ids.txt`; `None` for comments, blank and malformed lines.
pub fn parse_line(line: &str) -> Option<IdsLine> {
    if line.starts_with('#') || line.starts_with(';') {
        return None;
    }
    let mut cols = line.trim_end_matches(['\r', '\n']).split('\t');
    let _code = cols.next()?.strip_prefix("U+")?;
    let ch = cols.next()?.to_string();
    if ch.is_empty() {
        return None;
    }
    let ids: Vec<Ids> = cols.filter_map(parse_column).collect();
    if ids.is_empty() {
        return None;
    }
    Some(IdsLine { ch, ids })
}

/// IDS columns as `search.db` keeps them: tab-separated, as in `ids.txt`.
pub fn parse_columns(cols: &str) -> Vec<Ids> {
    cols.split('\t').filter_map(parse_column).collect()
}

/// `⿹②一[GTKV]`
fn parse_column(col: &str) -> Option<Ids> {
    let col = col.trim();
    let (seq, regions) = match col.strip_suffix(']').and_then(|s| s.rsplit_once('[')) {
        Some((seq, regions)) => (seq, regions),
        None => (col, ""),
    };
    (!seq.is_empty()).then(|| Ids {
        ids: seq.to_string(),
        regions: regions.to_string(),
    })
}

/// Radicals and the forms they take as a component, which IDS name instead: 河 is
/// `⿰氵可`, not `⿰水可`. The first of each is the character itself.
const COMPONENT_FORMS: &[&[&str]] = &[
    &["人", "亻", "𠆢"],
    &["刀", "刂", "⺈"],
    &["心", "忄", "⺗"],
    &["手", "扌", "龵"],
    &["水", "氵", "氺"],
    &["火", "灬"],
    &["犬", "犭"],
    &["示", "礻"],
    &["衣", "衤"],
    &["玉", "王", "⺩"],
    &["艸", "艹", "⺾", "⺿", "⻀"],
    &["竹", "⺮"],
    &["糸", "糹", "纟"],
    &["言", "訁", "讠"],
    &["金", "釒", "钅"],
    &["食", "飠", "饣", "𩙿"],
    &["足", "⻊"],
    &["肉", "⺼"],
    &["网", "罒", "⺲", "⺳"],
    &["老", "耂"],
    &["攴", "攵"],
    &["辵", "辶", "⻌", "⻍", "⻎"],
    &["邑", "阝"],
    &["阜", "阝"],
    &["爪", "爫"],
    &["牛", "牜"],
    &["羊", "⺶", "⺷"],
    &["歹", "歺"],
    &["疒"],
    &["門", "门"],
    &["車", "车"],
    &["貝", "贝"],
    &["見", "见"],
    &["馬", "马"],
    &["魚", "鱼"],
    &["鳥", "鸟"],
    &["頁", "页"],
    &["長", "镸", "长"],
    &["雨", "⻗"],
    &["小", "⺌", "⺍"],
    &["尢", "尣"],
    &["彐", "彑"],
    &["乙", "乚"],
    &["冫", "⺀"],
];

/// `c` and the forms it takes as a component ([`COMPONENT_FORMS`]); just `c` for most.
pub fn component_forms(c: &str) -> Vec<String> {
    let mut out = vec![c.to_string()];
    for forms in COMPONENT_FORMS {
        if forms.contains(&c) {
            for f in forms.iter() {
                if !out.iter().any(|o| o == f) {
                    out.push(f.to_string());
                }
            }
        }
    }
    out
}

/// A Kangxi radical (`⼝`, U+2F1D) as the ideograph it looks like (口), which IDS use.
pub fn normalize_component(c: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let mut cs = c.chars();
    match (cs.next(), cs.next()) {
        (Some(ch @ '\u{2F00}'..='\u{2FDF}'), None) => ch.nfkc().collect(),
        _ => c.to_string(),
    }
}

/// The components an IDS is made of, in order of appearance and without repeats: `⿰木木`
/// gives 木, `⿱⿰木木一` gives 木 and 一. Unencoded components (circled numbers, entity
/// references, `？`) are left out, and so is `ch` itself: a character that does not break
/// down has no components.
pub fn components(ids: &str, ch: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut chars = ids.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            // &CDP-8C4E;
            for c in chars.by_ref() {
                if c == ';' {
                    break;
                }
            }
            continue;
        }
        if idc_arity(c).is_some() || is_placeholder(c) {
            continue;
        }
        // a variation selector belongs to the character before it
        let mut s = c.to_string();
        while let Some(&v) = chars.peek() {
            if is_variation_selector(v) {
                s.push(v);
                chars.next();
            } else {
                break;
            }
        }
        if s != ch && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// The components of every IDS of a line, without repeats.
pub fn line_components(line: &IdsLine) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for ids in &line.ids {
        for c in components(&ids.ids, &line.ch) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// ①-⑳ (unencoded components of N strokes) and ？ (unknown)
fn is_placeholder(c: char) -> bool {
    matches!(c, '\u{2460}'..='\u{2473}' | '？' | '?')
}

fn is_variation_selector(c: char) -> bool {
    matches!(c, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

/// Whether `s` is a single CJK ideograph (or radical/component form), the characters
/// [`components`] and the character tables in `search.db` are about. A variation selector
/// may follow it.
pub fn is_han_char(s: &str) -> bool {
    let mut cs = s.chars();
    let Some(c) = cs.next() else {
        return false;
    };
    is_han(c) && cs.all(is_variation_selector)
}

/// CJK ideographs, radicals and strokes, including the extension blocks and compatibility
/// ideographs.
pub fn is_han(c: char) -> bool {
    matches!(c,
        '\u{2E80}'..='\u{2FDF}'   // CJK Radicals Supplement, Kangxi Radicals
        | '\u{3005}' | '\u{3007}' // 々 〇
        | '\u{31C0}'..='\u{31EF}' // CJK Strokes
        | '\u{3400}'..='\u{4DBF}' // Extension A
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}' // Compatibility Ideographs
        | '\u{20000}'..='\u{323AF}' // Extensions B-H, Compatibility Supplement
    )
}

/// The distinct Han characters of `text`, in order: what a character lookup shows.
pub fn han_chars(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let mut s = c.to_string();
        while let Some(&v) = chars.peek() {
            if is_variation_selector(v) {
                s.push(v);
                chars.next();
            } else {
                break;
            }
        }
        if is_han(c) && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_with_and_without_regions() {
        assert_eq!(
            parse_line("U+4E0E\t与\t⿹②一[GTKV]\t⿻②一[J]"),
            Some(IdsLine {
                ch: "与".into(),
                ids: vec![
                    Ids {
                        ids: "⿹②一".into(),
                        regions: "GTKV".into()
                    },
                    Ids {
                        ids: "⿻②一".into(),
                        regions: "J".into()
                    },
                ],
            })
        );
        assert_eq!(
            parse_line("U+6797\t林\t⿰木木\n").unwrap().ids,
            [Ids {
                ids: "⿰木木".into(),
                regions: "".into()
            }]
        );
        assert_eq!(parse_line("# Copyright"), None);
        assert_eq!(parse_line(""), None);
    }

    #[test]
    fn components_skip_idcs_placeholders_and_the_character_itself() {
        assert_eq!(components("⿰木木", "林"), ["木"]);
        assert_eq!(components("⿱⿰木木一", "x"), ["木", "一"]);
        assert_eq!(components("⿹②一", "与"), ["一"]);
        assert_eq!(components("⿰&CDP-8C4E;口", "x"), ["口"]);
        // does not break down
        assert!(components("一", "一").is_empty());
        assert_eq!(components("⿰亻\u{FE00}木", "x"), ["亻\u{FE00}", "木"]);

        let line = parse_line("U+4E1F\t丟\t⿱王厶\t⿱一去").unwrap();
        assert_eq!(line_components(&line), ["王", "厶", "一", "去"]);
    }

    #[test]
    fn columns_round_trip_and_radicals_have_their_forms() {
        assert_eq!(
            parse_columns("⿹②一[GTKV]\t⿻②一[J]"),
            parse_line("U+4E0E\t与\t⿹②一[GTKV]\t⿻②一[J]").unwrap().ids
        );
        assert_eq!(component_forms("水"), ["水", "氵", "氺"]);
        assert_eq!(component_forms("氵"), ["氵", "水", "氺"]);
        // a form of two radicals
        assert_eq!(component_forms("阝"), ["阝", "邑", "阜"]);
        assert_eq!(component_forms("木"), ["木"]);
        assert_eq!(normalize_component("\u{2F1D}"), "口");
        assert_eq!(normalize_component("口"), "口");
    }

    #[test]
    fn han_chars_keeps_ideographs_only() {
        assert_eq!(
            han_chars("日本語の勉強、日本"),
            ["日", "本", "語", "勉", "強"]
        );
        assert_eq!(han_chars("한국어 漢字 𠀋"), ["漢", "字", "𠀋"]);
        assert!(is_han_char("氵"));
        assert!(is_han_char("⺡"));
        assert!(!is_han_char("日本"));
        assert!(!is_han_char("か"));
    }
}
