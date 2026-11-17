//! KANJIDIC2 (EDRDG's kanji dictionary, part of the JMdict project) as a Yomitan kanji
//! dictionary, from [jmdict-simplified](https://github.com/scriptin/jmdict-simplified)'s
//! JSON of `kanjidic2.xml`.
//!
//! The library is a folder of Yomitan zips, so `add` converts the JSON into one: a kanji bank
//! entry per character, with its on and kun readings and its meanings, and everything else
//! KANJIDIC has as stats, which a tag bank names. Readings in other languages (pinyin,
//! Korean, Vietnamese) and name readings (nanori) are stats of the `reading` category, which
//! the app shows with the readings.
//!
//! @see https://www.edrdg.org/wiki/index.php/KANJIDIC_Project (CC BY-SA 4.0)
//! @see https://github.com/scriptin/jmdict-simplified/blob/master/node/packages/jmdict-simplified-types/index.ts

use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use yomitan_parser::parser::TERM_BANK_BYTES;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter, write::SimpleFileOptions};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Kanjidic2 {
    version: String,
    #[serde(default)]
    languages: Vec<String>,
    dict_date: String,
    #[serde(default)]
    database_version: String,
    characters: Vec<Character>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Character {
    literal: String,
    #[serde(default)]
    radicals: Vec<TypeValue<u32>>,
    misc: Misc,
    #[serde(default)]
    dictionary_references: Vec<DictionaryReference>,
    #[serde(default)]
    query_codes: Vec<TypeValue<String>>,
    reading_meaning: Option<ReadingMeaning>,
}

#[derive(Deserialize)]
struct TypeValue<T> {
    r#type: String,
    value: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Misc {
    grade: Option<u32>,
    #[serde(default)]
    stroke_counts: Vec<u32>,
    frequency: Option<u32>,
    #[serde(default)]
    radical_names: Vec<String>,
    jlpt_level: Option<u32>,
}

#[derive(Deserialize)]
struct DictionaryReference {
    r#type: String,
    morohashi: Option<Morohashi>,
    value: String,
}

#[derive(Deserialize)]
struct Morohashi {
    volume: u32,
    page: u32,
}

#[derive(Deserialize)]
struct ReadingMeaning {
    #[serde(default)]
    groups: Vec<Group>,
    #[serde(default)]
    nanori: Vec<String>,
}

#[derive(Deserialize)]
struct Group {
    #[serde(default)]
    readings: Vec<TypeValue<String>>,
    #[serde(default)]
    meanings: Vec<Meaning>,
}

#[derive(Deserialize)]
struct Meaning {
    lang: String,
    value: String,
}

/// The stats and tags a converted entry can have: (name, category, sort order, notes).
/// Readings in other languages first, then what the character is, then lookup codes, then
/// where it is in other dictionaries.
const TAGS: &[(&str, &str, i32, &str)] = &[
    ("pinyin", "reading", 0, "Pinyin"),
    ("korean_h", "reading", 1, "Korean"),
    ("korean_r", "reading", 2, "Korean (romanized)"),
    ("vietnam", "reading", 3, "Vietnamese"),
    ("nanori", "reading", 4, "Name readings (nanori)"),
    (
        "kyouiku",
        "frequent",
        0,
        "Kyōiku kanji, taught in elementary school",
    ),
    ("jouyou", "frequent", 0, "Jōyō kanji, for general use"),
    ("jinmeiyou", "frequent", 0, "Jinmeiyō kanji, for names"),
    ("strokes", "misc", 10, "Stroke count"),
    ("grade", "misc", 11, "Grade"),
    (
        "freq",
        "misc",
        12,
        "Frequency rank (of 2,500 in newspapers)",
    ),
    ("jlpt", "misc", 13, "JLPT level (former, 1-4)"),
    ("radical", "misc", 14, "Radical (Kangxi)"),
    ("radical_name", "misc", 15, "Radical name"),
    ("nelson_rad", "misc", 16, "Radical (Nelson)"),
    ("skip", "code", 20, "SKIP code"),
    ("sh_desc", "code", 21, "Spahn & Hadamitzky descriptor"),
    ("four_corner", "code", 22, "Four-corner code"),
    ("deroo", "code", 23, "De Roo code"),
    ("misclass", "code", 24, "Misclassification code"),
    (
        "moro",
        "index",
        30,
        "Morohashi Dai Kan-Wa Jiten (index, volume.page)",
    ),
    (
        "nelson_c",
        "index",
        31,
        "Modern Reader's Japanese-English Character Dictionary",
    ),
    (
        "nelson_n",
        "index",
        32,
        "The New Nelson Japanese-English Character Dictionary",
    ),
    (
        "halpern_njecd",
        "index",
        33,
        "New Japanese-English Character Dictionary",
    ),
    ("halpern_kkd", "index", 34, "Kodansha Kanji Dictionary"),
    ("halpern_kkld", "index", 35, "Kanji Learner's Dictionary"),
    (
        "halpern_kkld_2ed",
        "index",
        36,
        "Kanji Learner's Dictionary, 2nd edition",
    ),
    ("heisig", "index", 37, "Remembering the Kanji"),
    ("heisig6", "index", 38, "Remembering the Kanji, 6th edition"),
    ("gakken", "index", 39, "A New Dictionary of Kanji Usage"),
    ("oneill_names", "index", 40, "Japanese Names"),
    ("oneill_kk", "index", 41, "Essential Kanji"),
    (
        "henshall",
        "index",
        42,
        "A Guide to Remembering Japanese Characters",
    ),
    (
        "henshall3",
        "index",
        43,
        "A Guide to Reading and Writing Japanese, 3rd edition",
    ),
    ("sh_kk", "index", 44, "Kanji and Kana"),
    ("sh_kk2", "index", 45, "Kanji and Kana, 2011 edition"),
    (
        "sakade",
        "index",
        46,
        "A Guide to Reading and Writing Japanese",
    ),
    ("jf_cards", "index", 47, "Japanese Kanji Flashcards"),
    ("tutt_cards", "index", 48, "Tuttle Kanji Cards"),
    (
        "crowley",
        "index",
        49,
        "The Kanji Way to Japanese Language Power",
    ),
    ("kanji_in_context", "index", 50, "Kanji in Context"),
    ("busy_people", "index", 51, "Japanese for Busy People"),
    (
        "kodansha_compact",
        "index",
        52,
        "Kodansha Compact Kanji Guide",
    ),
    ("maniette", "index", 53, "Les Kanjis dans la tête"),
];

/// Whether `path` is jmdict-simplified's KANJIDIC2 JSON: the `.json` itself, or a zip of it.
pub fn is_kanjidic(path: &Path) -> bool {
    read_json(path).is_ok_and(|s| s.is_some())
}

/// The JSON text, if `path` is KANJIDIC2 JSON or a zip of it
fn read_json(path: &Path) -> Result<Option<String>> {
    let mut f = File::open(path)?;
    let mut magic = [0u8; 2];
    let n = f.read(&mut magic)?;
    if n == 2 && &magic == b"PK" {
        let mut z = ZipArchive::new(File::open(path)?)?;
        let names: Vec<String> = z.file_names().map(String::from).collect();
        if names.iter().any(|n| n == "index.json") {
            return Ok(None); // a Yomitan dictionary
        }
        let Some(name) = names
            .iter()
            .find(|n| n.starts_with("kanjidic2") && n.ends_with(".json"))
        else {
            return Ok(None);
        };
        let mut s = String::new();
        z.by_name(name)?.read_to_string(&mut s)?;
        return Ok(Some(s));
    }
    let mut s = String::new();
    File::open(path)?.read_to_string(&mut s)?;
    // the metadata before the characters
    let head: String = s.chars().take(4096).collect();
    Ok((head.trim_start().starts_with('{') && head.contains("\"characters\"")).then_some(s))
}

/// Convert KANJIDIC2 JSON (or a zip of it) at `src` to a Yomitan kanji dictionary at `dst`.
/// The output is deterministic, so converting the same release again gives the same zip.
/// Returns the number of characters.
pub fn convert(src: &Path, dst: &Path) -> Result<usize> {
    let text = read_json(src)?.context("not KANJIDIC2 JSON from jmdict-simplified")?;
    let dict: Kanjidic2 =
        serde_json::from_str(&text).context("reading KANJIDIC2 JSON (jmdict-simplified)")?;
    drop(text);
    if dict.characters.is_empty() {
        bail!("KANJIDIC2 JSON has no characters");
    }
    let lang = if dict.languages.iter().any(|l| l == "en") || dict.languages.is_empty() {
        "en".to_string()
    } else {
        dict.languages[0].clone()
    };

    let index = json!({
        "title": "KANJIDIC",
        "revision": format!("{} ({})", dict.dict_date, dict.database_version),
        "format": 3,
        "sequenced": false,
        "author": "Electronic Dictionary Research and Development Group",
        "url": "https://www.edrdg.org/wiki/index.php/KANJIDIC_Project",
        "description": format!(
            "KANJIDIC2 of {}, converted from jmdict-simplified {} by cjk-dict's dict-admin.",
            dict.dict_date, dict.version
        ),
        "attribution": "This publication has included material from the KANJIDIC2 dictionary file, the property of the Electronic Dictionary Research and Development Group, used in conformance with the Group's licence (CC BY-SA 4.0): https://www.edrdg.org/edrdg/licence.html",
        "sourceLanguage": "ja",
        "targetLanguage": lang,
    });
    let tags: Vec<Value> = TAGS
        .iter()
        .map(|(name, category, order, notes)| json!([name, category, order, notes, 0]))
        .collect();

    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::DEFAULT);
    let mut w = ZipWriter::new(std::io::BufWriter::new(File::create(dst)?));
    w.start_file("index.json", opts)?;
    w.write_all(serde_json::to_string(&index)?.as_bytes())?;
    w.start_file("tag_bank_1.json", opts)?;
    w.write_all(serde_json::to_string(&tags)?.as_bytes())?;

    // kanji banks of about the size the app reads term banks in
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
    for c in &dict.characters {
        let row = serde_json::to_vec(&entry(c, &lang))?;
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
    Ok(dict.characters.len())
}

/// `[character, onyomi, kunyomi, tags, meanings, stats]`
fn entry(c: &Character, lang: &str) -> Value {
    let mut on = vec![];
    let mut kun = vec![];
    let mut other: Vec<(&str, Vec<&str>)> = vec![];
    let mut meanings = vec![];
    if let Some(rm) = &c.reading_meaning {
        for g in &rm.groups {
            for r in &g.readings {
                match r.r#type.as_str() {
                    "ja_on" => on.push(r.value.as_str()),
                    "ja_kun" => kun.push(r.value.as_str()),
                    t => match other.iter_mut().find(|(k, _)| *k == t) {
                        Some((_, v)) => v.push(&r.value),
                        None => other.push((t, vec![&r.value])),
                    },
                }
            }
            meanings.extend(
                g.meanings
                    .iter()
                    .filter(|m| m.lang == lang)
                    .map(|m| m.value.as_str()),
            );
        }
        if !rm.nanori.is_empty() {
            other.push(("nanori", rm.nanori.iter().map(String::as_str).collect()));
        }
    }

    let mut stats = Map::new();
    for (k, v) in &other {
        if *k == "pinyin" {
            let marked: Vec<String> = v.iter().map(|p| tone_marks(p)).collect();
            stat(&mut stats, k, marked.join(", "));
            continue;
        }
        // Korean readings are separated so in Yomitan's KANJIDIC too
        stat(&mut stats, k, v.join(", "));
    }
    if let Some(&s) = c.misc.stroke_counts.first() {
        stat(&mut stats, "strokes", s.to_string());
    }
    if let Some(g) = c.misc.grade {
        stat(&mut stats, "grade", grade_name(g));
    }
    if let Some(f) = c.misc.frequency {
        stat(&mut stats, "freq", f.to_string());
    }
    if let Some(j) = c.misc.jlpt_level {
        stat(&mut stats, "jlpt", j.to_string());
    }
    for r in &c.radicals {
        match r.r#type.as_str() {
            "classical" => stat(&mut stats, "radical", radical(r.value)),
            "nelson_c" => stat(&mut stats, "nelson_rad", r.value.to_string()),
            _ => {}
        }
    }
    stat(&mut stats, "radical_name", c.misc.radical_names.join(", "));
    for q in &c.query_codes {
        stat(&mut stats, &q.r#type, q.value.clone());
    }
    for d in &c.dictionary_references {
        let v = match &d.morohashi {
            Some(m) => format!("{} ({}.{})", d.value, m.volume, m.page),
            None => d.value.clone(),
        };
        stat(&mut stats, &d.r#type, v);
    }

    let tags = match c.misc.grade {
        Some(1..=6) => "kyouiku jouyou",
        Some(8) => "jouyou",
        Some(9 | 10) => "jinmeiyou",
        _ => "",
    };
    json!([
        c.literal,
        on.join(" "),
        kun.join(" "),
        tags,
        meanings,
        stats
    ])
}

/// Set a stat, after the values it has (a character can have several SKIP codes)
fn stat(stats: &mut Map<String, Value>, k: &str, v: String) {
    if v.is_empty() {
        return;
    }
    let v = match stats.get(k) {
        Some(Value::String(old)) => format!("{old}, {v}"),
        _ => v,
    };
    stats.insert(k.to_string(), Value::String(v));
}

fn grade_name(g: u32) -> String {
    match g {
        1..=6 => format!("{g} (elementary school)"),
        8 => "secondary school".into(),
        9 => "for names".into(),
        10 => "for names (a variant of a jōyō kanji)".into(),
        g => g.to_string(),
    }
}

/// KANJIDIC's numbered pinyin with tone marks: `yu3` → `yǔ`, `lu:e4` → `lüè`, `ma5` → `ma`.
/// The mark goes on a or e, on the o of ou, else on the last vowel.
fn tone_marks(p: &str) -> String {
    let s = p.replace("u:", "ü").replace('v', "ü");
    let Some(tone) = s.chars().last().and_then(|c| c.to_digit(10)) else {
        return s;
    };
    let base = &s[..s.len() - 1];
    if !(1..=4).contains(&tone) {
        return base.to_string();
    }
    const MARKS: [(char, [char; 4]); 6] = [
        ('a', ['ā', 'á', 'ǎ', 'à']),
        ('e', ['ē', 'é', 'ě', 'è']),
        ('i', ['ī', 'í', 'ǐ', 'ì']),
        ('o', ['ō', 'ó', 'ǒ', 'ò']),
        ('u', ['ū', 'ú', 'ǔ', 'ù']),
        ('ü', ['ǖ', 'ǘ', 'ǚ', 'ǜ']),
    ];
    let chars: Vec<char> = base.chars().collect();
    let at = chars
        .iter()
        .position(|&c| c == 'a' || c == 'e')
        .or_else(|| {
            base.find("ou")
                .map(|_| chars.iter().position(|&c| c == 'o').unwrap())
        })
        .or_else(|| chars.iter().rposition(|c| "iouü".contains(*c)));
    let Some(at) = at else {
        return p.to_string();
    };
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| match MARKS.iter().find(|(v, _)| *v == c) {
            Some((_, m)) if i == at => m[tone as usize - 1],
            _ => c,
        })
        .collect()
}

/// `149 ⾔`: the Kangxi radical's number and character
fn radical(n: u32) -> String {
    match char::from_u32(0x2F00 + n - 1) {
        Some(c) if (1..=214).contains(&n) => format!("{n} {c}"),
        _ => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yomitan_parser::parser::{BankEntries, BankKind, YomitanZip};

    const SAMPLE: &str = r#"{
      "version": "3.6.2", "languages": ["en"], "dictDate": "2026-11-16",
      "fileVersion": 4, "databaseVersion": "2026-320",
      "characters": [
        {"literal":"語","codepoints":[{"type":"ucs","value":"8a9e"}],
         "radicals":[{"type":"classical","value":149}],
         "misc":{"grade":2,"strokeCounts":[14],"variants":[],"frequency":301,"radicalNames":[],"jlptLevel":4},
         "dictionaryReferences":[{"type":"moro","morohashi":{"volume":10,"page":474},"value":"35533"},
                                 {"type":"heisig6","morohashi":null,"value":"371"}],
         "queryCodes":[{"type":"skip","skipMisclassification":null,"value":"1-7-7"}],
         "readingMeaning":{"groups":[{"readings":[
             {"type":"pinyin","onType":null,"status":null,"value":"yu3"},
             {"type":"pinyin","onType":null,"status":null,"value":"yu4"},
             {"type":"korean_h","onType":null,"status":null,"value":"어"},
             {"type":"ja_on","onType":null,"status":null,"value":"ゴ"},
             {"type":"ja_kun","onType":null,"status":null,"value":"かた.る"},
             {"type":"ja_kun","onType":null,"status":null,"value":"かた.らう"}],
           "meanings":[{"lang":"en","value":"word"},{"lang":"en","value":"speech"},{"lang":"fr","value":"mot"}]}],
           "nanori":[]}},
        {"literal":"𠀋","codepoints":[],"radicals":[],
         "misc":{"grade":null,"strokeCounts":[6],"variants":[],"frequency":null,"radicalNames":[],"jlptLevel":null},
         "dictionaryReferences":[],"queryCodes":[],"readingMeaning":null}
      ]
    }"#;

    #[test]
    fn pinyin_gets_tone_marks() {
        for (numbered, marked) in [
            ("yu3", "yǔ"),
            ("zhong1", "zhōng"),
            ("guo2", "guó"),
            ("lu:e4", "lüè"),
            ("nu:3", "nǚ"),
            ("dou4", "dòu"),
            ("gui4", "guì"),
            ("liu2", "liú"),
            ("ma5", "ma"),
            ("er2", "ér"),
        ] {
            assert_eq!(tone_marks(numbered), marked, "{numbered}");
        }
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dict-admin-kanjidic-{}-{name}", std::process::id()))
    }

    #[test]
    fn converts_to_a_yomitan_kanji_dictionary() {
        let src = temp("src.zip");
        {
            let mut w = ZipWriter::new(File::create(&src).unwrap());
            w.start_file("kanjidic2-en-3.6.2.json", SimpleFileOptions::default())
                .unwrap();
            w.write_all(SAMPLE.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        assert!(is_kanjidic(&src));
        let dst = temp("dst.zip");
        assert_eq!(convert(&src, &dst).unwrap(), 2);
        assert!(!is_kanjidic(&dst));

        let mut z = YomitanZip::open(&dst).unwrap();
        let index = z.index().unwrap();
        assert_eq!(index.title, "KANJIDIC");
        assert_eq!(index.revision, "2026-11-16 (2026-320)");
        assert_eq!(index.source_language.as_deref(), Some("ja"));
        let bank = z
            .banks()
            .into_iter()
            .find(|b| b.kind == BankKind::Kanji)
            .unwrap();
        let BankEntries::Kanji(entries) = z.read_bank(&bank).unwrap() else {
            panic!("not a kanji bank");
        };
        let go = &entries[0];
        assert_eq!(go.kanji, "語");
        assert_eq!(go.onyomi, "ゴ");
        assert_eq!(go.kunyomi, "かた.る かた.らう");
        assert_eq!(go.tags, "kyouiku jouyou");
        assert_eq!(go.meanings, r#"["word","speech"]"#);
        let stats: Map<String, Value> = serde_json::from_str(&go.stats).unwrap();
        assert_eq!(stats["pinyin"], "yǔ, yù");
        assert_eq!(stats["korean_h"], "어");
        assert_eq!(stats["strokes"], "14");
        assert_eq!(stats["radical"], "149 ⾔");
        assert_eq!(stats["moro"], "35533 (10.474)");
        assert_eq!(stats["skip"], "1-7-7");
        // every stat has a tag that names it
        for k in stats.keys() {
            assert!(TAGS.iter().any(|t| t.0 == k), "{k}");
        }
        assert_eq!(entries[1].kanji, "𠀋");
        assert_eq!(entries[1].meanings, "[]");

        // deterministic: the same release converts to the same zip
        let again = temp("again.zip");
        convert(&src, &again).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), std::fs::read(&again).unwrap());

        // the JSON itself works too
        let json = temp("src.json");
        std::fs::write(&json, SAMPLE).unwrap();
        assert!(is_kanjidic(&json));
        drop(z);
        for p in [src, dst, again, json] {
            let _ = std::fs::remove_file(p);
        }
    }
}
