//! `dict-admin remove`: delete a dictionary's rows from D1 (in batches) and its media pack from R2.

use anyhow::{Result, bail};

use crate::{import::assets_key, sql::s, wrangler::Wrangler};

pub fn remove(w: &Wrangler, name: &str, batch: i64, force: bool) -> Result<()> {
    let rows = w.query_rows(&format!(
        "SELECT id, assets_key FROM dict WHERE name = {}",
        s(name)
    ))?;

    let Some(row) = rows.first() else {
        if force {
            eprintln!("dict {name:?} not in D1; deleting R2 media pack only");
            return w.r2_delete(&assets_key(name));
        }
        bail!("dict {name:?} not found (use --force to delete the R2 media pack anyway)");
    };
    let dict_id = row["id"].as_i64().unwrap_or_default();
    let key = row["assets_key"].as_str().map(String::from);

    // hide from search immediately
    w.query(&format!(
        "UPDATE dict SET status = 'deleting' WHERE id = {dict_id}"
    ))?;

    // terms: fix up / drop the shared term_search rows they point to, then delete
    let (lo, hi) = id_range(w, "term", "id", dict_id)?;
    if let (Some(lo), Some(hi)) = (lo, hi) {
        let mut a = lo;
        while a <= hi {
            let b = a + batch - 1;
            eprintln!("  term ids {a}..={b} (of {lo}..={hi})");
            let in_batch = format!(
                "SELECT search_id FROM term WHERE dict_id = {dict_id} AND id BETWEEN {a} AND {b}"
            );
            let others = format!(
                "FROM term t WHERE t.search_id = term_search.id AND t.dict_id != {dict_id}"
            );
            let agg = |c: &str| {
                format!(
                    "{c} = (SELECT NULLIF(trim(replace(group_concat(DISTINCT t.{c}), ',', ' ')), '') {others})"
                )
            };
            w.query(&format!(
                "UPDATE term_search SET {}, {}, {}, score = (SELECT max(t.score) {others}) WHERE id IN ({in_batch}) AND EXISTS (SELECT 1 {others});
                 DELETE FROM term_search WHERE id IN ({in_batch}) AND NOT EXISTS (SELECT 1 {others});
                 DELETE FROM term WHERE dict_id = {dict_id} AND id BETWEEN {a} AND {b};",
                agg("def_tags"),
                agg("rules"),
                agg("tags"),
            ))?;
            a = b + 1;
        }
    }

    for table in ["term_meta", "kanji", "kanji_meta", "tag", "asset"] {
        let (lo, hi) = id_range(w, table, "rowid", dict_id)?;
        if let (Some(lo), Some(hi)) = (lo, hi) {
            let mut a = lo;
            while a <= hi {
                let b = a + batch - 1;
                eprintln!("  {table} rowids {a}..={b}");
                w.query(&format!(
                    "DELETE FROM {table} WHERE dict_id = {dict_id} AND rowid BETWEEN {a} AND {b}"
                ))?;
                a = b + 1;
            }
        }
    }

    w.query(&format!("DELETE FROM dict WHERE id = {dict_id}"))?;

    if let Some(key) = key {
        eprintln!("deleting r2://{}/{key}", w.bucket);
        if let Err(e) = w.r2_delete(&key) {
            eprintln!("warning: R2 delete failed (rows already removed): {e:#}");
        }
    }

    eprintln!("removed {name:?}");
    Ok(())
}

fn id_range(
    w: &Wrangler,
    table: &str,
    col: &str,
    dict_id: i64,
) -> Result<(Option<i64>, Option<i64>)> {
    let rows = w.query_rows(&format!(
        "SELECT min({col}) lo, max({col}) hi FROM {table} WHERE dict_id = {dict_id}"
    ))?;
    let r = rows.first();
    Ok((
        r.and_then(|r| r["lo"].as_i64()),
        r.and_then(|r| r["hi"].as_i64()),
    ))
}
