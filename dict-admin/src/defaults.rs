//! Default dictionaries (ported from cjk-explainer's old `src/util/dicts.ts`).
//!
//! GitHub sources resolve to the matching asset of the repo's latest release;
//! the wty dictionaries have fixed URLs.

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Deserialize;

struct GhSource {
    name: &'static str,
    /// matched against release asset names
    pattern: &'static str,
    lang: Option<&'static str>,
}

const GITHUB: &[(&str, &[GhSource])] = &[
    (
        "MarvNC/pixiv-yomitan",
        &[GhSource {
            name: "Pixiv",
            pattern: r"^Pixiv_.+\.zip$",
            lang: None,
        }],
    ),
    (
        "MarvNC/wikipedia-yomitan",
        &[
            GhSource {
                name: "ja.Wikipedia",
                pattern: r"^ja\.Wikipedia\..+\.zip$",
                lang: Some("ja-JP"),
            },
            GhSource {
                name: "zh.Wikipedia",
                pattern: r"^zh\.Wikipedia\..+\.zip$",
                lang: Some("zh-CN"),
            },
            GhSource {
                name: "ko.Wikipedia",
                pattern: r"^ko\.Wikipedia\..+\.zip$",
                lang: Some("ko-KR"),
            },
        ],
    ),
    (
        "MarvNC/cc-cedict-yomitan",
        &[GhSource {
            name: "CC-CEDICT",
            pattern: r"^CC-CEDICT\.zip$",
            lang: Some("zh-CN"),
        }],
    ),
    (
        "Lyroxide/yomitan-ko-dic",
        &[GhSource {
            name: "KO-EN.KRDICT",
            pattern: r"^KO-EN\.KRDICT\.zip$",
            lang: Some("ko-KR"),
        }],
    ),
];

/// wty (Wiktionary) dictionaries, L1 -> English
const WTY_LANGS: &[&str] = &["ja-JP", "ko-KR", "zh-CN"];

pub struct DefaultDict {
    pub name: &'static str,
    pub lang: Option<&'static str>,
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
        assert_eq!(r[0].lang, Some("zh-CN"));
        assert!(r[0].url.ends_with("/dict/zh/en/wty-zh-en.zip"));
        assert!(resolve(&["Nope".to_string()]).is_err());
        assert_eq!(names().len(), 9);
    }
}
