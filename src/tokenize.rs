use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use lindera_analysis::tokenizer::{Tokenizer, TokenizerBuilder};
use serde_json::json;

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

pub struct TokenizerMapper {
    root_dir: PathBuf,
    tokenizer: Mutex<HashMap<Lang, Arc<Tokenizer>>>,
}

impl TokenizerMapper {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            tokenizer: Mutex::new(HashMap::new()),
        }
    }

    pub async fn tokenize(&self, lang: Lang, text: &str) -> Result<Vec<String>, YomitanError> {
        let tokenizer = {
            let mut tokenizer_map = self.tokenizer.lock()?;
            match tokenizer_map.get(&lang) {
                Some(t) => Arc::clone(t),
                None => {
                    let mut builder = TokenizerBuilder::from_config(json!({
                        "segmenter": {
                            "mode": "normal"
                        },
                        "character_filters": [
                            {
                                "kind": "unicode_normalize",
                                "args": { "kind": "nfkc" }
                            },
                            {
                                "kind": "japanese_iteration_mark",
                                "args": {
                                    "normalize_kanji": true,
                                    "normalize_kana": true
                                }
                            }
                        ],
                        "token_filters": [
                            {
                                "kind": "remove_diacritical_mark",
                                "args": {
                                    // do not remove dakuten, handakuten
                                    "japanese": false
                                }
                            },
                            {
                                "kind": "japanese_kana",
                                "args": {
                                    "kind": "hiragana"
                                }
                            },
                            {
                                "kind": "japanese_katakana_stem",
                                "args": {
                                    "min": 3
                                }
                            },
                            {
                                "kind": "remove_diacritical_mark",
                                "args": {
                                    "japanese": false
                                }
                            },
                            {
                                "kind": "uppercase"
                            }
                        ],
                    }))?;

                    let model = match lang {
                        Lang::Ja => "unidic",
                        Lang::Zh => "jieba",
                        Lang::Ko => "ko-dic",
                    };

                    builder.set_segmenter_dictionary(
                        &self
                            .root_dir
                            .join(format!("lindera-{}", model))
                            .display()
                            .to_string(),
                    );

                    let tokenizer = Arc::new(builder.build()?);
                    tokenizer_map.insert(lang, Arc::clone(&tokenizer));
                    tokenizer
                }
            }
        };

        let mut out = vec![];
        let mut tokens = tokenizer.tokenize(text)?;
        'outer: for token in tokens.iter_mut() {
            let surface = token.surface.to_string();

            // for some reasons, `[,-]` is considered by unidic to be 名詞, 普通名詞, サ変可能
            // filter out numbers too
            if !surface.chars().any(char::is_alphabetic) {
                continue;
            }

            for (i, d) in token.details().iter().enumerate() {
                if *d == "*" {
                    continue;
                }

                match lang {
                    Lang::Ja => {
                        // https://lindera.github.io/lindera/lindera-unidic/dictionary_format.html#dictionary-format
                        // POS is index 4 in docs, but 0 in token.details();
                        let i = i + 4;

                        match i {
                            // POS is 4-7
                            4..8 => {
                                if d.starts_with("助") || d.ends_with("記号") {
                                    continue 'outer;
                                }
                            }
                            // Orthographic base form
                            14 => {
                                out.push(d.to_string());
                                continue 'outer;
                            }
                            _ => (),
                        };
                    }
                    Lang::Ko => {}
                    Lang::Zh => {}
                }
            }

            out.push(surface);
        }

        Ok(out)
    }
}
