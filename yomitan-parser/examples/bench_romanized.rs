//! Time romanized Korean searches (`hangugeo` for 한국어) over synthetic Hangul terms.
//!
//! cargo run --release --features search --example bench_romanized -- <scratch dir>
use std::time::Instant;

use rusqlite::{Connection, params};
use yomitan_parser::{
    korean,
    search::{SearchDb, SearchRequest},
};

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
         CREATE INDEX idx_term_search_score ON term_search (score);
         CREATE TABLE term_link (search_id INTEGER, dict_id INTEGER, bank INTEGER, idx INTEGER);
         CREATE TABLE term_roman (key TEXT NOT NULL, search_id INTEGER NOT NULL, score INTEGER,
           PRIMARY KEY (key, search_id)) WITHOUT ROWID;
         CREATE INDEX idx_term_roman_score ON term_roman (score);",
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
        let id: i64 = tx
            .query_row(
                "INSERT INTO term_search (term, term_ko, score) VALUES (?1, ?1, ?2) RETURNING id",
                params![term, i % 100],
                |r| r.get(0),
            )
            .unwrap();
        for key in korean::keys(&term) {
            tx.execute(
                "INSERT OR IGNORE INTO term_roman (key, search_id, score) VALUES (?1, ?2, ?3)",
                params![key, id, i % 100],
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();
    conn.execute_batch("ANALYZE").unwrap();
    drop(conn);

    println!("with term_roman:");
    run(&SearchDb::open(&db_path, &dir).unwrap());
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("DROP TABLE term_roman").unwrap();
    drop(conn);
    println!("without (search.db from before it):");
    run(&SearchDb::open(&db_path, &dir).unwrap());
}

fn run(db: &SearchDb) {
    for (q, kind) in [
        ("g", None),
        ("a", None),
        ("ha", None),
        ("han", None),
        ("hangu", None),
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
