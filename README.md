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

`app/` and `worker/` form a pnpm workspace: run `pnpm install` once at the repo root (one `pnpm-lock.yaml`). From the root, `pnpm app <script>` and `pnpm worker <script>` run a package's scripts, and `pnpm typecheck` / `pnpm test` run them in both. `pnpm format` runs Prettier (a root devDependency, also used by VS Code's format on save).

The app tokenizes "~" search queries with `yomitan-parser` (path dependency, without the `zip` feature), the same code `dict-admin` uses at import.

## Setup

All commands in this README run from the repo root and work the same in bash and PowerShell.

```bash
pnpm install
pnpm worker wrangler login
pnpm worker wrangler d1 create cjk-dict    # paste database_id into worker/wrangler.toml
pnpm worker wrangler r2 bucket create cjk-dict
pnpm admin migrate
pnpm worker deploy
```

## Managing dictionaries

`pnpm admin <command>` builds and runs `dict-admin` (`cargo run --release`) against Cloudflare. `pnpm admin:local <command>` targets `wrangler dev` storage instead. Arguments pass through as-is.

```bash
pnpm admin list
pnpm admin add path/to/JMdict.zip --lang ja-JP
pnpm admin add https://github.com/MarvNC/cc-cedict-yomitan/releases/latest/download/CC-CEDICT.zip --lang zh-CN
pnpm admin add new/JMdict.zip --name JMdict --replace     # update to a new revision
pnpm admin remove JMdict

pnpm admin defaults --list         # default dictionaries and their latest download URLs
pnpm admin defaults                # import all that aren't imported yet
pnpm admin defaults --only CC-CEDICT --only wty-zh-en
pnpm admin defaults --only CC-CEDICT --replace           # update to the latest release

pnpm admin --help                  # all options; `pnpm admin add --help` etc. per command
```

Windows paths work too, e.g. `pnpm admin add C:\Downloads\JMdict.zip --lang ja-JP`.

The defaults (the list the app used to download itself) are Pixiv, `ja`/`zh`/`ko.Wikipedia` (MarvNC), CC-CEDICT, KO-EN.KRDICT, and `wty-ja-en`/`wty-ko-en`/`wty-zh-en` (Wiktionary). GitHub-hosted ones resolve to the asset of the repo's latest release. Set `GITHUB_TOKEN` if you hit the API's unauthenticated rate limit. The Wikipedia dictionaries are large; mind the D1 size limit below.

`add`:

1. Reads `index.json`. The name defaults to the zip file stem, and the language to `--lang`, then `sourceLanguage`.
2. If the dictionary has media, packs it and uploads the pack to `r2://<bucket>/dicts/<name>.assets` (`--no-upload` if it is already there).
3. Inserts a `dict` row with `status = 'importing'`. The Worker ignores such dicts.
4. Tokenizes every term with lindera (dictionaries are downloaded to `--lindera-dir` on first use). It then writes SQL in chunks of `--chunk-size` statements and runs each chunk with `wrangler d1 execute --file`.
5. Writes any glossary too large for D1 to the glossary pack and uploads it.
6. Marks the dict `ready`.

If an import fails, the dict stays `importing`; run `remove <name>` to clean up.

`remove` deletes in id-range batches (`--batch`). For each shared `term_search` row it first recomputes `def_tags`/`rules`/`tags`/`score` from the remaining dictionaries, or deletes the row if no dictionary references it anymore. It then deletes the media and glossary packs, if any.

Options can also be set by environment: `CJK_DICT_WORKER_DIR`, `CJK_DICT_DATABASE`, `CJK_DICT_BUCKET`, `CJK_DICT_LINDERA_DIR`, `WRANGLER` (default: the workspace's wrangler, run as `node worker/node_modules/wrangler/bin/wrangler.js` with no shell, so SQL arguments are passed intact on Windows too). Behind a TLS-intercepting proxy, set `SSL_CERT_FILE`.

Limits to keep in mind:

- D1 rejects single statements over 100 KB. Larger glossaries go to the R2 glossary pack. An entry is skipped with a warning only if it is still too large without its glossary, which would take a term over 100 KB.
- `wrangler r2 object put` uploads at most ~300 MB. For a bigger media pack, run once with `--keep-sql` and `--no-upload`, then upload `tmp/sql/<name>.assets` with an S3 client (e.g. `rclone`) to `dicts/<name>.assets`.
- A D1 database holds at most 10 GB.

## API

| Method | Path                         |                                              |
| ------ | ---------------------------- | -------------------------------------------- |
| `GET`  | `/dicts`                     | ready dictionaries                           |
| `POST` | `/search`                    | see below                                    |
| `GET`  | `/dicts/:name/assets/<path>` | a media file; usable directly as `<img src>` |

```jsonc
// POST /search
{
  "type": null, // null: prefix search on term | "exact" | "fts" (token search)
  "lang": "ja-JP", // optional; restricts to terms tokenizable in that language
  "term": "日本", // prefix / exact modes
  "tokens": { "ja": "日本" }, // fts mode: client-side tokenized term per language
  "reading": "pinyin", // optional; with lang zh-CN or empty, pinyin is expanded to every tone combination
  "limit": 10, // max 50
  "offset": 0,
}
// -> { "items": [{ ...term_search, "entries": [{ dict_id, dict_path, term_id, term, reading, glossary, ... }] }], "next": 10 | null }
```

Pinyin expansion lives in `worker/src/pinyin.ts`, a port of `yomitan-parser/src/pinyin.rs`. The Rust version normalizes readings at import and the TS version expands queries, so keep them in sync. `pnpm test` runs the same cases as the Rust tests.

## App

```bash
pnpm app start                            # tauri dev (desktop), against `wrangler dev`
pnpm app tauri build                      # release bundle; needs VITE_DICT_API_URL
pnpm app tauri android init               # first time on Android
pnpm app android
```

Which Worker the app talks to:

- **Development** (`pnpm start`, `pnpm android`): the app calls same-origin `/api/*`, and Vite proxies it to `wrangler dev` on `http://localhost:8787`. Start it with `pnpm worker dev`. Set `WRANGLER_DEV_URL` to proxy somewhere else, e.g. a deployed worker.
- **Production builds**: `VITE_DICT_API_URL`, e.g. in `app/.env.production`. The build fails if it is not set. There is no in-app setting.

PrimeVue shows an "Invalid PrimeUI License" badge unless `VITE_PRIMEUI_LICENSE_KEY` is set too (same as cjk-explainer).

- The CSP allows inline styles (`style-src 'unsafe-inline'`), which PrimeVue's runtime theme needs.
- The HTTP capability (`app/src-tauri/capabilities/default.json`) and the CSP `img-src` (`app/src-tauri/tauri.conf.json`) allow `https://*.workers.dev`. For a custom domain, add it to both. Development needs neither, since it is same-origin through the proxy.
- Search modes: prefix (default), `=` exact, and `~` token search. `~` needs the tokenizer; until it has downloaded, `~` is disabled and falls back to prefix search.
- The search language (header) is the only setting; it is remembered per device.
- Optional: search by reading (pinyin is expanded to all tones by the Worker), auto-convert romaji to kana, and a clipboard monitor that searches whatever CJK text you copy.

## Local development

```bash
pnpm worker migrate:local
pnpm worker dev                     # wrangler dev on http://localhost:8787

# in another terminal
pnpm admin:local add some.zip
pnpm admin:local list
pnpm app start                      # the app, proxied to wrangler dev
```
