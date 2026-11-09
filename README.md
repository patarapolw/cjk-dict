# cjk-dict

Yomitan dictionary search for Chinese, Japanese and Korean: a Tauri app (desktop and Android) that searches a dictionary library kept in a private Cloudflare R2 bucket.

- The **library** is a folder of original Yomitan dictionary zips, plus `search.db`, a small SQLite index built from them. `dict-admin` manages it, and `rclone` mirrors it to R2.
- The **app** downloads the library, or only what changed, and searches on the device:
  - It queries `search.db` for matches.
  - It reads definitions and images straight from the zips, so nothing is indexed on the device.
  - It also downloads the lindera tokenizer dictionaries (~300 MB, in the background on first launch), used to tokenize "~" queries.

```
yomitan-parser/   Rust crate: Yomitan zip reader, search.db build + query, lindera tokenizer, pinyin
dict-admin/       Rust CLI: manage the library, build search.db, upload with rclone
app/              Tauri app (Vue 3 + PrimeVue); app/src-tauri is its own Cargo workspace
```

All commands below run from the repo root and work the same in bash and PowerShell. Run `pnpm install` once first. `pnpm format` runs Prettier, and `pnpm typecheck` checks the app.

## Security

- The library (`library/`, including `dicts.json`) is gitignored. Your dictionary list and custom zips live only on your machine and in R2.
- R2 credentials are only read from environment variables; `.env` files are gitignored.
- The app is built with a **read-only** R2 token compiled in, so anyone who has the built app can extract it and download the bucket. Keep that in mind before sharing builds.

## The library

```
library/
  <name>.zip      original Yomitan zips (default + custom dictionaries)
  dicts.json      [{ name, lang?, source? }]: build order and language overrides
  search.db       written by `build`: terms, tokens, readings, tags, and where each entry is in its zip
  manifest.json   written by `build`: file sizes and sha256; the app compares these to download only what changed
```

`pnpm admin <command>` runs `dict-admin` (`cargo run --release`), passing arguments through. The library folder defaults to `library/`; set `--library` or `CJK_DICT_LIBRARY` to use another.

```bash
pnpm admin defaults --list                 # default dictionaries and their latest download URLs
pnpm admin defaults                        # download all that aren't in the library yet
pnpm admin defaults --only CC-CEDICT --only wty-zh-en
pnpm admin defaults --only CC-CEDICT --replace           # update to the latest release

pnpm admin add path/to/MyDict.zip --lang ja-JP
pnpm admin add https://example.com/Other.zip --name Other --lang zh-CN
pnpm admin add new/MyDict.zip --name MyDict --replace    # new revision
pnpm admin remove MyDict
pnpm admin list

pnpm admin build                           # search.db + manifest.json
pnpm admin upload --remote r2:cjk-dict     # rclone; or set CJK_DICT_REMOTE
```

- **`add`** copies or downloads the zip into the library as `<name>.zip`, after checking that it is a Yomitan dictionary. `--lang` overrides the zip's `sourceLanguage`.
- **Default dictionaries** (the list the app used to download itself): Pixiv, `ja`/`zh`/`ko.Wikipedia` (MarvNC), CC-CEDICT, KO-EN.KRDICT, and `wty-ja-en`/`wty-ko-en`/`wty-zh-en` (Wiktionary). GitHub-hosted ones resolve to the asset of the repo's latest release. Set `GITHUB_TOKEN` if you hit the API's unauthenticated rate limit.
- **`build`** rebuilds `search.db` from scratch: CC-CEDICT, 203k terms, takes about 6 s. Only terms are indexed; definitions, term/kanji meta and media stay in the zips. The lindera tokenizer dictionaries are downloaded to `tmp/lindera` on first use (`--lindera-dir`, `CJK_DICT_LINDERA_DIR`).
- **`upload`** refuses a stale build. It copies the zips first, then `search.db`, then `manifest.json`, so the app never sees a manifest pointing at missing files. Then it deletes from R2 what was removed from the library. Behind a TLS-intercepting proxy, set `SSL_CERT_FILE`.

### R2 and rclone

1. Create a private bucket, e.g. `cjk-dict`.
2. Create two R2 API tokens:
   - **Object Read & Write**, for rclone on your machine.
   - **Object Read only**, scoped to the bucket, for the app.
3. Configure rclone (`rclone config`): type `s3`, provider `Cloudflare`, the read/write token's keys, and endpoint `https://<account id>.r2.cloudflarestorage.com`. Name the remote, e.g. `r2`. The destination is then `r2:cjk-dict`, or `r2:cjk-dict/some/prefix`.

## App

```bash
pnpm app start                             # tauri dev (desktop)
pnpm app tauri build                       # release bundle
pnpm app tauri android init                # first time on Android
pnpm app android
```

Where the app downloads the library from is set by environment variables. They are read when the app is built (compiled in) and can be overridden at runtime:

| Variable                        | Value                                                            |
| ------------------------------- | ---------------------------------------------------------------- |
| `CJK_DICT_R2_ACCOUNT_ID`        | Cloudflare account id                                            |
| `CJK_DICT_R2_ACCESS_KEY_ID`     | **read-only** token's access key id                              |
| `CJK_DICT_R2_SECRET_ACCESS_KEY` | **read-only** token's secret                                     |
| `CJK_DICT_R2_BUCKET`            | e.g. `cjk-dict`                                                  |
| `CJK_DICT_R2_PREFIX`            | optional folder inside the bucket                                |
| `CJK_DICT_BASE_URL`             | instead of R2: a plain HTTP base URL, e.g. for local development |

The app signs its R2 requests itself, with presigned URLs. For development without R2, serve the local library:

```bash
rclone serve http library --addr 127.0.0.1:8080
# then run the app with CJK_DICT_BASE_URL=http://127.0.0.1:8080
```

On start, the app compares the remote `manifest.json` with its own copy and downloads only changed files, in the background and resumably. They go to a staging folder and replace the old files only when all of them have arrived and their checksums match. Until then, the app keeps searching the previous version.

- Search modes: prefix (default), `=` exact, and `~` token search. `~` needs the tokenizer; until it has downloaded, `~` is disabled and falls back to prefix search.
- The search language (header) is the only setting; it is remembered per device.
- Optional: search by reading (pinyin is expanded to all tones), auto-convert romaji to kana, and a clipboard monitor that searches whatever CJK text you copy.
- Images in definitions are served from the zips through a `dict://` protocol.
- PrimeVue shows an "Invalid PrimeUI License" badge unless `VITE_PRIMEUI_LICENSE_KEY` is set. The CSP allows inline styles (`style-src 'unsafe-inline'`), which PrimeVue's runtime theme needs.
