//! Default dictionaries (ported from cjk-explainer's old `src/util/dicts.ts`).
//!
//! GitHub sources resolve to the matching asset of the repo's latest release;
//! the wty dictionaries have fixed URLs. KANJIDIC comes as JSON, which `add` converts.

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Deserialize;

struct GhSource {
    name: &'static str,
    /// matched against release asset names
    pattern: &'static str,
    /// source language (L1), the terms'
    lang: Option<&'static str>,
    /// target language (L2), the definitions', when index.json doesn't say
    target_lang: Option<&'static str>,
}

const GITHUB: &[(&str, &[GhSource])] = &[
    (
        // KANJIDIC2 (EDRDG, part of the JMdict project) as JSON; `add` converts it to a
        // Yomitan kanji dictionary. Kanji: readings (also Chinese, Korean and Vietnamese),
        // meanings, stroke counts, grades, ...
        "scriptin/jmdict-simplified",
        &[GhSource {
            name: "KANJIDIC",
            pattern: r"^kanjidic2-en-.+\.json\.zip$",
            // Japanese, Chinese and Korean readings alike
            lang: None,
            target_lang: None,
        }],
    ),
    (
        "MarvNC/pixiv-yomitan",
        &[GhSource {
            name: "Pixiv",
            pattern: r"^Pixiv_.+\.zip$",
            lang: None,
            // an encyclopedia in Japanese
            target_lang: Some("ja"),
        }],
    ),
    (
        "MarvNC/wikipedia-yomitan",
        &[
            GhSource {
                name: "ja.Wikipedia",
                pattern: r"^ja\.Wikipedia\..+\.zip$",
                lang: Some("ja-JP"),
                target_lang: Some("ja"),
            },
            GhSource {
                name: "zh.Wikipedia",
                pattern: r"^zh\.Wikipedia\..+\.zip$",
                // Chinese Wikipedia is written in both scripts, for every region
                lang: Some("zh"),
                target_lang: Some("zh"),
            },
            GhSource {
                name: "ko.Wikipedia",
                pattern: r"^ko\.Wikipedia\..+\.zip$",
                lang: Some("ko-KR"),
                target_lang: Some("ko"),
            },
        ],
    ),
    (
        "MarvNC/cc-cedict-yomitan",
        &[GhSource {
            name: "CC-CEDICT",
            pattern: r"^CC-CEDICT\.zip$",
            // Mandarin, traditional and simplified alike
            lang: Some("zh"),
            target_lang: Some("en"),
        }],
    ),
    (
        "Lyroxide/yomitan-ko-dic",
        &[GhSource {
            name: "KO-EN.KRDICT",
            pattern: r"^KO-EN\.KRDICT\.zip$",
            lang: Some("ko-KR"),
            target_lang: Some("en"),
        }],
    ),
];

/// wty (Wiktionary) dictionaries, L1 -> English. Chinese is `zh`: Wiktionary has both
/// scripts, and more than Mandarin of the mainland.
const WTY_LANGS: &[&str] = &["ja-JP", "ko-KR", "zh"];

pub struct DefaultDict {
    pub name: &'static str,
    pub lang: Option<&'static str>,
    pub target_lang: Option<&'static str>,
    pub url: String,
}

/// All default dictionary names, in import order.
pub fn names() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = GITHUB
        .iter()
        .flat_map(|(_, srcs)| srcs.iter().map(|s| s.name))
        .collect();
    out.extend(WTY_LANGS.iter().map(|l| wty_name(l)));
    out
}

fn wty_name(lang: &str) -> &'static str {
    match lang {
        "ja-JP" => "wty-ja-en",
        "ko-KR" => "wty-ko-en",
        _ => "wty-zh-en",
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// @see https://docs.github.com/en/rest/releases/releases#get-the-latest-release
fn latest_release(repo: &str) -> Result<Release> {
    tokio::task::block_in_place(|| {
        let client = reqwest::blocking::Client::builder()
            .user_agent("cjk-dict-admin")
            .build()?;
        let mut req = client
            .get(format!(
                "https://api.github.com/repos/{repo}/releases/latest"
            ))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        // optional; raises the unauthenticated limit of 60 requests/hour
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            req = req.bearer_auth(token);
        }
        let res = req.send()?;
        if !res.status().is_success() {
            bail!("GitHub {repo}: {}", res.status());
        }
        Ok(serde_json::from_str(&res.text()?)?)
    })
}

/// Resolve download URLs. `only` filters by name (empty = all).
pub fn resolve(only: &[String]) -> Result<Vec<DefaultDict>> {
    let all = names();
    for n in only {
        if !all.contains(&n.as_str()) {
            bail!(
                "unknown default dictionary {n:?}; one of: {}",
                all.join(", ")
            );
        }
    }
    let wanted = |name: &str| only.is_empty() || only.iter().any(|n| n == name);

    let mut out = vec![];
    for (repo, srcs) in GITHUB {
        let srcs: Vec<&GhSource> = srcs.iter().filter(|s| wanted(s.name)).collect();
        if srcs.is_empty() {
            continue;
        }
        let release = latest_release(repo)?;
        for s in srcs {
            let re = Regex::new(s.pattern)?;
            let asset = release
                .assets
                .iter()
                .find(|a| re.is_match(&a.name))
                .with_context(|| {
                    format!(
                        "{repo} release {}: no asset matches {}",
                        release.tag_name, s.pattern
                    )
                })?;
            out.push(DefaultDict {
                name: s.name,
                lang: s.lang,
                target_lang: s.target_lang,
                url: asset.browser_download_url.clone(),
            });
        }
    }

    for lang in WTY_LANGS {
        let name = wty_name(lang);
        if !wanted(name) {
            continue;
        }
        let l1 = lang.split('-').next().unwrap_or(lang);
        out.push(DefaultDict {
            name,
            lang: Some(lang),
            target_lang: Some("en"),
            url: format!(
                "https://huggingface.co/datasets/daxida/wty-release/resolve/main/latest/dict/{l1}/en/{name}.zip"
            ),
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_release_asset_names() {
        let examples = [
            ("Pixiv", "Pixiv_2026-11-01.zip", "PixivLight_2026-11-01.zip"),
            (
                "ja.Wikipedia",
                "ja.Wikipedia.2026-11-01.zip",
                "zh.Wikipedia.2026-11-01.zip",
            ),
            (
                "zh.Wikipedia",
                "zh.Wikipedia.2026-11-01.zip",
                "ja.Wikipedia.2026-11-01.zip",
            ),
            (
                "ko.Wikipedia",
                "ko.Wikipedia.2026-11-01.zip",
                "ko.Wikipedia.2026-11-01.json",
            ),
            ("CC-CEDICT", "CC-CEDICT.zip", "CC-CEDICT.Hanzi.zip"),
            (
                "KANJIDIC",
                "kanjidic2-en-3.6.2+20261116213322.json.zip",
                "kanjidic2-all-3.6.2+20261116213322.json.zip",
            ),
            ("KO-EN.KRDICT", "KO-EN.KRDICT.zip", "KO-KO.KRDICT.zip"),
        ];
        for (name, yes, no) in examples {
            let s = GITHUB
                .iter()
                .flat_map(|(_, srcs)| srcs.iter())
                .find(|s| s.name == name)
                .unwrap();
            let re = Regex::new(s.pattern).unwrap();
            assert!(re.is_match(yes), "{name} should match {yes}");
            assert!(!re.is_match(no), "{name} should not match {no}");
        }
    }

    #[test]
    fn only_filters_without_network() {
        let r = resolve(&["wty-zh-en".to_string()]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].lang, Some("zh"));
        assert!(r[0].url.ends_with("/dict/zh/en/wty-zh-en.zip"));
        assert!(resolve(&["Nope".to_string()]).is_err());
        assert_eq!(names().len(), 10);
    }
}
