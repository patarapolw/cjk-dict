# dict-admin

`dict-admin` manages the dictionary library, builds its search index and uploads it to R2. Run it as `pnpm admin <command>` from the repo root; the [README](../README.md#the-library) has the commands at a glance.

## `add`

- Copies or downloads a zip into the library as `<name>.zip`, after checking that it is a Yomitan dictionary.
- Rejects a zip with neither term nor kanji entries, e.g. a broken release.
- `--replace` overwrites a dictionary of the same name, e.g. with a new revision.

### Languages

- `--lang` overrides the zip's `sourceLanguage`: the terms' language (L1).
- `--target-lang` overrides its `targetLanguage`: the definitions' language (L2).
- The app marks terms and definitions with them, so Han characters take the forms of their language: Japanese, Korean, or Chinese as the dictionary has it (`zh`, `zh-TW`, ...; Mandarin isn't always `zh-CN`).
  - Terms take L1.
  - Definitions take L2 when it is Japanese, Chinese or Korean, else L1, so Han characters in English definitions take the terms' forms.
- `--target-lang` is needed only for definitions in another of those languages than the terms, e.g. `ja` for Pixiv, which gives neither.
- To change a dictionary's languages without adding it again, edit `lang` / `target_lang` in `dicts.json`, then `build`.

### KANJIDIC2

KANJIDIC2 JSON from jmdict-simplified (`kanjidic2-en-*.json.zip`, or the `.json`) is converted to a Yomitan kanji dictionary first:

- on and kun readings, and meanings;
- the rest as stats with names:
  - shown with the readings: the Chinese (pinyin, with tone marks) and Korean readings, and name readings;
  - shown with the details: the romanized Korean and Vietnamese readings.

`pnpm admin defaults --only KANJIDIC --replace` updates it to the latest release.

## `defaults`

The default dictionaries (the list the app used to download itself):

- KANJIDIC: kanji, from [jmdict-simplified](https://github.com/scriptin/jmdict-simplified)'s JSON of the JMdict project's KANJIDIC2. No language, as it has Japanese, Chinese and Korean readings.
- Pixiv.
- `ja`/`zh`/`ko.Wikipedia` (MarvNC).
- CC-CEDICT.
- KO-EN.KRDICT.
- `wty-ja-en`/`wty-ko-en`/`wty-zh-en` (Wiktionary).

GitHub-hosted ones resolve to the asset of the repo's latest release. Set `GITHUB_TOKEN` if you hit the API's unauthenticated rate limit.

## `build` and `rebuild`

- `build` updates `search.db` incrementally:
  - It indexes new or changed dictionaries, and removes deleted ones.
  - It hashes the zips against the previous manifest to detect replacements.
- `rebuild` discards the current index and rebuilds it from scratch.
- Only terms, and where kanji entries are, are indexed. Definitions, term/kanji meta and media stay in the zips.
- The lindera tokenizer dictionaries are downloaded to `tmp/lindera` when indexing is needed (`--lindera-dir`, `CJK_DICT_LINDERA_DIR`).

### Characters

`build` also indexes dictionaries' kanji banks: single characters only, e.g. KANJIDIC, or the Hanzi edition of CC-CEDICT, which also has whole words there.

It fills `search.db`'s character tables from the [CJKVI IDS database](https://github.com/cjkvi/cjkvi-ids)'s `ids.txt`:

- how each character breaks down into components;
- which characters each component is in;
- its Hangul readings (from hanja_hangul);
- how many terms use it.

Notes:

- Japanese words' character readings need a kanji dictionary with Japanese readings. `build` warns when there is none; KANJIDIC is one of the defaults.
- `ids.txt` is downloaded to `tmp/cjkvi-ids` (`--ids-dir`, `CJK_DICT_IDS_DIR`); delete it to get the latest.
- The tables are refilled only when `ids.txt` or the dictionaries change, so an unchanged library keeps the same `search.db`.

### Korean Hanja readings

Korean headwords written in Hanja get Hangul readings when `search.db` is built (光速 → 광속, 光나다 → 광나다), since Korean dictionaries give none.

- They come from [hanja_hangul](https://crates.io/crates/hanja_hangul):
  - a reading per character;
  - the initial-sound rule (李 → 이);
  - a table of dictionary words that read otherwise.
- Of a character's several readings (樂: 락, 낙, 악, 요), the one that is also a Hangul headword is kept.
- `build` fills in the readings of an existing `search.db` too.
- hanja_hangul's word tables are CC BY-SA 2.0 KR (derived from the National Institute of Korean Language's dictionaries). Only `dict-admin` uses it (yomitan-parser's `hangul-readings` feature), not the app.

## `size`

- Estimates the whole library's R2 mirror size, grouping dictionary zips by language and counting shared files once.
- `--lang` filters the zip breakdown only; it does not change what `upload` sends.
- Run it after `build`. Removing dictionaries and rebuilding lets you compare the updated estimate.

## `upload`

- Refuses a stale build.
- Copies the zips first, then `search.db`, then `manifest.json`, so the app never sees a manifest pointing at missing files.
- Then deletes from R2 what was removed from the library.
- Behind a TLS-intercepting proxy, set `SSL_CERT_FILE`.
