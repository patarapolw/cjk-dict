//! Replay "typing" against a real search.db to time each keystroke's search.
//!
//! cargo run --release --features search --example bench_search -- <dicts dir>
use std::{path::PathBuf, time::Instant};

use yomitan_parser::search::{SearchDb, SearchRequest};

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("dicts dir"));
    let db = SearchDb::open(&dir.join("search.db"), &dir).unwrap();

    let typing = [
        ("ja-JP", vec!["た", "たべ", "たべる", "食", "食べ", "食べる", "日", "日本", "日本語"]),
        ("zh-CN", vec!["中", "中国", "中国人", "学", "学习"]),
        ("ko-KR", vec!["한", "한국", "한국어", "사", "사람"]),
        ("", vec!["日", "日本", "あ", "a"]),
    ];
    for pass in ["cold", "warm"] {
        println!("--- {pass}");
        for (lang, terms) in &typing {
            for t in terms {
                let req = SearchRequest {
                    lang: lang.to_string(),
                    term: t.to_string(),
                    limit: Some(5),
                    offset: Some(0),
                    ..Default::default()
                };
                let start = Instant::now();
                let r = db.search(&req).unwrap();
                let n: usize = r.items.iter().map(|i| i.entries.len()).sum();
                println!(
                    "{lang:6} {t:8} {:>7.1} ms  items={} entries={n}",
                    start.elapsed().as_secs_f64() * 1000.0,
                    r.items.len()
                );
            }
        }
    }
}
