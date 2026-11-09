//! `dict-admin add`: zip -> R2, entries -> D1.

use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use yomitan_parser::{
    parser::{BankEntries, YomitanZip},
    pinyin::normalize_pinyin,
    tokenize::{Lang, TokenizerMapper},
    util::{NonEmpty, OptionNonEmpty, normalize_text},
};

use crate::{
    sql::{num, opt_num, opt_s, s},
    wrangler::Wrangler,
};

/// D1 rejects statements over 100 KB
const MAX_STATEMENT_BYTES: usize = 100_000;

pub struct AddOpts {
    pub zip: PathBuf,
    /// URL or file name, recorded in `dict.source`
    pub source: String,
    pub name: String,
    pub lang: Option<Lang>,
    pub lindera_dir: PathBuf,
    pub sql_dir: PathBuf,
    pub keep_sql: bool,
    pub chunk_size: usize,
    pub upload: bool,
}

/// R2 key of a dictionary's media pack
pub fn assets_key(name: &str) -> String {
    format!("dicts/{name}.assets")
}

/// R2 key of a dictionary's pack of oversized glossaries
pub fn glossary_key(name: &str) -> String {
    format!("dicts/{name}.glossary")
}

/// Glossaries whose INSERT would exceed D1's statement limit, appended to one file
/// that is uploaded to R2 as `glossary_key(name)`.
struct GlossaryPack {
    path: PathBuf,
    file: Option<BufWriter<File>>,
    offset: u64,
    count: usize,
}

impl GlossaryPack {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: None,
            offset: 0,
            count: 0,
        }
    }

    /// returns (offset, size)
    fn push(&mut self, glossary: &str) -> Result<(u64, u64)> {
        if self.file.is_none() {
            self.file = Some(BufWriter::new(File::create(&self.path)?));
        }
        let bytes = glossary.as_bytes();
        self.file.as_mut().unwrap().write_all(bytes)?;
        let at = (self.offset, bytes.len() as u64);
        self.offset += bytes.len() as u64;
        self.count += 1;
        Ok(at)
    }

    /// Flush; Some(path) if anything was written.
    fn finish(&mut self) -> Result<Option<&Path>> {
        match self.file.take() {
            Some(mut f) => {
                f.flush()?;
                Ok(Some(&self.path))
            }
            None => Ok(None),
        }
    }
}

pub struct PackedAsset {
    pub path: String,
    pub offset: u64,
    pub size: u64,
}

/// Concatenate every media file (decompressed) into one file; returns (path, offset, size).
fn pack_assets(zip: &mut YomitanZip, out_path: &Path) -> Result<Vec<PackedAsset>> {
    let names = zip.asset_names();
    let mut out = Vec::with_capacity(names.len());
    if names.is_empty() {
        return Ok(out);
    }
    let mut f = BufWriter::new(File::create(out_path)?);
    let mut offset = 0u64;
    for name in names {
        let bytes = zip.read_bytes(&name)?;
        f.write_all(&bytes)?;
        out.push(PackedAsset {
            path: name,
            offset,
            size: bytes.len() as u64,
        });
        offset += bytes.len() as u64;
    }
    f.flush()?;
    Ok(out)
}

pub fn parse_lang(s: &str) -> Option<Lang> {
    Lang::from(s).or_else(|| match s.split(['-', '_']).next()? {
        "ja" => Some(Lang::Ja),
        "zh" => Some(Lang::Zh),
        "ko" => Some(Lang::Ko),
        _ => None,
    })
}

struct ChunkWriter<'a> {
    wrangler: &'a Wrangler,
    dir: PathBuf,
    prefix: String,
    chunk_size: usize,
    keep: bool,
    idx: usize,
    count: usize,
    file: Option<BufWriter<File>>,
    path: PathBuf,
    pub skipped: usize,
}

impl<'a> ChunkWriter<'a> {
    fn new(
        wrangler: &'a Wrangler,
        dir: &Path,
        prefix: &str,
        chunk_size: usize,
        keep: bool,
    ) -> Self {
        Self {
            wrangler,
            dir: dir.to_path_buf(),
            prefix: prefix.to_string(),
            chunk_size,
            keep,
            idx: 0,
            count: 0,
            file: None,
            path: PathBuf::new(),
            skipped: 0,
        }
    }

    fn push(&mut self, stmt: &str) -> Result<()> {
        if stmt.len() > MAX_STATEMENT_BYTES {
            self.skipped += 1;
            eprintln!(
                "warning: skipping statement of {} bytes (D1 limit {}): {}…",
                stmt.len(),
                MAX_STATEMENT_BYTES,
                stmt.chars().take(120).collect::<String>()
            );
            return Ok(());
        }
        if self.file.is_none() {
            self.idx += 1;
            self.path = self
                .dir
                .join(format!("{}_{:05}.sql", self.prefix, self.idx));
            self.file = Some(BufWriter::new(File::create(&self.path)?));
        }
        let f = self.file.as_mut().unwrap();
        f.write_all(stmt.as_bytes())?;
        f.write_all(b";\n")?;
        self.count += 1;
        if self.count >= self.chunk_size {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        let Some(mut f) = self.file.take() else {
            return Ok(());
        };
        f.flush()?;
        drop(f);
        eprintln!(
            "  executing {} ({} statements)",
            self.path.display(),
            self.count
        );
        self.wrangler
            .execute_file(&self.path)
            .with_context(|| format!("executing {}", self.path.display()))?;
        if !self.keep {
            std::fs::remove_file(&self.path)?;
        }
        self.count = 0;
        Ok(())
    }
}

pub async fn add(w: &Wrangler, opts: AddOpts) -> Result<()> {
    let mut zip =
        YomitanZip::open(&opts.zip).with_context(|| format!("opening {}", opts.zip.display()))?;
    let index_str = zip.index_json_str().context("reading index.json")?;
    let index = zip.index()?;

    let source_language = opts
        .lang
        .or_else(|| index.source_language.as_deref().and_then(parse_lang));

    eprintln!(
        "dict {:?}: title={:?} revision={:?} lang={:?}",
        opts.name,
        index.title,
        index.revision,
        source_language.map(|l| l.to_string())
    );

    let existing = w.query_rows(&format!(
        "SELECT id, status FROM dict WHERE name = {}",
        s(&opts.name)
    ))?;
    if let Some(r) = existing.first() {
        bail!(
            "dict {:?} already exists (id={}, status={}); run `remove` first or pass --replace",
            opts.name,
            r["id"],
            r["status"]
        );
    }

    // media: one uncompressed pack per dict, served by ranged R2 reads. Nothing for text-only dicts.
    std::fs::create_dir_all(&opts.sql_dir)?;
    let pack_path = opts
        .sql_dir
        .join(format!("{}.assets", sanitize(&opts.name)));
    let assets = pack_assets(&mut zip, &pack_path)?;
    let (key, assets_size) = if assets.is_empty() {
        eprintln!("no media files");
        (None, None)
    } else {
        let key = assets_key(&opts.name);
        let size = std::fs::metadata(&pack_path)?.len();
        eprintln!("{} media files, {} bytes", assets.len(), size);
        if opts.upload {
            eprintln!("uploading media pack -> r2://{}/{}", w.bucket, key);
            w.r2_put(&key, &pack_path, "application/octet-stream")?;
        }
        if !opts.keep_sql {
            std::fs::remove_file(&pack_path)?;
        }
        (Some(key), Some(size))
    };

    let rows = w.query_rows(&format!(
        "INSERT INTO dict (name, title, revision, L1, L2, index_json, source, assets_key, assets_size, status)
         VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, 'importing') RETURNING id",
        s(&opts.name),
        s(&index.title),
        s(&index.revision),
        opt_s(source_language.map(|l| l.to_string()).as_deref()),
        opt_s(index.target_language.as_deref()),
        s(&index_str),
        s(&opts.source),
        opt_s(key.as_deref()),
        opt_num(assets_size.map(|n| n as f64))
    ))?;
    let dict_id = rows
        .first()
        .and_then(|r| r["id"].as_i64())
        .context("INSERT dict did not return id")?;
    eprintln!("dict_id = {dict_id}");

    let result = import_entries(w, &mut zip, dict_id, source_language, &assets, &opts).await;
    let (term_count, gloss_key) = match result {
        Ok(n) => n,
        Err(e) => {
            eprintln!(
                "import failed; dict {:?} left in status 'importing'. Clean up with: dict-admin remove {:?}",
                opts.name, opts.name
            );
            return Err(e);
        }
    };

    w.query(&format!(
        "UPDATE dict SET status = 'ready', term_count = {term_count}, glossary_key = {} WHERE id = {dict_id}",
        opt_s(gloss_key.as_deref())
    ))?;
    eprintln!("done: {:?} ready with {term_count} terms", opts.name);
    Ok(())
}

async fn import_entries(
    w: &Wrangler,
    zip: &mut YomitanZip,
    dict_id: i64,
    source_language: Option<Lang>,
    assets: &[PackedAsset],
    opts: &AddOpts,
) -> Result<(i64, Option<String>)> {
    crate::lindera::ensure(&opts.lindera_dir)?;
    let tok = TokenizerMapper::new(opts.lindera_dir.clone());

    let mut out = ChunkWriter::new(
        w,
        &opts.sql_dir,
        &format!("{}_{}", sanitize(&opts.name), dict_id),
        opts.chunk_size,
        opts.keep_sql,
    );

    for a in assets {
        out.push(&format!(
            "INSERT OR REPLACE INTO asset (dict_id, path, offset, size) VALUES ({dict_id}, {}, {}, {})",
            s(&a.path),
            a.offset,
            a.size,
        ))?;
    }

    let mut gloss = GlossaryPack::new(
        opts.sql_dir
            .join(format!("{}.glossary", sanitize(&opts.name))),
    );

    let banks = zip.banks();
    let total_banks = banks.len();
    let mut term_count: i64 = 0;

    for (i, bank) in banks.iter().enumerate() {
        let entries = zip.read_bank(bank)?;
        eprintln!(
            "[{}/{}] {} ({} entries)",
            i + 1,
            total_banks,
            bank.name,
            entries.len()
        );

        match entries {
            BankEntries::Tag(rs) => {
                for r in rs {
                    out.push(&format!(
                        "INSERT INTO tag (dict_id, name, category, sort_order, notes, score) VALUES ({dict_id}, {}, {}, {}, {}, {})",
                        s(&r.name),
                        s(&r.category),
                        num(r.sort_order),
                        s(&r.notes),
                        num(r.score)
                    ))?;
                }
            }
            BankEntries::Kanji(rs) => {
                for r in rs {
                    out.push(&format!(
                        "INSERT INTO kanji (dict_id, kanji, onyomi, kunyomi, tags, meanings, stats) VALUES ({dict_id}, {}, {}, {}, {}, {}, {})",
                        s(&r.kanji),
                        s(&r.onyomi),
                        s(&r.kunyomi),
                        s(&r.tags),
                        s(&r.meanings),
                        s(&r.stats)
                    ))?;
                }
            }
            BankEntries::KanjiMeta(rs) => {
                for r in rs {
                    let data = match &r.data {
                        yomitan_parser::parser::StringOrNumber::Number(n) => num(*n),
                        yomitan_parser::parser::StringOrNumber::String(v) => s(v),
                    };
                    out.push(&format!(
                        "INSERT INTO kanji_meta (dict_id, kanji, type, data) VALUES ({dict_id}, {}, {}, {data})",
                        s(&r.kanji),
                        s(&r.r#type),
                    ))?;
                }
            }
            BankEntries::TermMeta(rs) => {
                for r in rs {
                    out.push(&format!(
                        "INSERT INTO term_meta (dict_id, term, type, data) VALUES ({dict_id}, {}, {}, {})",
                        s(&r.term),
                        s(&r.r#type),
                        s(&r.data)
                    ))?;
                }
            }
            BankEntries::Term(rs) => {
                for r in rs {
                    let (upsert, search_id) = term_statements(&r, source_language, &tok).await?;
                    let mut insert =
                        term_insert(dict_id, &search_id, &r, Glossary::Inline(&r.glossary));
                    if insert.len() > MAX_STATEMENT_BYTES {
                        // too big for D1: glossary goes to the R2 pack
                        let (offset, size) = gloss.push(&r.glossary)?;
                        insert =
                            term_insert(dict_id, &search_id, &r, Glossary::Packed(offset, size));
                        if insert.len() > MAX_STATEMENT_BYTES {
                            out.skipped += 1;
                            eprintln!(
                                "warning: skipping term {:?}: {} bytes even without glossary",
                                r.term,
                                insert.len()
                            );
                            continue;
                        }
                    }
                    out.push(&upsert)?;
                    out.push(&insert)?;
                    term_count += 1;
                }
            }
        }
    }

    out.flush()?;
    if out.skipped > 0 {
        eprintln!("warning: {} statements skipped", out.skipped);
    }

    // upload before the dict is marked ready, so the worker never sees missing glossaries
    let count = gloss.count;
    let key = match gloss.finish()? {
        None => None,
        Some(path) => {
            let key = glossary_key(&opts.name);
            eprintln!(
                "{count} oversized glossaries, {} bytes",
                std::fs::metadata(path)?.len()
            );
            if opts.upload {
                eprintln!("uploading glossary pack -> r2://{}/{}", w.bucket, key);
                w.r2_put(&key, path, "application/json")?;
            }
            if !opts.keep_sql {
                std::fs::remove_file(path)?;
            }
            Some(key)
        }
    };

    Ok((term_count, key))
}

/// Mirrors the old local `YomitanSearch::import`, but with `term_link` folded into `term`.
/// Returns the term_search upsert and the subquery selecting its id.
async fn term_statements(
    r: &yomitan_parser::parser::TermEntry,
    source_language: Option<Lang>,
    tok: &TokenizerMapper,
) -> Result<(String, String)> {
    let p1_term = normalize_text(&r.term);
    let p2_term_ja = tok
        .join_tokens(&r.term, Lang::Ja, &source_language)
        .await?
        .unwrap_or_default();
    let p3_term_zh = tok
        .join_tokens(&r.term, Lang::Zh, &source_language)
        .await?
        .unwrap_or_default();
    let p4_term_ko = tok
        .join_tokens(&r.term, Lang::Ko, &source_language)
        .await?
        .unwrap_or_default();

    let reading = if source_language == Some(Lang::Zh) {
        normalize_pinyin(&r.reading).unwrap_or_else(|| r.reading.clone())
    } else {
        r.reading.clone()
    };
    let p5_reading = normalize_text(&reading)
        .non_empty_trimmed()
        .map(|s| format!(" {s} "))
        .unwrap_or_default();

    let p6_def_tags = r.def_tags.clone().non_empty_trimmed();
    let p7_rules = r.rules.clone().non_empty_trimmed();
    let p8_score = Some(r.score);
    let p9_tags = r.tags.clone().non_empty_trimmed();

    fn update_space_sep(c: &str) -> String {
        format!(
            r#""{c}" = CASE WHEN "{c}" IS excluded.{c} THEN "{c}" ELSE NULLIF(trim(printf('%s %s', "{c}", excluded.{c})), '') END"#
        )
    }

    let upsert = format!(
        "INSERT INTO term_search (term, term_ja, term_zh, term_ko, reading, def_tags, rules, score, tags) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT DO UPDATE SET {}, {}, {}, score = max(coalesce(score, excluded.score), coalesce(excluded.score, score))",
        s(&p1_term),
        s(&p2_term_ja),
        s(&p3_term_zh),
        s(&p4_term_ko),
        s(&p5_reading),
        opt_s(p6_def_tags.as_deref()),
        opt_s(p7_rules.as_deref()),
        opt_num(p8_score),
        opt_s(p9_tags.as_deref()),
        update_space_sep("def_tags"),
        update_space_sep("rules"),
        update_space_sep("tags"),
    );

    let b = |v: &str| if v.is_empty() { 0 } else { 1 };
    // matches the expressions of idx_u_term_search, so it is an index lookup
    let search_id = format!(
        "(SELECT id FROM term_search WHERE term = {} AND reading = {} AND (term_ja != '') = {} AND (term_zh != '') = {} AND (term_ko != '') = {})",
        s(&p1_term),
        s(&p5_reading),
        b(&p2_term_ja),
        b(&p3_term_zh),
        b(&p4_term_ko),
    );

    Ok((upsert, search_id))
}

enum Glossary<'a> {
    Inline(&'a str),
    /// (offset, size) in the R2 glossary pack
    Packed(u64, u64),
}

fn term_insert(
    dict_id: i64,
    search_id: &str,
    r: &yomitan_parser::parser::TermEntry,
    glossary: Glossary,
) -> String {
    let (glossary, offset, size) = match glossary {
        Glossary::Inline(g) => (s(g), "NULL".to_string(), "NULL".to_string()),
        Glossary::Packed(o, n) => ("NULL".to_string(), o.to_string(), n.to_string()),
    };
    format!(
        "INSERT INTO term (dict_id, search_id, term, reading, def_tags, rules, score, glossary, glossary_offset, glossary_size, sequence, tags) VALUES ({dict_id}, {search_id}, {}, {}, {}, {}, {}, {glossary}, {offset}, {size}, {}, {})",
        s(&r.term),
        s(&r.reading),
        opt_s(r.def_tags.as_deref()),
        s(&r.rules),
        num(r.score),
        r.sequence,
        s(&r.tags),
    )
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
