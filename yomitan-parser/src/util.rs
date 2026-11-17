pub trait NonEmpty: Sized {
    fn non_empty_trimmed(self) -> Option<Self>;
    fn non_empty(self) -> Option<Self>;
}

impl NonEmpty for String {
    fn non_empty_trimmed(self) -> Option<Self> {
        let t = self.trim();
        (!t.is_empty()).then(|| t.to_owned()) // also strips surrounding whitespace
    }
    fn non_empty(self) -> Option<String> {
        (!self.is_empty()).then_some(self)
    }
}

pub trait OptionNonEmpty {
    fn non_empty_trimmed(self) -> Self;
    fn non_empty(self) -> Self;
}

impl OptionNonEmpty for Option<String> {
    fn non_empty_trimmed(self) -> Self {
        self.and_then(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else if t.len() == s.len() {
                Some(s)
            }
            // already trimmed: reuse
            else {
                Some(t.to_owned())
            }
        })
    }
    fn non_empty(self) -> Self {
        self.filter(|s| !s.is_empty())
    }
}

use std::borrow::Cow;

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

fn nfkc(text: &str) -> Cow<'_, str> {
    if is_nfkc_quick(text.chars()) == IsNormalized::Yes {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfkc().collect())
    }
}

/// NFKC + uppercase; the canonical form stored in `term_search.term` / `reading`.
pub fn normalize_text(text: &str) -> String {
    nfkc(text).to_uppercase()
}

/// Katakana as hiragana (ア → あ, ヴ → ゔ), so that either finds the other. ー and the
/// katakana without a hiragana counterpart (ヷ ヸ ヹ ヺ) are kept.
pub fn fold_kana(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|c| ('\u{30A1}'..='\u{30F6}').contains(&c)) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|c| match c {
                '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
                c => c,
            })
            .collect(),
    )
}

/// [`normalize_text`], with katakana as hiragana ([`fold_kana`]): what a search.db built
/// since kana folding stores.
pub fn normalize_term(text: &str) -> String {
    let s = normalize_text(text);
    match fold_kana(&s) {
        Cow::Borrowed(_) => s,
        Cow::Owned(f) => f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katakana_folds_to_hiragana() {
        assert_eq!(normalize_term("コーヒー"), "こーひー");
        assert_eq!(normalize_term("ｺｰﾋｰ"), "こーひー");
        assert_eq!(normalize_term("キョウ"), "きょう");
        assert_eq!(normalize_term("ヴァイオリンとヵ月"), "ゔぁいおりんとゕ月");
        assert_eq!(normalize_term("ヷ"), "ヷ");
        assert_eq!(normalize_term("abcカ"), "ABCか");
        assert!(matches!(fold_kana("ひらがな"), Cow::Borrowed(_)));
    }
}
