//! Rebank a zip, check the rows are unchanged, and time reading single entries before/after.
//!
//! cargo run --release --features zip --example bench_rebank -- <src.zip> <out.zip>
use std::{path::PathBuf, time::Instant};

use serde_json::value::RawValue;
use yomitan_parser::parser::{BankKind, TERM_BANK_BYTES, YomitanZip, rebank};

/// (bank number, rows) for every term bank
fn term_rows(path: &PathBuf) -> Vec<(u32, Vec<Box<RawValue>>)> {
    let mut z = YomitanZip::open(path).unwrap();
    let banks: Vec<_> = z.banks().into_iter().filter(|b| b.kind == BankKind::Term).collect();
    banks
        .iter()
        .map(|b| (b.index, serde_json::from_str(&z.read_to_string(&b.name).unwrap()).unwrap()))
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1).map(PathBuf::from);
    let (src, dst) = (args.next().unwrap(), args.next().unwrap());

    let t = Instant::now();
    let changed = rebank(&src, &dst, TERM_BANK_BYTES).unwrap();
    println!("rebank: {changed} in {:.1}s", t.elapsed().as_secs_f64());
    if !changed {
        return;
    }
    let (a, b) = (term_rows(&src), term_rows(&dst));
    let flat = |v: &[(u32, Vec<Box<RawValue>>)]| -> Vec<String> {
        v.iter().flat_map(|(_, r)| r.iter().map(|x| x.get().to_string())).collect()
    };
    let (fa, fb) = (flat(&a), flat(&b));
    assert_eq!(fa, fb, "rows differ");
    println!(
        "rows identical: {}  banks {} -> {}  size {:.1} -> {:.1} MB",
        fa.len(),
        a.len(),
        b.len(),
        std::fs::metadata(&src).unwrap().len() as f64 / 1e6,
        std::fs::metadata(&dst).unwrap().len() as f64 / 1e6
    );
    // numbering is 1..=n
    assert!(b.iter().enumerate().all(|(i, (n, _))| *n as usize == i + 1));

    // rebanking again is a no-op
    let again = dst.with_extension("again.zip");
    assert!(!rebank(&dst, &again, TERM_BANK_BYTES).unwrap());

    // cost of reading one entry = inflating + splitting its bank (no cache)
    for (label, path, banks) in [("before", &src, &a), ("after", &dst, &b)] {
        let mut z = YomitanZip::open(path).unwrap();
        let picks: Vec<u32> = (0..10).map(|i| banks[i * banks.len() / 10].0).collect();
        let t = Instant::now();
        for n in &picks {
            let s = z.read_to_string(&format!("term_bank_{n}.json")).unwrap();
            let rows: Vec<&RawValue> = serde_json::from_str(&s).unwrap();
            std::hint::black_box(rows);
        }
        println!(
            "{label:6}: {:.1} ms per entry read",
            t.elapsed().as_secs_f64() * 1000.0 / picks.len() as f64
        );
    }
}
