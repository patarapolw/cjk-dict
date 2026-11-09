# cjk-dict

Yomitan dictionary search for Chinese, Japanese and Korean: a Tauri app backed by a dictionary server on Cloudflare.

- **D1** holds the search index (`term_search` + FTS5) and every dictionary entry (`term`, `term_meta`, `kanji`, `kanji_meta`, `tag`).
- **R2** holds what doesn't fit in D1, as at most two objects per dictionary. Byte ranges are recorded in D1, and the Worker reads each item with a ranged read. Most text dictionaries use no R2 at all, and zips are not stored.
  - `dicts/<name>.assets`: images and other files referenced by structured content, packed uncompressed (ranges in `asset`).
  - `dicts/<name>.glossary`: glossaries too large for a D1 statement (over 100 KB). These rows have `term.glossary` NULL and the range in `glossary_offset`/`glossary_size`. The Worker fills them in, so clients see no difference.
- The **Worker** is read-only. Dictionaries are only added or removed with the **`dict-admin`** CLI.
- The **app** (Tauri + Vue, desktop and Android) searches through the Worker. It downloads only the lindera tokenizer dictionaries (~300 MB, in the background on first launch), to tokenize "~" queries on the device.

```
yomitan-parser/   Rust crate: Yomitan zip reader (feature "zip"), lindera tokenizer, pinyin helpers
dict-admin/       Rust CLI: add / remove / list dictionaries (drives `wrangler` for D1 + R2)
worker/           Cloudflare Worker (TypeScript) + D1 migrations
app/              Tauri app (Vue 3 + PrimeVue); app/src-tauri is its own Cargo workspace
```

The app tokenizes "~" search queries with `yomitan-parser` (path dependency, without the `zip` feature), the same code `dict-admin` uses at import.

## Setup

```bash
cd worker
pnpm install
pnpm wrangler login
pnpm wrangler d1 create cjk-dict        # paste database_id into wrangler.toml
pnpm wrangler r2 bucket create cjk-dict
pnpm migrate                           # or: cargo run -p dict-admin -- migrate
pnpm run deploy
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
2. If the dictionary has media, packs it and uploads the pack to `r2://<bucket>/dicts/<name>.assets` (`--no-upload` if it is already there).
3. Inserts a `dict` row with `status = 'importing'`. The Worker ignores such dicts.
4. Tokenizes every term with lindera (dictionaries are downloaded to `--lindera-dir` on first use). It then writes SQL in chunks of `--chunk-size` statements and runs each chunk with `wrangler d1 execute --file`.
5. Writes any glossary too large for D1 to the glossary pack and uploads it.
6. Marks the dict `ready`.

If an import fails, the dict stays `importing`; run `remove <name>` to clean up.

`remove` deletes in id-range batches (`--batch`). For each shared `term_search` row it first recomputes `def_tags`/`rules`/`tags`/`score` from the remaining dictionaries, or deletes the row if no dictionary references it anymore. It then deletes the media and glossary packs, if any.

Options can also be set by environment: `CJK_DICT_WORKER_DIR`, `CJK_DICT_DATABASE`, `CJK_DICT_BUCKET`, `CJK_DICT_LINDERA_DIR`, `WRANGLER` (default `pnpm exec wrangler`, run in `--worker-dir`). Behind a TLS-intercepting proxy, set `SSL_CERT_FILE`.

Limits to keep in mind:

- D1 rejects single statements over 100 KB. Larger glossaries go to the R2 glossary pack. An entry is skipped with a warning only if it is still too large without its glossary, which would take a term over 100 KB.
- `wrangler r2 object put` uploads at most ~300 MB. For a bigger media pack, run once with `--keep-sql` and `--no-upload`, then upload `tmp/sql/<name>.assets` with an S3 client (e.g. `rclone`) to `dicts/<name>.assets`.
- A D1 database holds at most 10 GB.

## API

| Method | Path | |
| --- | --- | --- |
| `GET` | `/dicts` | ready dictionaries |
| `POST` | `/search` | see below |
| `GET` | `/dicts/:name/assets/<path>` | a media file; usable directly as `<img src>` |

```jsonc
// POST /search
{
  "type": null,          // null: prefix search on term | "exact" | "fts" (token search)
  "lang": "ja-JP",       // optional; restricts to terms tokenizable in that language
  "term": "日本",        // prefix / exact modes
  "tokens": { "ja": "日本" }, // fts mode: client-side tokenized term per language
  "reading": "pinyin",   // optional; with lang zh-CN or empty, pinyin is expanded to every tone combination
  "limit": 10,           // max 50
  "offset": 0
}
// -> { "items": [{ ...term_search, "entries": [{ dict_id, dict_path, term_id, term, reading, glossary, ... }] }], "next": 10 | null }
```

Pinyin expansion lives in `worker/src/pinyin.ts`, a port of `yomitan-parser/src/pinyin.rs`. The Rust version normalizes readings at import and the TS version expands queries, so keep them in sync. `pnpm test` in `worker/` runs the same cases as the Rust tests.

## App

```bash
cd app
ppnpm install
pnpm start                        # tauri dev (desktop)
pnpm tauri build                  # release bundle
pnpm tauri android init && pnpm android   # first time on Android
```

Set the Worker URL under **Settings**, or at build time with `VITE_DICT_API_URL` (e.g. in `app/.env`). PrimeVue shows an "Invalid PrimeUI License" badge unless `VITE_PRIMEUI_LICENSE_KEY` is set too (same as cjk-explainer).

- The CSP allows inline styles (`style-src 'unsafe-inline'`), which PrimeVue's runtime theme needs.
- The HTTP capability (`app/src-tauri/capabilities/default.json`) and the CSP `img-src` (`app/src-tauri/tauri.conf.json`) allow `https://*.workers.dev` and `http://localhost:8787` (`wrangler dev`). For a custom domain, add it to both.
- Search modes: prefix (default), `=` exact, and `~` token search. `~` needs the tokenizer; until it has downloaded, `~` is disabled and falls back to prefix search.
- Optional: search by reading (pinyin is expanded to all tones by the Worker), auto-convert romaji to kana, and a clipboard monitor that searches whatever CJK text you copy.

## Local development

```bash
cd worker && pnpm migrate:local && pnpm wrangler dev
# in another shell, from the repo root
cargo run -p dict-admin -- --local add some.zip
curl localhost:8787/search -d '{"term":"日本"}'
```
