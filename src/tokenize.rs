use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use lindera::{dictionary::load_dictionary, mode::Mode, segmenter::Segmenter};
use lindera_analysis::{
    character_filter::CharacterFilterLoader, token_filter::TokenFilterLoader, tokenizer::Tokenizer,
};
use serde_json::json;
use tokio::{sync::OnceCell, task::spawn_blocking};

use crate::{error::YomitanError, util::NonEmpty};

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub enum Lang {
    Ja,
    Ko,
    Zh,
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Lang::Ja => "ja-JP",
            Lang::Ko => "ko-KR",
            Lang::Zh => "zh-CN",
        })
    }
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Ja, Lang::Ko, Lang::Zh];

    pub fn new_oncecell_map<T>() -> HashMap<Lang, OnceCell<T>> {
        Lang::ALL.iter().map(|&l| (l, OnceCell::new())).collect()
    }

    pub fn from(value: &str) -> Option<Self> {
        match value {
            "ja-JP" => Some(Lang::Ja),
            "ko-KR" => Some(Lang::Ko),
            "zh-CN" => Some(Lang::Zh),
            _ => None,
        }
    }
}

static KO_STOP_TAGS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "JKS", "JKC", "JKG", "JKO", "JKB", "JKV", "JKQ", "JX", "JC", "EP", "EF", "EC", "ETN",
        "ETM", "SF", "SE", "SSO", "SSC", "SC", "SY",
    ]
    .into_iter()
    .collect()
});

static KO_VERB_TAGS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| ["VV", "VA", "VX", "VCN"].into_iter().collect());

// lindera `details` skip the first 4 dictionary columns (surface, left id, right id, cost),
// so a column number from the dictionary_format docs is `column - 4`.
const JA_LEXEME: usize = 11 - 4; // unidic: 11 - Lexeme
const JA_ORTH_BASE: usize = 14 - 4; // unidic: 14 - Orthographic base form
const KO_TYPE: usize = 8 - 4; // ko-dic: 8 - Type
const KO_EXPRESSION: usize = 11 - 4; // ko-dic: 11 - Expression

pub struct TokenizerMapper {
    root_dir: PathBuf,
    segmenter: HashMap<Lang, OnceCell<Arc<Segmenter>>>,
    tokenizer: HashMap<Lang, OnceCell<Arc<Tokenizer>>>,
}

impl TokenizerMapper {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            segmenter: Lang::new_oncecell_map(),
            tokenizer: Lang::new_oncecell_map(),
        }
    }

    pub async fn segment(
        &self,
        lang: Lang,
        text: &str,
    ) -> Result<Vec<(String, Vec<String>)>, YomitanError> {
        let segmenter = self.get_segmenter(lang).await?;

        let mut out = vec![];
        let mut tokens = segmenter.segment(std::borrow::Cow::Borrowed(text))?;
        for token in tokens.iter_mut() {
            out.push((
                token.surface.to_string(),
                token.details_iter().map(|s| s.to_string()).collect(),
            ));
        }

        Ok(out)
    }

    pub async fn tokenize(&self, lang: Lang, text: &str) -> Result<Vec<String>, YomitanError> {
        let tokenizer = self.tokenizer[&lang]
            .get_or_try_init(|| async move {
                let character_filters = vec![
                    json!({
                        "kind": "unicode_normalize",
                        "args": { "kind": "nfkc" }
                    }),
                    json!({
                        "kind": "japanese_iteration_mark",
                        "args": {
                            "normalize_kanji": true,
                            "normalize_kana": true
                        }
                    }),
                ];

                let token_filters = vec![
                    json!({
                        "kind": "remove_diacritical_mark",
                        "args": {
                            // do not remove dakuten, handakuten
                            "japanese": false
                        }
                    }),
                    json!({
                        "kind": "japanese_kana",
                        "args": {
                            "kind": "hiragana"
                        }
                    }),
                    json!({
                        "kind": "japanese_katakana_stem",
                        "args": {
                            "min": 3
                        }
                    }),
                    json!({
                        "kind": "remove_diacritical_mark",
                        "args": {
                            "japanese": false
                        }
                    }),
                    json!({
                        "kind": "uppercase"
                    }),
                ];

                let segmenter = self.get_segmenter(lang).await?;
                let mut tokenizer = Tokenizer::new((*segmenter).clone());

                for character_filter_setting in character_filters {
                    let character_filter_name = character_filter_setting["kind"].as_str();
                    if let Some(character_filter_name) = character_filter_name {
                        // Append a character filter to the tokenizer.
                        tokenizer.append_character_filter(CharacterFilterLoader::load_from_value(
                            character_filter_name,
                            &character_filter_setting["args"],
                        )?);
                    }
                }

                for token_filter_setting in token_filters {
                    let token_filter_name = token_filter_setting["kind"].as_str();
                    if let Some(token_filter_name) = token_filter_name {
                        // Append a token filter to the tokenizer.
                        tokenizer.append_token_filter(TokenFilterLoader::load_from_value(
                            token_filter_name,
                            &token_filter_setting["args"],
                        )?);
                    }
                }

                Ok::<_, YomitanError>(Arc::new(tokenizer))
            })
            .await?;

        let mut out: Vec<String> = vec![];
        let mut tokens = tokenizer.tokenize(text)?;
        let mut number_end = None;

        for token in tokens.iter_mut() {
            let surface = token.surface.to_string();
            if !surface.is_empty() && surface.chars().all(char::is_numeric) {
                if number_end == Some(token.byte_start) {
                    if let Some(number) = out.last_mut() {
                        number.push_str(&surface);
                    }
                } else {
                    out.push(surface);
                }
                number_end = Some(token.byte_end);
                continue;
            }

            number_end = None;
            let details = token.details();
            out.extend(terms_for_token(lang, &surface, &details));
        }

        Ok(out)
    }

    async fn get_segmenter(&self, lang: Lang) -> Result<Arc<Segmenter>, YomitanError> {
        let root_dir = self.root_dir.clone();

        self.segmenter[&lang]
            .get_or_try_init(|| async move {
                spawn_blocking(move || {
                    let model = match lang {
                        Lang::Ja => "unidic", // more consistent minimal units than ipadic(-neologd) and much smaller than sudachidict
                        Lang::Zh => "cc-cedict", // cc-cedict may have better support traditional form better than jieba
                        Lang::Ko => "ko-dic",
                    };

                    let dictionary = load_dictionary(
                        &root_dir
                            .join(format!("lindera-{}", model))
                            .display()
                            .to_string(),
                    )?;

                    Ok::<_, YomitanError>(Arc::new(Segmenter::new(Mode::Normal, dictionary, None)))
                })
                .await?
                .map_err(YomitanError::from)
            })
            .await
            .cloned()
    }

    pub(crate) async fn join_tokens(
        &self,
        text: &str,
        for_lang: Lang,
        dict_lang: &Option<Lang>,
    ) -> Result<Option<String>, YomitanError> {
        let Some(preserve_text) = join_gate(text, for_lang, dict_lang) else {
            return Ok(None);
        };

        let tokens = self.tokenize(for_lang, text).await?;
        Ok(join_finish(tokens, text, preserve_text))
    }
}

/// What one lindera token contributes to the search text: nothing (dropped), its dictionary
/// form, or several fragments (Korean compounds and inflections).
/// `details` are the dictionary fields after the surface; index 0 is the part of speech.
fn terms_for_token(lang: Lang, surface: &str, details: &[&str]) -> Vec<String> {
    // for some reasons, `[,-]` is considered by unidic to be 名詞, 普通名詞, サ変可能
    if !surface.chars().any(char::is_alphanumeric) {
        return vec![];
    }
    if surface.chars().all(char::is_numeric) {
        return vec![surface.to_string()];
    }

    let mut is_ko_verb = false;

    match lang {
        Lang::Ja => {
            if let Some(pos) = details.first()
                && (pos.starts_with("助") || pos.ends_with("記号"))
            {
                return vec![];
            }

            // @see https://lindera.github.io/lindera/lindera-unidic/dictionary_format.html
            for idx in [JA_LEXEME, JA_ORTH_BASE] {
                if let Some(d) = details.get(idx)
                    && *d != "*"
                {
                    return vec![d.to_string()];
                }
            }
        }
        Lang::Ko => {
            if let Some(pos) = details.first() {
                if KO_STOP_TAGS.contains(pos) {
                    return vec![];
                }
                if KO_VERB_TAGS.contains(pos) {
                    is_ko_verb = true;
                }
            }

            // @see https://lindera.github.io/lindera/lindera-ko-dic/dictionary_format.html
            if let Some(ty) = details.get(KO_TYPE)
                && let Some(expr) = details.get(KO_EXPRESSION)
                && matches!(*ty, "Compound" | "Inflect" | "Preanalysis")
                && *expr != "*"
            {
                let mut out = vec![];
                for sub_token in expr.split('+') {
                    let sub_tokens: Vec<&str> = sub_token.split('/').collect();
                    if let Some(pos) = sub_tokens.get(1) {
                        if KO_STOP_TAGS.contains(pos) {
                            continue;
                        }
                        if KO_VERB_TAGS.contains(pos) {
                            is_ko_verb = true;
                        }
                    }

                    if let Some(t) = sub_tokens.first() {
                        out.push(if is_ko_verb {
                            format!("{t}다")
                        } else {
                            t.to_string()
                        });
                    }
                }
                return out;
            }
        }
        Lang::Zh => {}
    }

    vec![if is_ko_verb {
        format!("{surface}다")
    } else {
        surface.to_string()
    }]
}

/// Decides whether `text` is tokenized for `for_lang` at all.
/// `None` = skip; `Some(preserve_text)` = tokenize, and fall back to the raw text if
/// tokenizing leaves nothing (only when the dictionary itself declares the language).
fn join_gate(text: &str, for_lang: Lang, dict_lang: &Option<Lang>) -> Option<bool> {
    let mut preserve_text = false;
    if let Some(s_lang) = dict_lang {
        if *s_lang != for_lang {
            return None;
        }
        preserve_text = true;
    }

    if let Some(g_lang) = guess_lang(text)
        && g_lang != for_lang
    {
        return None;
    }

    Some(preserve_text)
}

fn join_finish(tokens: Vec<String>, text: &str, preserve_text: bool) -> Option<String> {
    tokens
        .join(" ")
        .non_empty_trimmed()
        .or_else(|| preserve_text.then(|| text.to_string()))
}

#[derive(Default)]
struct ScriptCounts {
    kana: usize,
    hangul: usize,
    han: usize,
}

fn count_scripts(text: &str) -> ScriptCounts {
    let mut c = ScriptCounts::default();
    for ch in text.chars() {
        match ch as u32 {
            // Hiragana, Katakana, Katakana Phonetic Extensions, half-width katakana
            0x3041..=0x3096
            | 0x309D..=0x309F
            | 0x30A1..=0x30FA
            | 0x30FD..=0x30FF
            | 0x31F0..=0x31FF
            | 0xFF66..=0xFF9F => c.kana += 1,

            // Hangul syllables, Jamo, Compatibility Jamo, Jamo Extended-A/B
            0xAC00..=0xD7A3
            | 0x1100..=0x11FF
            | 0x3130..=0x318F
            | 0xA960..=0xA97F
            | 0xD7B0..=0xD7FF => c.hangul += 1,

            // CJK ideographs: Unified, Extension A, Extension B+, Compatibility
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2FA1F | 0xF900..=0xFAFF => c.han += 1,

            _ => {}
        }
    }
    c
}

pub fn guess_lang(text: &str) -> Option<Lang> {
    let c = count_scripts(text);
    match (c.kana, c.hangul, c.han) {
        (0, 0, 0) => None, // no CJK at all
        (k, h, _) if k > 0 && h > 0 => {
            // mixed: go with the larger share
            Some(if k >= h { Lang::Ja } else { Lang::Ko })
        }
        (k, 0, _) if k > 0 => Some(Lang::Ja), // any kana => Japanese
        (0, h, _) if h > 0 => Some(Lang::Ko), // Hangul (even with Hanja) => Korean
        (0, 0, _) => None,                    // Han only: Chinese or Japanese, ambiguous
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------- Lang

    #[test]
    fn lang_display_and_from_round_trip() {
        for (lang, tag) in [
            (Lang::Ja, "ja-JP"),
            (Lang::Ko, "ko-KR"),
            (Lang::Zh, "zh-CN"),
        ] {
            assert_eq!(lang.to_string(), tag);
            assert_eq!(Lang::from(tag), Some(lang));
        }
    }

    #[test]
    fn lang_from_is_exact_match_only() {
        for bad in ["", "ja", "JA-JP", "ja-jp", " ja-JP", "en-US", "zh-TW"] {
            assert_eq!(Lang::from(bad), None, "input: {bad:?}");
        }
    }

    #[test]
    fn lang_all_lists_every_variant_once() {
        let set: HashSet<Lang> = Lang::ALL.into_iter().collect();
        assert_eq!(Lang::ALL.len(), 3);
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn oncecell_map_has_one_empty_cell_per_language() {
        let map: HashMap<Lang, OnceCell<u8>> = Lang::new_oncecell_map();
        assert_eq!(map.len(), Lang::ALL.len());
        for lang in Lang::ALL {
            assert!(map[&lang].get().is_none(), "{lang}");
        }
    }

    #[test]
    fn ko_tag_sets() {
        assert!(KO_STOP_TAGS.is_disjoint(&KO_VERB_TAGS));
        for tag in ["JKS", "JX", "EP", "EF", "ETM", "SF", "SY"] {
            assert!(KO_STOP_TAGS.contains(tag), "{tag}");
        }
        for tag in ["NNG", "NNP", "VV", "VA", "MAG"] {
            assert!(!KO_STOP_TAGS.contains(tag), "{tag}");
        }
        let verbs: HashSet<&str> = ["VV", "VA", "VX", "VCN"].into_iter().collect();
        assert_eq!(*KO_VERB_TAGS, verbs);
    }

    // ------------------------------------------------- script detection

    fn counts(s: &str) -> (usize, usize, usize) {
        let c = count_scripts(s);
        (c.kana, c.hangul, c.han)
    }

    #[test]
    fn count_scripts_range_boundaries() {
        const KANA: (usize, usize, usize) = (1, 0, 0);
        const HANGUL: (usize, usize, usize) = (0, 1, 0);
        const HAN: (usize, usize, usize) = (0, 0, 1);
        const NONE: (usize, usize, usize) = (0, 0, 0);

        let cases = [
            ('\u{3040}', NONE), // just below hiragana
            ('\u{3041}', KANA), // ぁ
            ('\u{3096}', KANA), // ゖ
            ('\u{3097}', NONE),
            ('\u{3099}', NONE),   // combining dakuten
            ('\u{30A0}', NONE),   // ゠
            ('\u{30A1}', KANA),   // ァ
            ('\u{30FA}', KANA),   // ヺ
            ('\u{30FB}', NONE),   // ・
            ('\u{30FC}', NONE),   // ー (prolonged sound mark is not counted)
            ('\u{30FD}', KANA),   // ヽ
            ('\u{30FF}', KANA),   // ヿ
            ('\u{FF66}', KANA),   // half-width ｦ
            ('\u{FF9F}', KANA),   // half-width ﾟ
            ('\u{AC00}', HANGUL), // 가
            ('\u{D7A3}', HANGUL), // 힣
            ('\u{D7A4}', NONE),
            ('\u{1100}', HANGUL), // conjoining jamo
            ('\u{3131}', HANGUL), // compatibility jamo ㄱ
            ('\u{4E00}', HAN),
            ('\u{9FFF}', HAN),
            ('\u{3400}', HAN),  // extension A
            ('\u{20000}', HAN), // extension B
            ('\u{F900}', HAN),  // compatibility ideographs
            ('a', NONE),
            ('1', NONE),
            ('。', NONE),
        ];
        for (ch, expected) in cases {
            assert_eq!(counts(&ch.to_string()), expected, "U+{:04X}", ch as u32);
        }
    }

    #[test]
    fn count_scripts_adds_up_over_a_string() {
        // の + テ キ ス ト = 5 kana, 日本語 = 3 han
        assert_eq!(counts("日本語のテキスト"), (5, 0, 3));
    }

    #[test]
    fn guess_lang_cases() {
        let cases: &[(&str, Option<Lang>)] = &[
            // no CJK script at all
            ("", None),
            ("hello", None),
            ("123 !?", None),
            ("nǐ hǎo", None),
            ("ー", None),
            // any kana => Japanese
            ("こんにちは", Some(Lang::Ja)),
            ("カタカナ", Some(Lang::Ja)),
            ("ｶﾀｶﾅ", Some(Lang::Ja)),
            ("日本語です", Some(Lang::Ja)),
            // Hangul => Korean, even with Hanja
            ("한국어", Some(Lang::Ko)),
            ("漢字와", Some(Lang::Ko)),
            // Han only is ambiguous
            ("漢字", None),
            ("中文", None),
            ("\u{20000}", None),
            // kana + Hangul: larger share wins, a tie goes to Japanese
            ("あ한", Some(Lang::Ja)),
            ("あいう한", Some(Lang::Ja)),
            ("あ한국어", Some(Lang::Ko)),
        ];
        for &(text, expected) in cases {
            assert_eq!(guess_lang(text), expected, "text: {text:?}");
        }
    }

    // ------------------------------------------------------ join_tokens

    #[test]
    fn join_gate_dictionary_language_decides_skipping() {
        // dictionary says Korean but Japanese was asked for: skip, whatever the text is
        assert_eq!(join_gate("日本語", Lang::Ja, &Some(Lang::Ko)), None);
        assert_eq!(join_gate("", Lang::Zh, &Some(Lang::Ja)), None);
        // same language: tokenize, and keep the raw text as a fallback
        assert_eq!(join_gate("日本語", Lang::Ja, &Some(Lang::Ja)), Some(true));
        assert_eq!(join_gate("漢字", Lang::Zh, &Some(Lang::Zh)), Some(true));
    }

    #[test]
    fn join_gate_without_dictionary_language_guesses_from_script() {
        assert_eq!(join_gate("こんにちは", Lang::Ja, &None), Some(false));
        assert_eq!(join_gate("こんにちは", Lang::Zh, &None), None);
        assert_eq!(join_gate("한국어", Lang::Ko, &None), Some(false));
        assert_eq!(join_gate("한국어", Lang::Ja, &None), None);
        // Han-only text is ambiguous, so it is allowed for every language
        for lang in Lang::ALL {
            assert_eq!(join_gate("漢字", lang, &None), Some(false), "{lang}");
        }
        // no CJK at all: allowed too
        assert_eq!(join_gate("hello", Lang::Ko, &None), Some(false));
    }

    /// Pins current behaviour: the script guess can still veto when the dictionary language
    /// matches (e.g. a Chinese dictionary entry that contains kana gets no `zh` tokens).
    #[test]
    fn join_gate_script_guess_still_vetoes_a_matching_dictionary_language() {
        assert_eq!(join_gate("のび太", Lang::Zh, &Some(Lang::Zh)), None);
        assert_eq!(join_gate("한국어", Lang::Ja, &Some(Lang::Ja)), None);
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn join_finish_joins_with_single_spaces_and_trims() {
        assert_eq!(
            join_finish(strings(&["食べる", "猫"]), "x", false),
            Some("食べる 猫".to_string())
        );
        assert_eq!(
            join_finish(strings(&[" a "]), "x", false),
            Some("a".to_string())
        );
    }

    #[test]
    fn join_finish_falls_back_to_the_text_only_when_preserving() {
        assert_eq!(join_finish(vec![], "。。", true), Some("。。".to_string()));
        assert_eq!(join_finish(vec![], "。。", false), None);
        // blank tokens count as empty
        assert_eq!(join_finish(strings(&["", " "]), "t", false), None);
        assert_eq!(
            join_finish(strings(&["", " "]), "t", true),
            Some("t".to_string())
        );
    }

    #[tokio::test]
    async fn join_tokens_skips_without_loading_any_dictionary() {
        // the folder does not exist: an attempt to tokenize would return an error
        let mapper = TokenizerMapper::new(PathBuf::from("/definitely/not/here"));
        assert_eq!(
            mapper
                .join_tokens("日本語", Lang::Ja, &Some(Lang::Ko))
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            mapper.join_tokens("한국어", Lang::Ja, &None).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn missing_dictionary_is_an_error_and_is_retried() {
        let mapper = TokenizerMapper::new(PathBuf::from("/definitely/not/here"));
        assert!(mapper.tokenize(Lang::Ja, "猫").await.is_err());
        assert!(mapper.tokenize(Lang::Ja, "猫").await.is_err()); // a failed init is not cached
        assert!(mapper.segment(Lang::Ko, "고양이").await.is_err());
    }

    // ------------------------------------------- per-token filtering

    const POS: usize = 0;

    /// A `details` slice with `*` everywhere except the listed (index, value) fields.
    fn det(fields: &[(usize, &'static str)]) -> Vec<&'static str> {
        let len = fields
            .iter()
            .map(|&(i, _)| i + 1)
            .max()
            .unwrap_or(0)
            .max(12);
        let mut v = vec!["*"; len];
        for &(i, value) in fields {
            v[i] = value;
        }
        v
    }

    #[test]
    fn punctuation_only_tokens_are_dropped_and_numbers_are_kept_in_every_language() {
        for lang in Lang::ALL {
            for surface in [",", "-", "。", " ", "…"] {
                let out = terms_for_token(lang, surface, &det(&[(POS, "名詞")]));
                assert!(out.is_empty(), "{lang} {surface:?} -> {out:?}");
            }

            for surface in ["123", "２０２６"] {
                assert_eq!(
                    terms_for_token(lang, surface, &det(&[(POS, "名詞")])),
                    vec![surface],
                    "{lang} {surface:?}"
                );
            }
        }
    }

    #[test]
    fn ja_drops_particles_auxiliaries_and_symbols() {
        for pos in ["助詞", "助動詞", "補助記号", "記号"] {
            let out = terms_for_token(Lang::Ja, "が", &det(&[(POS, pos)]));
            assert!(out.is_empty(), "{pos}");
        }
        assert_eq!(
            terms_for_token(Lang::Ja, "猫", &det(&[(POS, "名詞")])),
            vec!["猫"]
        );
    }

    #[test]
    fn ja_prefers_lexeme_then_orthographic_base_form_then_surface() {
        let both = det(&[
            (POS, "動詞"),
            (JA_LEXEME, "食べる"),
            (JA_ORTH_BASE, "食べる-orth"),
        ]);
        assert_eq!(terms_for_token(Lang::Ja, "食べ", &both), vec!["食べる"]);

        let only_orth = det(&[(POS, "動詞"), (JA_ORTH_BASE, "食べる")]);
        assert_eq!(
            terms_for_token(Lang::Ja, "食べ", &only_orth),
            vec!["食べる"]
        );

        let neither = det(&[(POS, "動詞")]);
        assert_eq!(terms_for_token(Lang::Ja, "食べ", &neither), vec!["食べ"]);

        assert_eq!(terms_for_token(Lang::Ja, "猫", &[]), vec!["猫"]); // no details at all
    }

    #[test]
    fn ko_drops_stop_tags() {
        for pos in ["JKS", "JX", "EP", "EF", "EC", "ETM", "SF", "SY"] {
            let out = terms_for_token(Lang::Ko, "x", &det(&[(POS, pos)]));
            assert!(out.is_empty(), "{pos}");
        }
    }

    #[test]
    fn ko_bare_verb_and_adjective_stems_get_da_appended() {
        for pos in ["VV", "VA", "VX", "VCN"] {
            assert_eq!(
                terms_for_token(Lang::Ko, "먹", &det(&[(POS, pos)])),
                vec!["먹다"],
                "{pos}"
            );
        }
    }

    #[test]
    fn ko_other_parts_of_speech_keep_the_surface() {
        for pos in ["NNG", "NNP", "MAG", "SN"] {
            assert_eq!(
                terms_for_token(Lang::Ko, "학교", &det(&[(POS, pos)])),
                vec!["학교"],
                "{pos}"
            );
        }
    }

    #[test]
    fn ko_inflected_form_becomes_the_dictionary_form() {
        // 갔 = 가 (VV) + 았 (EP)
        let d = det(&[
            (POS, "VV+EP"),
            (KO_TYPE, "Inflect"),
            (KO_EXPRESSION, "가/VV/*+았/EP/*"),
        ]);
        assert_eq!(terms_for_token(Lang::Ko, "갔", &d), vec!["가다"]);
    }

    #[test]
    fn ko_expression_keeps_every_verb_and_drops_stop_fragments() {
        // 먹어보 = 먹 (VV) + 어 (EC) + 보 (VX)
        let d = det(&[
            (POS, "VV+EC+VX"),
            (KO_TYPE, "Inflect"),
            (KO_EXPRESSION, "먹/VV/*+어/EC/*+보/VX/*"),
        ]);
        assert_eq!(
            terms_for_token(Lang::Ko, "먹어보", &d),
            vec!["먹다", "보다"]
        );

        // trailing sentence-final ending is dropped
        let d = det(&[
            (POS, "VV+EP+EF"),
            (KO_TYPE, "Inflect"),
            (KO_EXPRESSION, "가/VV/*+았/EP/*+다/EF/*"),
        ]);
        assert_eq!(terms_for_token(Lang::Ko, "갔다", &d), vec!["가다"]);
    }

    #[test]
    fn ko_compound_nouns_are_split_without_da() {
        for ty in ["Compound", "Preanalysis"] {
            let d = det(&[
                (POS, "NNG"),
                (KO_TYPE, ty),
                (KO_EXPRESSION, "무궁/NNG/*+화/NNG/*"),
            ]);
            assert_eq!(
                terms_for_token(Lang::Ko, "무궁화", &d),
                vec!["무궁", "화"],
                "{ty}"
            );
        }
    }

    #[test]
    fn ko_only_listed_types_with_an_expression_are_decomposed() {
        // ordinary entry: the expression is ignored
        let d = det(&[
            (POS, "NNG"),
            (KO_TYPE, "*"),
            (KO_EXPRESSION, "무궁/NNG/*+화/NNG/*"),
        ]);
        assert_eq!(terms_for_token(Lang::Ko, "무궁화", &d), vec!["무궁화"]);
        // listed type but nothing to decompose into
        let d = det(&[(POS, "NNG"), (KO_TYPE, "Compound"), (KO_EXPRESSION, "*")]);
        assert_eq!(terms_for_token(Lang::Ko, "무궁화", &d), vec!["무궁화"]);
    }

    #[test]
    fn ko_expression_fragment_without_a_pos_is_kept_as_is() {
        let d = det(&[(POS, "NNG"), (KO_TYPE, "Compound"), (KO_EXPRESSION, "가")]);
        assert_eq!(terms_for_token(Lang::Ko, "가", &d), vec!["가"]);
    }

    /// Known bug: `is_ko_verb` is set once and then applies to every later fragment, so a noun
    /// that follows a verb in one expression also gets 다. Run with
    /// `cargo test known_bug -- --ignored` to see it fail; remove the `ignore` once fixed.
    #[test]
    #[ignore = "known bug: the verb flag leaks to later fragments"]
    fn known_bug_ko_verb_flag_must_not_leak_to_later_fragments() {
        let d = det(&[
            (POS, "VV+NNG"),
            (KO_TYPE, "Compound"),
            (KO_EXPRESSION, "가/VV/*+학교/NNG/*"),
        ]);
        assert_eq!(
            terms_for_token(Lang::Ko, "가학교", &d),
            vec!["가다", "학교"]
        );
    }

    #[test]
    fn zh_ignores_the_japanese_and_korean_rules() {
        let d = det(&[
            (POS, "助詞"),
            (JA_LEXEME, "別"),
            (KO_TYPE, "Compound"),
            (KO_EXPRESSION, "a/NNG/*+b/NNG/*"),
        ]);
        assert_eq!(terms_for_token(Lang::Zh, "北京", &d), vec!["北京"]);
    }

    // ------------------------------------- real dictionaries (opt in)
    //
    // Point LINDERA_DICT_DIR at the folder holding `lindera-unidic`, `lindera-ko-dic` and
    // `lindera-cc-cedict` (the lindera 6.2.0 release zips), then run:
    //
    //     cargo test -p yomitan-parser dict_ -- --ignored
    //
    // Expected values depend on those dictionary versions.

    fn real_mapper() -> TokenizerMapper {
        let dir = std::env::var("LINDERA_DICT_DIR")
            .expect("set LINDERA_DICT_DIR to run the dictionary tests");
        TokenizerMapper::new(PathBuf::from(dir))
    }

    #[tokio::test]
    #[ignore = "needs LINDERA_DICT_DIR"]
    async fn dict_ja_keeps_content_words_in_dictionary_form() {
        let m = real_mapper();
        assert_eq!(
            m.tokenize(Lang::Ja, "猫が好きです。").await.unwrap(),
            ["猫", "好き"]
        );
        assert_eq!(
            m.tokenize(Lang::Ja, "食べました").await.unwrap(),
            ["食べる"]
        );
        assert_eq!(
            m.tokenize(Lang::Ja, "彼は走っている").await.unwrap(),
            ["彼", "走る", "居る"]
        );
    }

    #[tokio::test]
    #[ignore = "needs LINDERA_DICT_DIR"]
    async fn dict_ja_keeps_numbers_and_drops_punctuation() {
        let m = real_mapper();
        assert_eq!(
            m.tokenize(Lang::Ja, "2026年10月14日").await.unwrap(),
            ["2026", "年", "10", "月", "14", "日"]
        );
        assert_eq!(
            m.tokenize(Lang::Ja, "2026-10").await.unwrap(),
            ["2026", "10"]
        );
        assert_eq!(
            m.tokenize(Lang::Ja, "２０２６").await.unwrap(),
            ["2026"]
        );
        assert!(m.tokenize(Lang::Ja, "。。").await.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs LINDERA_DICT_DIR"]
    async fn dict_ko_returns_dictionary_forms_and_drops_particles() {
        let m = real_mapper();
        assert_eq!(m.tokenize(Lang::Ko, "갔다").await.unwrap(), ["가다"]);
        assert_eq!(
            m.tokenize(Lang::Ko, "먹어보았다").await.unwrap(),
            ["먹다", "보다"]
        );
        assert_eq!(
            m.tokenize(Lang::Ko, "나는 책을 읽는다").await.unwrap(),
            ["나", "책", "읽다"]
        );
        assert_eq!(
            m.tokenize(Lang::Ko, "무궁화 꽃이 피었습니다")
                .await
                .unwrap(),
            ["무궁", "화", "꽃", "피다"]
        );
    }

    #[tokio::test]
    #[ignore = "needs LINDERA_DICT_DIR"]
    async fn dict_zh_splits_words_and_drops_punctuation() {
        let m = real_mapper();
        assert_eq!(
            m.tokenize(Lang::Zh, "我爱北京天安门").await.unwrap(),
            ["我", "爱", "北京", "天安门"]
        );
        assert_eq!(
            m.tokenize(Lang::Zh, "你好，世界！").await.unwrap(),
            ["你好", "世界"]
        );
    }

    #[tokio::test]
    #[ignore = "needs LINDERA_DICT_DIR"]
    async fn dict_latin_words_are_uppercased_in_every_language() {
        let m = real_mapper();
        assert_eq!(m.tokenize(Lang::Ja, "Tokyo").await.unwrap(), ["TOKYO"]);
        assert_eq!(
            m.tokenize(Lang::Ko, "iPhone을 샀다").await.unwrap(),
            ["IPHONE", "사다"]
        );
        assert_eq!(
            m.tokenize(Lang::Zh, "我用iPhone").await.unwrap(),
            ["我", "用", "IPHONE"]
        );
    }

    #[tokio::test]
    #[ignore = "needs LINDERA_DICT_DIR"]
    async fn dict_segment_returns_surface_and_part_of_speech() {
        let m = real_mapper();
        let out = m.segment(Lang::Ja, "猫").await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "猫");
        assert_eq!(out[0].1[POS], "名詞");
    }

    #[tokio::test]
    #[ignore = "needs LINDERA_DICT_DIR"]
    async fn dict_join_tokens_end_to_end() {
        let m = real_mapper();
        // tokens joined by one space
        assert_eq!(
            m.join_tokens("猫が好きです。", Lang::Ja, &None)
                .await
                .unwrap(),
            Some("猫 好き".to_string())
        );
        // nothing left after filtering: the raw text survives only when the dictionary says so
        assert_eq!(
            m.join_tokens("。。", Lang::Ja, &Some(Lang::Ja))
                .await
                .unwrap(),
            Some("。。".to_string())
        );
        assert_eq!(m.join_tokens("。。", Lang::Ja, &None).await.unwrap(), None);
        // wrong language for the dictionary
        assert_eq!(
            m.join_tokens("猫", Lang::Zh, &Some(Lang::Ja))
                .await
                .unwrap(),
            None
        );
    }
}
