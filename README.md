# cjk-dict

Yomitan dictionary search for Chinese, Japanese and Korean: a Tauri app (desktop and Android) that searches a dictionary library kept in a private Cloudflare R2 bucket.

- The **library** is a folder of Yomitan dictionary zips, plus `search.db`, a small SQLite index built from them.
  - The zips' term banks are re-split into \~256 KB files, so the app can read single entries quickly.
  - `dict-admin` manages the library, and `rclone` mirrors it to R2.
- The **app** downloads the library, or only what changed, and searches on the device:
  - It queries `search.db` for matches.
  - It reads definitions and images straight from the zips, so nothing is indexed on the device.

```
yomitan-parser/   Rust crate: Yomitan zip reader, search.db build + query, lindera tokenizer, pinyin
dict-admin/       Rust CLI: manage the library, build search.db, upload with rclone
app/              Tauri app (Vue 3); app/src-tauri is its own Cargo workspace
docs/             how dict-admin, searching and the app work in detail
```

Words are split into tokens by [lindera](https://github.com/lindera/lindera), a morphological analyzer, with its unidic (Japanese), ko-dic (Korean) and cc-cedict (Chinese) dictionaries. `dict-admin` uses them to index terms, and the app to tokenize `~` queries.

More in [`docs/`](docs/):

- [dict-admin.md](docs/dict-admin.md): each `dict-admin` command, dictionary languages, the default dictionaries.
- [search.md](docs/search.md): search syntax, wildcards, readings, characters, Korean romanization.
- [app.md](docs/app.md): how the app downloads the library, shows entries, and its tooltips.
- [yomitan-differences.md](docs/yomitan-differences.md): where rendering differs from Yomitan.

## Setup

- All commands run from the repo root, and work the same in bash and PowerShell. Run `pnpm install` once first.
- Local development needs no settings:
  - `pnpm serve` serves `library/` over HTTP with rclone;
  - `pnpm dev` runs the app against it;
  - `dict-admin` defaults to `library/` and `tmp/lindera`.
- Production settings (R2 keys and the rclone upload destination) go in `.env` at the repo root (gitignored); copy `.env.example`.
  - The one file is read by the app's Rust build and by `dict-admin`. Vite reads it too, though the frontend uses no `VITE_*` variables yet.
  - Variables already set in the environment take precedence.
- `pnpm format` runs Prettier, and `pnpm typecheck` checks the app.

## Security

The app is currently set up for personal use. A public release would need changes first.

- A build compiles in a **read-only** R2 token, and anyone who has the build can extract it and download the whole bucket. Sharing a build shares every dictionary in the library.
- The library can include dictionaries you may not redistribute. Check their terms before sharing a build with anyone.
- The library (`library/`, including `dicts.json`) is gitignored. Your dictionary list and custom zips live only on your machine and in R2.
- R2 credentials are only read from `.env` or environment variables, and `.env` is gitignored.

## The library

```
library/
  <name>.zip      Yomitan zips, rebanked (default + custom dictionaries)
  dicts.json      [{ name, lang?, target_lang?, source? }]: build order and language overrides
  search.db       written by `build`: terms, tokens, readings, tags, and where each entry is in its zip;
                  character decomposition, and where each kanji entry is
  manifest.json   written by `build`: file sizes and sha256; the app compares these to download only what changed
```

`pnpm admin <command>` runs `dict-admin` (`cargo run --release`), passing arguments through. The library folder defaults to `library/`; set `--library` or `CJK_DICT_LIBRARY` to use another.

To start a library and publish it:

```bash
pnpm admin defaults                        # download the default dictionaries
pnpm admin build                           # search.db + manifest.json
pnpm admin upload --remote r2:cjk-dict     # rclone; or set CJK_DICT_REMOTE
```

All commands (details in [docs/dict-admin.md](docs/dict-admin.md)):

```bash
pnpm admin defaults --list                 # default dictionaries and their latest download URLs
pnpm admin defaults                        # download all that aren't in the library yet
pnpm admin defaults --only CC-CEDICT --only wty-zh-en
pnpm admin defaults --only CC-CEDICT --replace           # update to the latest release

pnpm admin add path/to/MyDict.zip --lang ja-JP
pnpm admin add https://example.com/Other.zip --name Other --lang zh-TW
pnpm admin add path/to/JaKoDict.zip --lang ja-JP --target-lang ko  # definitions in Korean
pnpm admin add new/MyDict.zip --name MyDict --replace    # new revision
pnpm admin remove MyDict
pnpm admin list                            # with L1, L2 (* set in dicts.json), size, entries
pnpm admin size                            # estimated R2 mirror size by language
pnpm admin size --lang ja-JP               # only Japanese dictionary zips

pnpm admin build                           # search.db + manifest.json
pnpm admin rebuild                         # discard and rebuild the complete search index
pnpm admin upload --remote r2:cjk-dict     # rclone; or set CJK_DICT_REMOTE
```

- `--lang` and `--target-lang` set a dictionary's languages: its terms' (L1) and its definitions' (L2), which pick the forms of Han characters. Most dictionaries need neither; see [Languages](docs/dict-admin.md#languages).
- `build` indexes only new or changed dictionaries; `rebuild` starts over.
- `upload` refuses a stale build, and uploads in an order that never leaves the app a manifest pointing at missing files.

### R2 and rclone

1. Create a private bucket, e.g. `cjk-dict`.
2. Create two R2 API tokens:
   - **Object Read & Write**, for rclone on your machine.
   - **Object Read only**, scoped to the bucket, for the app.
3. Configure rclone (`rclone config`):
   - type `s3`, provider `Cloudflare`;
   - the read/write token's keys;
   - endpoint `https://<account id>.r2.cloudflarestorage.com`;
   - name the remote, e.g. `r2`. The destination is then `r2:cjk-dict`, or `r2:cjk-dict/some/prefix`.
4. Set `CJK_DICT_REMOTE` in `.env` to that destination, or pass it as `upload --remote`.

`dict-admin` reads these from `.env` or the environment; command-line options override them:

| Variable               | Value                                                                                                                                            |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `CJK_DICT_REMOTE`      | where `upload` copies the library: an rclone `<remote>:<bucket>[/<prefix>]`, e.g. `r2:cjk-dict`, `r2` being the remote's name in `rclone config` |
| `CJK_DICT_LIBRARY`     | the library folder; default `library`                                                                                                            |
| `CJK_DICT_LINDERA_DIR` | the tokenizer dictionaries; default `tmp/lindera`                                                                                                |
| `CJK_DICT_IDS_DIR`     | the CJKVI IDS database; default `tmp/cjkvi-ids`                                                                                                  |
| `GITHUB_TOKEN`         | optional, for `defaults` when GitHub's API rate-limits it                                                                                        |

A prefix in `CJK_DICT_REMOTE` must match the app's `CJK_DICT_R2_PREFIX`.

## App

```bash
pnpm serve                                 # terminal 1: local library on 127.0.0.1:8080 (Ctrl+C to stop)
pnpm dev                                   # terminal 2: tauri dev against it, no .env needed
pnpm app start                             # tauri dev (desktop) against R2, from .env
pnpm app tauri build                       # package a release desktop app
pnpm app android                           # tauri dev (Android) without Vite dev server setup. Stays intact when detached.
pnpm app tauri android dev                 # tauri dev (Android) with Vite dev server. May require OS firewall setup.
```

### Local development

- `pnpm serve` runs `rclone serve http library --addr 127.0.0.1:8080`.
- `pnpm dev` runs the app with `CJK_DICT_BASE_URL=http://127.0.0.1:8080`.
  - The inline variable overrides `.env`.
  - It works on Windows because `.npmrc` turns on pnpm's `shell-emulator`.
- They run in separate terminals, so the server keeps running across app restarts until you stop it.
- Run `pnpm admin build` first, so the library has a `manifest.json`.

### Desktop builds

- Run `pnpm app tauri build`.
- On Windows:
  - the app executable is at `app/src-tauri/target/release/cjk-dict-app.exe`;
  - installer bundles are under `app/src-tauri/target/release/bundle/`.
- You can run the executable directly and create a shortcut instead of installing it.

### Debugging in VS Code

On Windows, use the launch configurations in [`.vscode/launch.json`](.vscode/launch.json), adapted from [Tauri's VS Code guide](https://tauri.app/develop/debug/vscode/):

- **App** builds and starts the app under the Visual Studio debugger (C/C++ extension), for breakpoints in Rust.
- **WebView** attaches to the frontend, for breakpoints in TypeScript and Vue.
- **App + WebView** does both.

They start the app without the Tauri CLI, running Vite and `cargo build` as tasks instead.

### Debugging on Android

- Turn on USB debugging on the phone, connect it, and run `pnpm app android` (a debug build).
- A debug build allows WebView debugging: open `chrome://inspect/#devices` in Chrome (or `edge://inspect` in Edge) on the computer, and inspect the app's WebView.
- It serves its own frontend, so it needs no firewall setup, unlike `pnpm app tauri android dev`.
- Some Android settings, e.g. Accessibility → Touch & hold delay, take effect when the app restarts.
- For driving the app from scripts (`adb forward`, `scripts/webview.mjs`), see [AGENTS.md](AGENTS.md#on-android).

### Where the app downloads from

Set in `.env` or by environment variables. They are read when the app is built (compiled in), and can be overridden at runtime:

| Variable                        | Value                                                            |
| ------------------------------- | ---------------------------------------------------------------- |
| `CJK_DICT_R2_ACCOUNT_ID`        | Cloudflare account id                                            |
| `CJK_DICT_R2_ACCESS_KEY_ID`     | **read-only** token's access key id                              |
| `CJK_DICT_R2_SECRET_ACCESS_KEY` | **read-only** token's secret                                     |
| `CJK_DICT_R2_BUCKET`            | e.g. `cjk-dict`                                                  |
| `CJK_DICT_R2_PREFIX`            | optional folder inside the bucket                                |
| `CJK_DICT_BASE_URL`             | instead of R2: a plain HTTP base URL, e.g. for local development |

How the app downloads and searches the library: [docs/app.md](docs/app.md), [docs/search.md](docs/search.md).

## License

- The code is under the [MIT License](LICENSE).
- Dictionaries, and data built from them such as `search.db`, keep their own licenses: see the [default dictionaries](docs/dict-admin.md#defaults) and their projects. So do:
  - [lindera](https://github.com/lindera/lindera)'s tokenizer dictionaries, downloaded from its releases;
  - [hanja_hangul](https://crates.io/crates/hanja_hangul)'s word tables, which `dict-admin` includes;
  - [libhangul](https://github.com/libhangul/libhangul)'s Hanja readings (BSD-3-Clause), which `dict-admin` includes;
  - the [CJKVI IDS database](https://github.com/cjkvi/cjkvi-ids)'s `ids.txt` (derived from the CHISE project, GPLv2), which `search.db` includes.
