//! Variant forms of Han characters, from [Unihan](https://www.unicode.org/reports/tr38/)'s
//! `Unihan_Variants.txt`: 学 → 學 (traditional, kyūjitai), 広 → 廣 (kyūjitai), 钟 → 鍾, 鐘.
//!
//! Lines are `U+5B66<TAB>kTraditionalVariant<TAB>U+5B78`; some values carry their sources
//! (`U+6588<kLau,kMatthews`), which are dropped.

/// How a variant relates to its character, in the order they are listed
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VariantKind {
    /// `kTraditionalVariant`: 学 → 學
    Traditional,
    /// `kSimplifiedVariant`: 學 → 学
    Simplified,
    /// `kJapaneseOldVariant`: 学 → 學, 広 → 廣
    Kyujitai,
    /// `kJapaneseNewVariant`: 學 → 学
    Shinjitai,
    /// `kSemanticVariant`: the same meaning, though not always interchangeable (學 → 斈)
    Semantic,
    /// `kSpecializedSemanticVariant`, the same meaning in some senses only, and `kZVariant`,
    /// the same character drawn differently
    Other,
}

impl VariantKind {
    /// As `search.db` keeps it
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Traditional => "traditional",
            Self::Simplified => "simplified",
            Self::Kyujitai => "kyujitai",
            Self::Shinjitai => "shinjitai",
            Self::Semantic => "semantic",
            Self::Other => "other",
        }
    }

    /// [`Self::as_str`] undone
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Traditional,
            Self::Simplified,
            Self::Kyujitai,
            Self::Shinjitai,
            Self::Semantic,
            Self::Other,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }

    /// The Unihan field's kind; `None` for fields that aren't variants of the kinds kept,
    /// e.g. `kSpoofingVariant` (characters that only look alike)
    pub fn of_field(field: &str) -> Option<Self> {
        Some(match field {
            "kTraditionalVariant" => Self::Traditional,
            "kSimplifiedVariant" => Self::Simplified,
            "kJapaneseOldVariant" => Self::Kyujitai,
            "kJapaneseNewVariant" => Self::Shinjitai,
            "kSemanticVariant" => Self::Semantic,
            "kSpecializedSemanticVariant" | "kZVariant" => Self::Other,
            _ => return None,
        })
    }
}

/// One variant of a character
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub ch: String,
    pub variant: String,
    pub kind: VariantKind,
    /// its place among the field's values: 钟's traditional forms are 鍾, then 鐘
    pub pos: usize,
}

/// `U+5B78` as a character
fn code_point(s: &str) -> Option<char> {
    let hex = s.strip_prefix("U+")?;
    char::from_u32(u32::from_str_radix(hex, 16).ok()?)
}

/// The variants in `Unihan_Variants.txt` (`text`), but a character's own (万's traditional
/// forms are 万 and 萬)
pub fn parse_variants(text: &str) -> Vec<Variant> {
    let mut out = vec![];
    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut cols = line.split('\t');
        let (Some(ch), Some(field), Some(values)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let (Some(ch), Some(kind)) = (code_point(ch), VariantKind::of_field(field)) else {
            continue;
        };
        let variants = values
            .split_whitespace()
            .filter_map(|v| code_point(v.split('<').next().unwrap_or("")))
            .filter(|v| *v != ch);
        for (pos, v) in variants.enumerate() {
            out.push(Variant {
                ch: ch.to_string(),
                variant: v.to_string(),
                kind,
                pos,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# Unihan_Variants.txt\n\
        #\tkSemanticVariant\n\
        U+340A\tkSpoofingVariant\tU+340B\n\
        U+4E07\tkTraditionalVariant\tU+4E07 U+842C\n\
        U+5B66\tkJapaneseOldVariant\tU+5B78\n\
        U+5B78\tkSemanticVariant\tU+6588<kLau,kMatthews,kMeyerWempe\n\
        U+949F\tkTraditionalVariant\tU+937E U+9418\n\
        U+7E6A\tkSpecializedSemanticVariant\tU+7E62<kMeyerWempe\n\
        malformed\n";

    #[test]
    fn variants_are_parsed() {
        let vs = parse_variants(TEXT);
        let got: Vec<_> = vs
            .iter()
            .map(|v| (v.ch.as_str(), v.variant.as_str(), v.kind.as_str(), v.pos))
            .collect();
        assert_eq!(
            got,
            [
                // not 万 itself
                ("万", "萬", "traditional", 0),
                ("学", "學", "kyujitai", 0),
                // without its sources
                ("學", "斈", "semantic", 0),
                ("钟", "鍾", "traditional", 0),
                ("钟", "鐘", "traditional", 1),
                ("繪", "繢", "other", 0),
            ]
        );
    }

    #[test]
    fn kinds_round_trip() {
        for k in [
            "traditional",
            "simplified",
            "kyujitai",
            "shinjitai",
            "semantic",
            "other",
        ] {
            assert_eq!(VariantKind::parse(k).unwrap().as_str(), k);
        }
        assert_eq!(VariantKind::parse("spoofing"), None);
    }
}
