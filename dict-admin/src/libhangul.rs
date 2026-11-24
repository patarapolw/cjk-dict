//! libhangul's Hanja table as a Yomitan kanji dictionary of Hanja, with each character's
//! Hangul readings (eum) and meaning-readings (훈음: 光 빛 광).
//!
//! [libhangul](https://github.com/libhangul/libhangul) has no Yomitan release, so `add`
//! converts its `hanja.txt`: comment lines (`#`, its copyright and license first), then a
//! line per reading of a character or word, `hangul:hanja:meanings`, e.g. `광:光:빛 광`;
//! the meanings, `, `-separated, may be empty (`학:学:`).
//!
//! - Single characters only: words are left out, and so are lines that don't parse.
//! - A character's readings are those [`keep_eum`] keeps: 樂 악 락 요, not also 낙.
//! - Its meanings are those of the readings kept: 樂 풍류 악, 즐길 악, 즐거울 락, ...
//! - A character with no meaning (simplified forms, 学) is left out: its readings alone are
//!   in `search.db` already.
//!
//! libhangul is BSD-3-Clause, whose notice `hanja.txt` starts with: it is the dictionary's
//! attribution.

use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde_json::json;
use sha2::{Digest, Sha256};
use yomitan_parser::{
    hanja::{keep_eum, main_reading},
    parser::TERM_BANK_BYTES,
};
use zip::{CompressionMethod, DateTime, ZipWriter, write::SimpleFileOptions};

/// Where `defaults` downloads `hanja.txt` from: the repo has no releases of it.
pub const URL: &str =
    "https://raw.githubusercontent.com/libhangul/libhangul/main/data/hanja/hanja.txt";

/// Whether `path` is libhangul's `hanja.txt`: comment lines, then `hangul:hanja:meanings`.
pub fn is_hanja_txt(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let mut head = vec![0u8; 8192];
    let Ok(n) = f.read(&mut head) else {
        return false;
    };
    head.truncate(n);
    // whole lines, in UTF-8 (the cut may split a character)
    let text = match std::str::from_utf8(&head) {
        Ok(s) => s,
        Err(e) if e.error_len().is_none() => {
            std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or_default()
        }
        Err(_) => return false,
    };
    let Some((lines, _)) = text.rsplit_once('\n') else {
        return false;
    };
    let rows: Vec<&str> = lines
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .collect();
    !rows.is_empty() && rows.iter().all(|l| parse_line(l).is_some())
}

fn is_hangul(c: char) -> bool {
    matches!(c, '\u{AC00}'..='\u{D7A3}' | '\u{1100}'..='\u{11FF}' | '\u{3131}'..='\u{318E}')
}

/// `(hangul, hanja, meanings)`
fn parse_line(line: &str) -> Option<(&str, &str, &str)> {
    let mut cols = line.trim_end_matches('\r').splitn(3, ':');
    let (hangul, hanja, meanings) = (cols.next()?, cols.next()?, cols.next()?);
    (!hangul.is_empty() && hangul.chars().all(is_hangul) && !hanja.is_empty())
        .then_some((hangul, hanja, meanings))
}

/// A character's readings, each with its meanings, in `hanja.txt`'s order
type Readings = Vec<(char, Vec<String>)>;

/// Convert libhangul's `hanja.txt` at `src` to a Yomitan kanji dictionary at `dst`. The
/// output is deterministic. Returns the number of characters.
pub fn convert(src: &Path, dst: &Path) -> Result<usize> {
    let bytes = std::fs::read(src)?;
    let text = std::str::from_utf8(&bytes).context("hanja.txt is not UTF-8")?;

    let notice: Vec<&str> = text
        .lines()
        .take_while(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim())
        .collect();
    let notice = notice.join("\n").trim().to_string();

    // character -> readings, in file order
    let mut chars: Vec<(char, Readings)> = vec![];
    let mut at: std::collections::HashMap<char, usize> = Default::default();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        // a few word lines have stray control characters (`무등\u{92}\u{80}:無等\u{92}\u{80}:`)
        let Some((hangul, hanja, meanings)) = parse_line(line) else {
            continue;
        };
        let mut hc = hanja.chars();
        let mut rc = hangul.chars();
        let (Some(c), None, Some(r), None) = (hc.next(), hc.next(), rc.next(), rc.next()) else {
            // a word
            continue;
        };
        let meanings = meanings
            .split(", ")
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(String::from);
        let k = *at.entry(c).or_insert_with(|| {
            chars.push((c, vec![]));
            chars.len() - 1
        });
        let readings = &mut chars[k].1;
        match readings.iter_mut().find(|(x, _)| *x == r) {
            Some((_, ms)) => {
                for m in meanings {
                    if !ms.contains(&m) {
                        ms.push(m);
                    }
                }
            }
            None => readings.push((r, meanings.collect())),
        }
    }

    // [character, readings, "", tags, meanings, stats]; the main reading first
    let mut rows = vec![];
    for (c, readings) in &chars {
        let main = main_reading(*c);
        let all: Vec<char> = readings.iter().map(|(r, _)| *r).collect();
        let mut kept = keep_eum(main, &all);
        if let Some(i) = kept.iter().position(|&r| Some(r) == main) {
            kept[..=i].rotate_right(1);
        }
        let meanings: Vec<&String> = kept
            .iter()
            .flat_map(|r| &readings.iter().find(|(x, _)| x == r).unwrap().1)
            .collect();
        if meanings.is_empty() {
            continue;
        }
        let readings: Vec<String> = kept.iter().map(|r| r.to_string()).collect();
        rows.push(json!([
            c.to_string(),
            readings.join(" "),
            "",
            "",
            meanings,
            {}
        ]));
    }
    if rows.is_empty() {
        bail!("hanja.txt has no characters with meanings");
    }

    let sha = Sha256::digest(&bytes);
    let index = json!({
        "title": "Hanja",
        // hanja.txt has no version: its hash tells revisions apart
        "revision": format!("hanja.txt {:x}", &sha)[..18].to_string(),
        "format": 3,
        "sequenced": false,
        "author": "libhangul",
        "url": "https://github.com/libhangul/libhangul",
        "description": "Hanja with their Hangul readings and meanings (훈음), from libhangul's hanja.txt, converted by cjk-dict's dict-admin.",
        "attribution": notice,
        "sourceLanguage": "ko",
        "targetLanguage": "ko",
    });

    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::DEFAULT);
    let mut w = ZipWriter::new(std::io::BufWriter::new(File::create(dst)?));
    w.start_file("index.json", opts)?;
    w.write_all(serde_json::to_string(&index)?.as_bytes())?;

    let mut n = 0;
    let mut chunk: Vec<u8> = vec![];
    let mut flush = |w: &mut ZipWriter<_>, chunk: &mut Vec<u8>| -> Result<()> {
        if chunk.is_empty() {
            return Ok(());
        }
        n += 1;
        w.start_file(format!("kanji_bank_{n}.json"), opts)?;
        w.write_all(b"[")?;
        w.write_all(chunk)?;
        w.write_all(b"]")?;
        chunk.clear();
        Ok(())
    };
    for row in &rows {
        let row = serde_json::to_vec(row)?;
        if !chunk.is_empty() && chunk.len() + 1 + row.len() > TERM_BANK_BYTES {
            flush(&mut w, &mut chunk)?;
        }
        if !chunk.is_empty() {
            chunk.push(b',');
        }
        chunk.extend_from_slice(&row);
    }
    flush(&mut w, &mut chunk)?;
    w.finish()?.flush()?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yomitan_parser::parser::{BankEntries, BankKind, YomitanZip};

    const SAMPLE: &str = "# Copyright (c) 2005,2006 Choe Hwanjin\n# All rights reserved.\n#\nㄱㄴ순:ㄱㄴ順:\n가가:呵呵:\n광:光:빛 광\n낙:樂:즐길 낙\n락:樂:즐거울 락, 즐길 락\n악:樂:풍류 악, 즐길 악\n요:樂:좋아할 요, 즐길 요\n낙:諾:대답할 낙, 허락할 낙\n락:諾:대답할 락, 허락할 락\n학:学:\n학:學:배울 학\n";

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dict-admin-libhangul-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn converts_to_a_yomitan_kanji_dictionary() {
        let src = temp("hanja.txt");
        std::fs::write(&src, SAMPLE).unwrap();
        assert!(is_hanja_txt(&src));
        // a line that doesn't parse, as hanja.txt has: left out
        std::fs::write(&src, format!("{SAMPLE}무등\u{92}:無等\u{92}:\n")).unwrap();
        let dst = temp("dst.zip");
        // 光, 樂, 諾, 學; not 学, which has no meaning, nor the words
        assert_eq!(convert(&src, &dst).unwrap(), 4);

        let mut z = YomitanZip::open(&dst).unwrap();
        let index = z.index().unwrap();
        assert_eq!(index.title, "Hanja");
        assert!(index.revision.starts_with("hanja.txt "));
        assert_eq!(index.source_language.as_deref(), Some("ko"));
        assert_eq!(index.target_language.as_deref(), Some("ko"));
        assert_eq!(
            index.attribution.as_deref(),
            Some("Copyright (c) 2005,2006 Choe Hwanjin\nAll rights reserved.")
        );
        let bank = z
            .banks()
            .into_iter()
            .find(|b| b.kind == BankKind::Kanji)
            .unwrap();
        let BankEntries::Kanji(entries) = z.read_bank(&bank).unwrap() else {
            panic!("not a kanji bank");
        };
        let got: Vec<(&str, &str, Vec<String>)> = entries
            .iter()
            .map(|e| {
                (
                    e.kanji.as_str(),
                    e.onyomi.as_str(),
                    serde_json::from_str::<Vec<String>>(&e.meanings).unwrap(),
                )
            })
            .collect();
        let got: Vec<(&str, &str, Vec<&str>)> = got
            .iter()
            .map(|(c, r, ms)| (*c, *r, ms.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            got,
            [
                ("光", "광", vec!["빛 광"]),
                // the main reading first; 낙, which the initial-sound rule makes, left out
                (
                    "樂",
                    "악 락 요",
                    vec![
                        "풍류 악",
                        "즐길 악",
                        "즐거울 락",
                        "즐길 락",
                        "좋아할 요",
                        "즐길 요"
                    ]
                ),
                // 락 beside the main 낙 is another reading: kept
                (
                    "諾",
                    "낙 락",
                    vec!["대답할 낙", "허락할 낙", "대답할 락", "허락할 락"]
                ),
                ("學", "학", vec!["배울 학"]),
            ]
        );

        // deterministic
        let again = temp("again.zip");
        convert(&src, &again).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), std::fs::read(&again).unwrap());
        drop(z);
        for p in [src, dst, again] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn other_files_are_not_hanja_txt() {
        let p = temp("other.txt");
        std::fs::write(&p, "１\tいち\t2\n").unwrap();
        assert!(!is_hanja_txt(&p));
        std::fs::write(&p, "{\"characters\": []}\n").unwrap();
        assert!(!is_hanja_txt(&p));
        std::fs::write(&p, "# only comments\n").unwrap();
        assert!(!is_hanja_txt(&p));
        let _ = std::fs::remove_file(p);
    }
}
