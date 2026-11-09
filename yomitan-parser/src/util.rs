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
