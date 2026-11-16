//! Time romanized Korean searches (`hangugeo` for 한국어) over synthetic Hangul terms.
//!
//! cargo run --release --features search --example bench_romanized -- <scratch dir>
use std::time::Instant;

use rusqlite::{Connection, params};
use yomitan_parser::search::{SearchDb, SearchRequest};

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("dir"));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("search.db");
    let _ = std::fs::remove_file(&db_path);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE dict (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE term_search (id INTEGER PRIMARY KEY, term TEXT NOT NULL,
           term_ja TEXT NOT NULL DEFAULT '', term_zh TEXT NOT NULL DEFAULT '',
           term_ko TEXT NOT NULL DEFAULT '', reading TEXT NOT NULL DEFAULT '',
           def_tags TEXT, rules TEXT, score INTEGER, tags TEXT);
         CREATE INDEX idx_term ON term_search (term);
         CREATE TABLE term_link (search_id INTEGER, dict_id INTEGER, bank INTEGER, idx INTEGER);",
    )
    .unwrap();
    let mut seed: u64 = 1;
    let mut rnd = |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    let tx = conn.unchecked_transaction().unwrap();
    for i in 0..300_000 {
        let len = 1 + rnd(4);
        let term: String = (0..len)
            .map(|_| char::from_u32(0xAC00 + rnd(11172) as u32).unwrap())
            .collect();
        tx.execute(
            "INSERT INTO term_search (term, term_ko, score) VALUES (?1, ?1, ?2)",
            params![term, i % 100],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    drop(conn);
    let db = SearchDb::open(&db_path, &dir).unwrap();
    for (q, kind) in [
        ("g", None),
        ("a", None),
        ("han", None),
        ("hangugeo", None),
        ("hangugeo", Some("exact")),
    ] {
        let t = Instant::now();
        let n = db
            .search(&SearchRequest {
                kind: kind.map(String::from),
                lang: "ko-KR".into(),
                term: q.into(),
                romanized: true,
                ..Default::default()
            })
            .unwrap()
            .items
            .len();
        println!("{q:10} {kind:?}: {n} items in {:?}", t.elapsed());
    }
}
