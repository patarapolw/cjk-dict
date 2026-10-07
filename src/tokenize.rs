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

use crate::error::YomitanError;

#[derive(Eq, PartialEq, Hash, Clone, Copy)]
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
    const ALL: [Lang; 3] = [Lang::Ja, Lang::Ko, Lang::Zh];

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

        let mut out = vec![];
        let mut tokens = tokenizer.tokenize(text)?;
        let mut is_ko_verb;

        'outer: for token in tokens.iter_mut() {
            is_ko_verb = false;
            let surface = token.surface.to_string();

            // for some reasons, `[,-]` is considered by unidic to be 名詞, 普通名詞, サ変可能
            // filter out numbers too
            if !surface.chars().any(char::is_alphabetic) {
                continue;
            }

            match lang {
                Lang::Ja => {
                    let details = token.details();

                    if let Some(pos) = details.get(0) {
                        if pos.starts_with("助") || pos.ends_with("記号") {
                            continue 'outer;
                        }
                    }

                    // @see https://lindera.github.io/lindera/lindera-unidic/dictionary_format.html
                    // 11 - Lexeme
                    // 14 - Orthographic base form
                    if let Some(d) = details.get(11 - 4)
                        && *d != "*"
                    {
                        out.push(d.to_string());
                        continue 'outer;
                    }

                    if let Some(d) = details.get(14 - 4)
                        && *d != "*"
                    {
                        out.push(d.to_string());
                        continue 'outer;
                    }
                }
                Lang::Ko => {
                    is_ko_verb = false;
                    let details = token.details();

                    if let Some(pos) = details.get(0) {
                        if KO_STOP_TAGS.contains(pos) {
                            continue 'outer;
                        }
                        if KO_VERB_TAGS.contains(pos) {
                            is_ko_verb = true;
                        }
                    }

                    // @see https://lindera.github.io/lindera/lindera-ko-dic/dictionary_format.html
                    // 8 - Type
                    // 11 - Expression
                    if let Some(ty) = details.get(8 - 4)
                        && let Some(expr) = details.get(11 - 4)
                    {
                        if matches!(*ty, "Compound" | "Inflect" | "Preanalysis") && *expr != "*" {
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

                                if let Some(t) = sub_tokens.get(0) {
                                    out.push(if is_ko_verb {
                                        format!("{t}다")
                                    } else {
                                        t.to_string()
                                    });
                                }
                            }
                            continue 'outer;
                        }
                    }
                }
                _ => {}
            }

            out.push(if is_ko_verb {
                format!("{surface}다")
            } else {
                surface.to_string()
            });
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
}
