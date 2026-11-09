# cjk-dict

Hosted Yomitan dictionary search for [cjk-explainer](https://github.com/patarapolw/cjk-explainer), on Cloudflare:

- **D1** holds the search index (`term_search` + FTS5) and every dictionary entry (`term`, `term_meta`, `kanji`, `kanji_meta`, `tag`).
- **R2** holds the original dictionary zips. Images and other files referenced by structured content are served straight out of the zip with a ranged R2 read. Their byte offsets are recorded in D1 (`asset`) at import time.
- The **Worker** is read-only. Dictionaries are only added or removed with the **`dict-admin`** CLI.

```
yomitan-parser/   Rust crate: Yomitan zip reader (feature "zip"), lindera tokenizer, pinyin helpers
dict-admin/       Rust CLI: add / remove / list dictionaries (drives `wrangler` for D1 + R2)
worker/           Cloudflare Worker (TypeScript) + D1 migrations
```

`yomitan-parser` is also used by the cjk-explainer app (without the `zip` feature) to tokenize queries and expand pinyin on the client:

```toml
yomitan-parser = { git = "https://github.com/patarapolw/cjk-dict" }
```

## Setup

```bash
cd worker
npm install
npx wrangler login
npx wrangler d1 create cjk-dict        # paste database_id into wrangler.toml
npx wrangler r2 bucket create cjk-dict
npm run migrate                        # or: cargo run -p dict-admin -- migrate
npm run deploy
```

## Managing dictionaries

Run from the repo root (or pass `--worker-dir`). Add `--local` to target `wrangler dev` storage instead of Cloudflare.

```bash
cargo build --release -p dict-admin
A=target/release/dict-admin

$A list
$A add path/to/JMdict.zip --lang ja-JP
$A add https://github.com/MarvNC/cc-cedict-yomitan/releases/latest/download/CC-CEDICT.zip --lang zh-CN
$A add new/JMdict.zip --name JMdict --replace     # update to a new revision
$A remove JMdict
```

`add`:

1. Reads `index.json`. The name defaults to the zip file stem, and the language to `--lang`, then `sourceLanguage`.
2. Uploads the zip to `r2://<bucket>/dicts/<name>.zip` (`--no-upload` if it is already there).
3. Inserts a `dict` row with `status = 'importing'`. The Worker ignores such dicts.
4. Tokenizes every term with lindera (dictionaries are downloaded to `--lindera-dir` on first use). It then writes SQL in chunks of `--chunk-size` statements and runs each chunk with `wrangler d1 execute --file`.
5. Marks the dict `ready`.

If an import fails, the dict stays `importing`; run `remove <name>` to clean up.

`remove` deletes in id-range batches (`--batch`). For each shared `term_search` row it first recomputes `def_tags`/`rules`/`tags`/`score` from the remaining dictionaries, or deletes the row if no dictionary references it anymore. It then deletes the R2 zip.

Options can also be set by environment: `CJK_DICT_WORKER_DIR`, `CJK_DICT_DATABASE`, `CJK_DICT_BUCKET`, `CJK_DICT_LINDERA_DIR`, `WRANGLER` (default `npx wrangler`). Behind a TLS-intercepting proxy, set `SSL_CERT_FILE`.

Limits to keep in mind:

- D1 rejects single statements over 100 KB. Entries with a larger glossary are skipped with a warning.
- `wrangler r2 object put` uploads at most ~300 MB. For bigger zips, upload with an S3 client (e.g. `rclone`) to `dicts/<name>.zip` and pass `--no-upload`.
- A D1 database holds at most 10 GB.

## API

| Method | Path | |
| --- | --- | --- |
| `GET` | `/dicts` | ready dictionaries |
| `POST` | `/search` | see below |
| `GET` | `/dicts/:name/assets/<path>` | a file from inside the dictionary zip |
| `GET` | `/dicts/:name/zip` | the original zip (supports `Range`) |

```jsonc
// POST /search
{
  "type": null,          // null: prefix search on term | "exact" | "fts" (token search)
  "lang": "ja-JP",       // optional; restricts to terms tokenizable in that language
  "term": "日本",        // prefix / exact modes
  "tokens": { "ja": "日本" }, // fts mode: client-side tokenized term per language
  "readings": ["にほん"],  // optional; alternatives OR-ed (client expands pinyin)
  "limit": 10,           // max 50
  "offset": 0
}
// -> { "items": [{ ...term_search, "entries": [{ dict_id, dict_path, term_id, term, reading, glossary, ... }] }], "next": 10 | null }
```

## Local development

```bash
cd worker && npm run migrate:local && npx wrangler dev
# in another shell, from the repo root
cargo run -p dict-admin -- --local add some.zip
curl localhost:8787/search -d '{"term":"日本"}'
```
