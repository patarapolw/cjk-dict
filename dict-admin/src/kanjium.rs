//! Kanjium's pitch accents as a Yomitan dictionary of pitch accents (term meta only).
//!
//! [Kanjium](https://github.com/mifunetoshiro/kanjium) has no Yomitan release, so `add`
//! converts its `accents.txt`: a line per word, `term<TAB>reading<TAB>positions`, e.g.
//! `１<TAB>ひと<TAB>0,2`. The reading is empty where the term is kana. A position is the
//! mora after which the pitch drops (0: it doesn't), and may follow the parts of speech it
//! is for: `(副)1,(形動)0`, `(名;形動)0`.
//!
//! Kanjium is CC BY-SA 4.0 and asks to be credited in apps that use it (see
//! [`ATTRIBUTION`]).
//!
//! @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-meta-bank-v3-schema.json

use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use yomitan_parser::parser::TERM_BANK_BYTES;
use zip::{CompressionMethod, DateTime, ZipWriter, write::SimpleFileOptions};

/// Where `defaults` downloads `accents.txt` from: the repo has no releases.
pub const URL: &str = "https://raw.githubusercontent.com/mifunetoshiro/kanjium/master/data/source_files/raw/accents.txt";

/// The credit Kanjium's README asks for, for its own additions to EDRDG's files.
pub const ATTRIBUTION: &str = "The pitch accent notation, verb particle data, phonetics, homonyms and other additions or modifications to EDICT, KANJIDIC or KRADFILE were provided by Uros O. through his free database (CC BY-SA 4.0): https://github.com/mifunetoshiro/kanjium";

/// Kanjium's parts of speech: (tag, notes)
const TAGS: &[(&str, &str)] = &[
    ("名", "noun"),
    ("代", "pronoun"),
    ("副", "adverb"),
    ("形動", "na-adjective"),
    ("感", "interjection"),
];

/// Whether `path` is Kanjium's `accents.txt`: tab-separated lines of a term, a reading and
/// pitch positions.
pub fn is_kanjium(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let mut head = vec![0u8; 4096];
    let Ok(n) = f.read(&mut head) else {
        return false;
    };
    head.truncate(n);
    // a whole line or more, in UTF-8 (the cut may split a character)
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
    let lines: Vec<&str> = lines.lines().filter(|l| !l.trim().is_empty()).collect();
    !lines.is_empty() && lines.iter().all(|l| parse_line(l).is_some())
}

/// A pitch accent: where the pitch drops, and the parts of speech it is for (empty: all)
#[derive(Debug, PartialEq)]
struct Pitch {
    position: u32,
    tags: Vec<String>,
}

/// `(term, reading, pitches)`; the reading is empty where the term is kana
fn parse_line(line: &str) -> Option<(&str, &str, Vec<Pitch>)> {
    let mut cols = line.trim_end_matches('\r').split('\t');
    let (term, reading, positions) = (cols.next()?, cols.next()?, cols.next()?);
    if cols.next().is_some() || term.is_empty() {
        return None;
    }
    let pitches = parse_positions(positions)?;
    Some((term, reading, pitches))
}

/// `(副)2,1,(形動)0` -> 2 and 1 for 副, 0 for 形動. The same position twice is one pitch,
/// with the tags of both.
fn parse_positions(s: &str) -> Option<Vec<Pitch>> {
    let mut out: Vec<Pitch> = vec![];
    let mut tags: Vec<String> = vec![];
    for part in s.split(',') {
        let num = match part.strip_prefix('(') {
            Some(rest) => {
                let (t, num) = rest.split_once(')')?;
                tags = t.split(';').map(str::to_string).collect();
                num
            }
            None => part,
        };
        let position: u32 = num.parse().ok()?;
        match out.iter_mut().find(|p| p.position == position) {
            Some(p) => {
                // a pitch for all parts of speech stays so
                if !p.tags.is_empty() {
                    if tags.is_empty() {
                        p.tags.clear();
                    } else {
                        for t in &tags {
                            if !p.tags.contains(t) {
                                p.tags.push(t.clone());
                            }
                        }
                    }
                }
            }
            None => out.push(Pitch {
                position,
                tags: tags.clone(),
            }),
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Convert Kanjium's `accents.txt` at `src` to a Yomitan dictionary of pitch accents at
/// `dst`. The output is deterministic. Returns the number of words.
pub fn convert(src: &Path, dst: &Path) -> Result<usize> {
    let bytes = std::fs::read(src)?;
    let text = std::str::from_utf8(&bytes).context("accents.txt is not UTF-8")?;

    // (term, reading) -> pitches, in file order; the same word on two lines is merged
    let mut words: Vec<(&str, &str, Vec<Pitch>)> = vec![];
    let mut at: std::collections::HashMap<(&str, &str), usize> = Default::default();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let (term, reading, pitches) =
            parse_line(line).with_context(|| format!("accents.txt line {}: {line:?}", i + 1))?;
        // Yomitan's pitch data always has a reading
        let reading = if reading.is_empty() { term } else { reading };
        match at.get(&(term, reading)) {
            Some(&w) => {
                for p in pitches {
                    if !words[w].2.iter().any(|q| q.position == p.position) {
                        words[w].2.push(p);
                    }
                }
            }
            None => {
                at.insert((term, reading), words.len());
                words.push((term, reading, pitches));
            }
        }
    }
    if words.is_empty() {
        bail!("accents.txt has no words");
    }

    let sha = Sha256::digest(&bytes);
    let index = json!({
        "title": "Kanjium",
        // accents.txt has no version: its hash tells revisions apart
        "revision": format!("accents.txt {:x}", &sha)[..20].to_string(),
        "format": 3,
        "sequenced": false,
        "author": "Uros O.",
        "url": "https://github.com/mifunetoshiro/kanjium",
        "description": "Pitch accents of Japanese words, from Kanjium's accents.txt, converted by cjk-dict's dict-admin.",
        "attribution": ATTRIBUTION,
        "sourceLanguage": "ja",
    });
    let tags: Vec<Value> = TAGS
        .iter()
        .map(|(name, notes)| json!([name, "partOfSpeech", 0, notes, 0]))
        .collect();

    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::DEFAULT);
    let mut w = ZipWriter::new(std::io::BufWriter::new(File::create(dst)?));
    w.start_file("index.json", opts)?;
    w.write_all(serde_json::to_string(&index)?.as_bytes())?;
    w.start_file("tag_bank_1.json", opts)?;
    w.write_all(serde_json::to_string(&tags)?.as_bytes())?;

    let mut n = 0;
    let mut chunk: Vec<u8> = vec![];
    let mut flush = |w: &mut ZipWriter<_>, chunk: &mut Vec<u8>| -> Result<()> {
        if chunk.is_empty() {
            return Ok(());
        }
        n += 1;
        w.start_file(format!("term_meta_bank_{n}.json"), opts)?;
        w.write_all(b"[")?;
        w.write_all(chunk)?;
        w.write_all(b"]")?;
        chunk.clear();
        Ok(())
    };
    for (term, reading, pitches) in &words {
        let pitches: Vec<Value> = pitches
            .iter()
            .map(|p| {
                if p.tags.is_empty() {
                    json!({ "position": p.position })
                } else {
                    json!({ "position": p.position, "tags": p.tags })
                }
            })
            .collect();
        let row = serde_json::to_vec(&json!([
            term,
            "pitch",
            { "reading": reading, "pitches": pitches }
        ]))?;
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
    Ok(words.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yomitan_parser::parser::{BankEntries, BankKind, YomitanZip};

    const SAMPLE: &str = "１\tいち\t2\n１\tひと\t0,2\nああ\t\t0\nちりちり\t\t0,(副)1,(形動)0\n生\tなま\t(名;形動)0,(副)1,0\nβカロテン\tベータカロテン\t3\nβカロテン\tベータカロテン\t4\n";

    fn pitch(position: u32, tags: &[&str]) -> Pitch {
        Pitch {
            position,
            tags: tags.iter().map(|t| t.to_string()).collect(),
        }
    }

    #[test]
    fn parses_positions_with_parts_of_speech() {
        assert_eq!(
            parse_positions("0,2").unwrap(),
            [pitch(0, &[]), pitch(2, &[])]
        );
        assert_eq!(
            parse_positions("(副)2,1,(形動)0").unwrap(),
            [pitch(2, &["副"]), pitch(1, &["副"]), pitch(0, &["形動"])]
        );
        // 0 for 名 and 形動, then for 副 too
        assert_eq!(
            parse_positions("(名;形動)0,(副)1,0").unwrap(),
            [pitch(0, &["名", "形動", "副"]), pitch(1, &["副"])]
        );
        // 0 for every part of speech, and 0 for 形動 among them: still for all
        assert_eq!(
            parse_positions("0,(副)1,(形動)0").unwrap(),
            [pitch(0, &[]), pitch(1, &["副"])]
        );
        assert!(parse_positions("").is_none());
        assert!(parse_positions("(名").is_none());
        assert!(parse_positions("x").is_none());
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dict-admin-kanjium-{}-{name}", std::process::id()))
    }

    #[test]
    fn converts_to_a_yomitan_pitch_dictionary() {
        let src = temp("accents.txt");
        std::fs::write(&src, SAMPLE).unwrap();
        assert!(is_kanjium(&src));
        let dst = temp("dst.zip");
        assert_eq!(convert(&src, &dst).unwrap(), 6);
        assert!(!is_kanjium(&dst));

        let mut z = YomitanZip::open(&dst).unwrap();
        let index = z.index().unwrap();
        assert_eq!(index.title, "Kanjium");
        assert!(index.revision.starts_with("accents.txt "));
        assert_eq!(index.source_language.as_deref(), Some("ja"));
        assert_eq!(index.attribution.as_deref(), Some(ATTRIBUTION));
        let banks = z.banks();
        assert!(!banks.iter().any(|b| b.kind == BankKind::Term));
        let bank = banks
            .into_iter()
            .find(|b| b.kind == BankKind::TermMeta)
            .unwrap();
        let BankEntries::TermMeta(entries) = z.read_bank(&bank).unwrap() else {
            panic!("not a term meta bank");
        };
        let data = |i: usize| -> Value { serde_json::from_str(&entries[i].data).unwrap() };
        assert_eq!(entries[0].term, "１");
        assert_eq!(entries[0].r#type, "pitch");
        assert_eq!(
            data(1),
            json!({"reading": "ひと", "pitches": [{"position": 0}, {"position": 2}]})
        );
        // a kana word is its own reading
        assert_eq!(
            data(2),
            json!({"reading": "ああ", "pitches": [{"position": 0}]})
        );
        assert_eq!(
            data(3),
            json!({"reading": "ちりちり", "pitches": [{"position": 0}, {"position": 1, "tags": ["副"]}]})
        );
        // the same word on two lines: one entry
        assert_eq!(entries.len(), 6);
        assert_eq!(
            data(5),
            json!({"reading": "ベータカロテン", "pitches": [{"position": 3}, {"position": 4}]})
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
    fn other_files_are_not_kanjium() {
        let p = temp("other.txt");
        std::fs::write(&p, "a\tb\n").unwrap();
        assert!(!is_kanjium(&p));
        std::fs::write(&p, "{\"characters\": []}\n").unwrap();
        assert!(!is_kanjium(&p));
        let _ = std::fs::remove_file(p);
    }
}
