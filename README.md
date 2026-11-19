# cjk-dict

Yomitan dictionary search for Chinese, Japanese and Korean: a Tauri app (desktop and Android) that searches a dictionary library kept in a private Cloudflare R2 bucket.

- The **library** is a folder of Yomitan dictionary zips (term banks re-split into ~256 KB files so the app can read single entries quickly), plus `search.db`, a small SQLite index built from them. `dict-admin` manages it, and `rclone` mirrors it to R2.
- The **app** downloads the library, or only what changed, and searches on the device:
  - It queries `search.db` for matches.
  - It reads definitions and images straight from the zips, so nothing is indexed on the device.
  - It also downloads the lindera tokenizer dictionaries (\~300 MB, in the background on first launch), used to tokenize "\~" queries.

```
yomitan-parser/   Rust crate: Yomitan zip reader, search.db build + query, lindera tokenizer, pinyin
dict-admin/       Rust CLI: manage the library, build search.db, upload with rclone
app/              Tauri app (Vue 3); app/src-tauri is its own Cargo workspace
```

All commands below run from the repo root and work the same in bash and PowerShell. Run `pnpm install` once first.

Local development needs no settings: `pnpm serve` serves `library/` over HTTP with rclone, `pnpm dev` runs the app against it, and `dict-admin` defaults to `library/` and `tmp/lindera`. Production settings (R2 keys and the rclone upload destination) go in `.env` at the repo root (gitignored); copy `.env.example`. The one file is read by the app's Rust build, by Vite (`VITE_*`) and by `dict-admin`. Variables already set in the environment take precedence. `pnpm format` runs Prettier, and `pnpm typecheck` checks the app.

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
  dicts.json      [{ name, lang?, source? }]: build order and language overrides
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

All commands:

```bash
pnpm admin defaults --list                 # default dictionaries and their latest download URLs
pnpm admin defaults                        # download all that aren't in the library yet
pnpm admin defaults --only CC-CEDICT --only wty-zh-en
pnpm admin defaults --only CC-CEDICT --replace           # update to the latest release
pnpm admin defaults --langs                # update the defaults' languages in dicts.json, no download

pnpm admin add path/to/MyDict.zip --lang ja-JP
pnpm admin add https://example.com/Other.zip --name Other --lang zh-TW
pnpm admin add path/to/JaKoDict.zip --lang ja-JP --target-lang ko  # definitions in Korean
pnpm admin add new/MyDict.zip --name MyDict --replace    # new revision
pnpm admin remove MyDict
pnpm admin list                            # with L1, L2 (* set in dicts.json), size, entries
pnpm admin size                             # estimated R2 mirror size by language
pnpm admin size --lang ja-JP                # only Japanese dictionary zips

pnpm admin build                           # search.db + manifest.json
pnpm admin rebuild                        # discard and rebuild the complete search index
pnpm admin upload --remote r2:cjk-dict     # rclone; or set CJK_DICT_REMOTE
```

- **`add`** copies or downloads the zip into the library as `<name>.zip`, after checking that it is a Yomitan dictionary. `--lang` overrides the zip's `sourceLanguage`, the terms' language (L1), and `--target-lang` its `targetLanguage`, the definitions' language (L2). The app marks terms with L1 and definitions with L2 when it is Japanese, Chinese or Korean, else with L1, so Han characters take the forms of their language: Japanese, Korean, or Chinese as the dictionary has it (`zh`, `zh-TW`, ...; Mandarin isn't always `zh-CN`). `--target-lang` is needed only for definitions in another of those languages than the terms, e.g. `ja` for Pixiv, which gives neither. To change a dictionary's languages without adding it again, edit `lang` / `target_lang` in `dicts.json`, then `build`. KANJIDIC2 JSON from jmdict-simplified (`kanjidic2-en-*.json.zip`, or the `.json`) is converted to a Yomitan kanji dictionary first: on and kun readings, meanings, and the rest as stats with names, including the Chinese (pinyin, with tone marks) and Korean readings and name readings, which the app shows with the readings, and the romanized Korean and Vietnamese readings, which it shows with the details. `pnpm admin defaults --only KANJIDIC --replace` updates it to the latest release.
- **Default dictionaries** (the list the app used to download itself): KANJIDIC (kanji, from [jmdict-simplified](https://github.com/scriptin/jmdict-simplified)'s JSON of the JMdict project's KANJIDIC2; no language, as it has Japanese, Chinese and Korean readings), Pixiv, `ja`/`zh`/`ko.Wikipedia` (MarvNC), CC-CEDICT, KO-EN.KRDICT, and `wty-ja-en`/`wty-ko-en`/`wty-zh-en` (Wiktionary). GitHub-hosted ones resolve to the asset of the repo's latest release. Set `GITHUB_TOKEN` if you hit the API's unauthenticated rate limit. `defaults` records each one's language overrides in `dicts.json` as it adds it; `defaults --langs` rewrites them for those already in the library, without downloading, which is needed only when a new version of `dict-admin` changes them (then `build`).
- **`build`** updates `search.db` incrementally: it indexes new or changed dictionaries and removes deleted ones. It hashes the zips against the previous manifest to detect replacements. **`rebuild`** discards the current index and rebuilds it from scratch. Only terms, and where kanji entries are, are indexed; definitions, term/kanji meta and media stay in the zips. The lindera tokenizer dictionaries are downloaded to `tmp/lindera` when indexing is needed (`--lindera-dir`, `CJK_DICT_LINDERA_DIR`).
- **Characters**: `build` also indexes dictionaries' kanji banks (single characters only; e.g. KANJIDIC, or the Hanzi edition of CC-CEDICT, which also has whole words there) and fills `search.db`'s character tables from the [CJKVI IDS database](https://github.com/cjkvi/cjkvi-ids)'s `ids.txt`: how each character breaks down into components, which characters each component is in, its Hangul readings (from hanja_hangul), and how many terms use it. Japanese words' character readings need a kanji dictionary with Japanese readings, which `build` warns about when there is none: KANJIDIC is one of the defaults. `ids.txt` is downloaded to `tmp/cjkvi-ids` (`--ids-dir`, `CJK_DICT_IDS_DIR`); delete it to get the latest. The tables are refilled only when `ids.txt` or the dictionaries change, so an unchanged library keeps the same `search.db`. `add` rejects a zip with neither term nor kanji entries, e.g. a broken release.
- **`size`** estimates the whole library's R2 mirror size, grouping dictionary ZIPs by language and counting shared files once. `--lang` filters the ZIP breakdown only; it does not change what `upload` sends. Run it after `build`; removing dictionaries and rebuilding lets you compare the updated storage estimate.
- **`upload`** refuses a stale build. It copies the zips first, then `search.db`, then `manifest.json`, so the app never sees a manifest pointing at missing files. Then it deletes from R2 what was removed from the library. Behind a TLS-intercepting proxy, set `SSL_CERT_FILE`.

### R2 and rclone

1. Create a private bucket, e.g. `cjk-dict`.
2. Create two R2 API tokens:
   - **Object Read & Write**, for rclone on your machine.
   - **Object Read only**, scoped to the bucket, for the app.
3. Configure rclone (`rclone config`): type `s3`, provider `Cloudflare`, the read/write token's keys, and endpoint `https://<account id>.r2.cloudflarestorage.com`. Name the remote, e.g. `r2`. The destination is then `r2:cjk-dict`, or `r2:cjk-dict/some/prefix`.

## App

```bash
pnpm serve                                 # terminal 1: local library on 127.0.0.1:8080 (Ctrl+C to stop)
pnpm dev                                   # terminal 2: tauri dev against it, no .env needed
pnpm app start                             # tauri dev (desktop) against R2, from .env
pnpm app tauri build                       # package a release desktop app
pnpm app android                           # tauri dev (Android) without Vite dev server setup. Stays intact when detached.
pnpm app tauri android dev                 # tauri dev (Android) with Vite dev server. May require OS firewall setup.
```

To make a desktop build, run `pnpm app tauri build`. On Windows, the app executable is at `app/src-tauri/target/release/cjk-dict-app.exe`, with installer bundles under `app/src-tauri/target/release/bundle/`. You can run the executable directly and create a shortcut instead of installing it.

To debug in VS Code on Windows, use the launch configurations in [`.vscode/launch.json`](.vscode/launch.json), adapted from [Tauri's VS Code guide](https://tauri.app/develop/debug/vscode/): **App** builds and starts the app under the Visual Studio debugger (C/C++ extension) for breakpoints in Rust, **WebView** attaches to the frontend for breakpoints in TypeScript and Vue, and **App + WebView** does both. They start the app without the Tauri CLI, running Vite and `cargo build` as tasks instead.

Where the app downloads the library from is set in `.env` or by environment variables. They are read when the app is built (compiled in) and can be overridden at runtime:

| Variable                        | Value                                                            |
| ------------------------------- | ---------------------------------------------------------------- |
| `CJK_DICT_R2_ACCOUNT_ID`        | Cloudflare account id                                            |
| `CJK_DICT_R2_ACCESS_KEY_ID`     | **read-only** token's access key id                              |
| `CJK_DICT_R2_SECRET_ACCESS_KEY` | **read-only** token's secret                                     |
| `CJK_DICT_R2_BUCKET`            | e.g. `cjk-dict`                                                  |
| `CJK_DICT_R2_PREFIX`            | optional folder inside the bucket                                |
| `CJK_DICT_BASE_URL`             | instead of R2: a plain HTTP base URL, e.g. for local development |

The app signs its R2 requests itself, with presigned URLs. For development without R2, `pnpm serve` runs `rclone serve http library --addr 127.0.0.1:8080`, and `pnpm dev` runs the app with `CJK_DICT_BASE_URL=http://127.0.0.1:8080`. They run in separate terminals, so the server keeps running across app restarts until you stop it. The inline variable overrides `.env`, and works on Windows because `.npmrc` turns on pnpm's `shell-emulator`. Run `pnpm admin build` first so the library has a `manifest.json`.

On start, the app compares the remote `manifest.json` with its own copy and downloads only changed files, in the background and resumably. They go to a staging folder and replace the old files only when all of them have arrived and their checksums match. Until then, the app keeps searching the previous version.

- Search modes: prefix (default), `=` exact, and `~` token search. `~` needs the tokenizer; until it has downloaded, `~` is disabled and falls back to prefix search.
- Katakana and hiragana find each other: `こーひー` finds コーヒー, and `<興>{キョウ}` finds 興味 (きょうみ). `search.db` stores terms, tokens and readings with katakana as hiragana; `dict-admin build` rebuilds a `search.db` from before this from scratch, and the app searches one as before until then.
- One search box. Readings use furigana syntax: `<日本>{にほん}語` matches 日本語 read にほん…, and `{reading}` alone searches by reading (`{zhong1 guo2}`; pinyin is expanded to all tones).
- Wildcards, in a term and in a term's reading: `*` any characters, `?` one character. `日?` finds 日本 and 日米…, `<日>{に?}本` 日本 read にほん or にっぽん…, and a leading `*` matches anywhere in the word: `*語` finds 日本語. In a term, `[...]` matches one of the characters and `[^...]` none of them: `[日月]曜日`; in a term's reading and in a reading alone, `[...]`: `{にほ[んっ]`. Full-width `＊？［］` work too. Not `*` or `?` in a reading alone (`{に?ほん}` is two reading words), nor with `=` or `~`. Without the term's index to narrow it, a search starting with a wildcard walks the terms by score: quick when matches are common, seconds on a big library when they are few.
- A character's reading: `*<興>{キョウ}` (one character, a reading without wildcards) finds the words with 興 read キョウ anywhere in them: 興味, 余興… It looks the character and reading up in `search.db`'s `term_char` index, which `dict-admin build` fills by splitting each term's reading among its characters: Chinese has a pinyin syllable per character and Korean a Hangul syllable per Hanja; Japanese uses the kanji dictionaries' readings (KANJIDIC's on, kun and name readings), so needs one in the library, and allows for compounds' sound changes (学校 がっこう: 学 is がく), so `*<学>{ガク}` finds 学校 too. Words whose reading belongs to the whole word (馬酔木 あせび) are not in it. Korean readings match with the initial-sound rule either way (`*<林>{림}` finds 林業 임업). Without the index, in a `search.db` from before it, the same query is the wildcard search.
- A space marks a word boundary: only the first word is looked up, and "Next" moves on to the rest. When nothing matches a longer text (e.g. a copied sentence), it is split into words with the tokenizer.
- Each result links to an exact search for itself, `~` words containing it, words with the same reading, and its characters. In-dictionary links and the back button keep a search history.
- **Characters** (`#/kanji?q=<characters>`): for each Han character, how it breaks down (each IDS, by region: the one for the search language first), every component all the way down, its kanji dictionary entries (readings, meanings, and stats with the dictionary's own notes), Hangul readings, and its single-character word entries. Each reading searches the words with the character read so, anywhere in the word: on `*<興>{キョウ}`, kun `*<興>{おこ}` (up to the okurigana, which the dot marks: 興る and 興す), pinyin `*<興>{xīng}`, Hangul `*<興>{흥}`; a Chinese or Korean reading switches the search language to its own, unless it is Auto-detect. Components link to their own page. Below, the characters made of all the typed characters, the most used first: `木口` lists 呆, 杏, ... A radical also matches the forms it takes as a component (水 finds 河, written with 氵), and Kangxi radicals (`⼝`) are read as the ideograph.
- The search language (header) is the only setting; it is remembered per device.
- The app follows the device's light or dark color-scheme preference.
- Korean: with Korean selected, typing Revised Romanization finds Hangul (`hangugeo`, `hangukeo` or `hangug-eo` → 한국어). Terms are romanized at search time, both as pronounced (국물 → gungmul, 신라 → silla) and letter by letter (gukmul), and loose spellings match: `eo`/`o`, `eu`/`u`, `ae`/`e`, `k`/`g`, `t`/`d`, `p`/`b`, `r`/`l`. `search.db` keeps these romanization keys in `term_roman`, so each keystroke is an index lookup; `dict-admin build` adds the table to an existing `search.db`. A `search.db` built before that still works, more slowly, by romanizing the terms as it searches.
- Korean headwords written in Hanja get Hangul readings when `search.db` is built (光速 → 광속, 光나다 → 광나다), since Korean dictionaries give none. They come from [hanja_hangul](https://crates.io/crates/hanja_hangul): a reading per character, the initial-sound rule (李 → 이), and a table of dictionary words that read otherwise; of a character's several readings (樂: 락, 낙, 악, 요), the one that is also a Hangul headword is kept. A Hangul word is its own reading, so `광속`, `{광속}` and `gwangsok` find both 광속 and 光速, and `<光速>{광속}` finds 光速. `dict-admin build` fills in the readings of an existing `search.db` too. hanja_hangul's word tables are CC BY-SA 2.0 KR (derived from the National Institute of Korean Language's dictionaries); only `dict-admin` uses it (yomitan-parser's `hangul-readings` feature), not the app.
- Optional: auto-convert romaji to kana (a syllable still being typed searches every kana it can become, by the converter's extended rules and with obsolete kana such as ゐ: `にほんg` is `にほん[がぎぐげごっ]`, `{にほn` `{にほ[なにぬねのん]`, `<食>{たb}べる` `<食>{た[っばびぶべぼ]}べる`), IME furigana (a Japanese IME's reading is kept as `<漢字>{かんじ}` when you convert), and a clipboard monitor that searches whatever CJK text you copy.
- Definitions render as in Yomitan: structured content (only the schema's elements and styles, `data` as `data-sc-*` attributes), each dictionary's own `styles.css` scoped to its entries, and images sized from `width`/`height`/`sizeUnits`, including monochrome images. Where the app differs from Yomitan, for example showing images that dictionaries mark collapsed, is listed in [docs/yomitan-differences.md](docs/yomitan-differences.md).
- Images in definitions are served from the zips through a `dict://` protocol.
- The CSP allows inline styles (`style-src 'unsafe-inline'`), used for the download progress indicator, structured content styles and dictionaries' `styles.css`.

## License

The code is under the [MIT License](LICENSE). Dictionaries, and data built from them such as `search.db`, keep their own licenses. So do the downloaded tokenizer dictionaries, [hanja_hangul](https://crates.io/crates/hanja_hangul)'s word tables, which `dict-admin` includes, and the [CJKVI IDS database](https://github.com/cjkvi/cjkvi-ids)'s `ids.txt` (derived from the CHISE project, GPLv2), which `search.db` includes.
