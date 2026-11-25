# dict-admin

`dict-admin` manages the dictionary library, builds its search index and uploads it to R2. Run it as `pnpm admin <command>` from the repo root; the [README](../README.md#the-library) has the commands at a glance.

## `add`

- Copies or downloads a zip into the library as `<name>.zip`, after checking that it is a Yomitan dictionary.
- Rejects a zip with no term, kanji or pitch accent entries, e.g. a broken release.
- `--replace` overwrites a dictionary of the same name, e.g. with a new revision.

### Languages

- `--lang` overrides the zip's `sourceLanguage`: the terms' language (L1).
- `--target-lang` overrides its `targetLanguage`: the definitions' language (L2).
- The app marks terms and definitions with these, so Han characters take their language's forms:
  - terms take L1;
  - definitions take L2 if it is Japanese, Chinese or Korean, else L1, so Han characters in English definitions take the terms' forms.
- Chinese keeps the tag the dictionary gives (`zh`, `zh-TW`, ...): Mandarin isn't always `zh-CN`.
- `--target-lang` matters only when the definitions are in a different one of Japanese, Chinese and Korean than the terms, e.g. `ja` for Pixiv, whose `index.json` gives no languages.
- To change a dictionary's languages without adding it again, edit `lang` / `target_lang` in `dicts.json`, then `build`.

### KANJIDIC2

KANJIDIC2 JSON from jmdict-simplified (`kanjidic2-en-*.json.zip`, or the `.json`) is converted to a Yomitan kanji dictionary first:

- on and kun readings, and meanings;
- the rest as stats with names:
  - shown with the readings: the Chinese (pinyin, with tone marks) and Korean readings, and name readings;
  - shown with the details: the romanized Korean and Vietnamese readings.

`pnpm admin defaults --only KANJIDIC --replace` updates it to the latest release.

### Kanjium

[Kanjium](https://github.com/mifunetoshiro/kanjium)'s `accents.txt` has no Yomitan release, so `add` converts it to a Yomitan dictionary of pitch accents (`term_meta_bank` entries of mode `pitch`, no terms):

- Each line is `term<TAB>reading<TAB>positions`, e.g. `１	ひと	0,2`.
  - A position is the mora after which the pitch drops; `0`: it doesn't.
  - An empty reading means the term is kana: it becomes the reading.
  - Parts of speech before positions become their tags: `(副)1,(形動)0`, `(名;形動)0`.
- The same word on two lines is merged.
- The conversion is deterministic; the revision is `accents.txt` and the start of its SHA-256.
- `index.json`'s `attribution` has the credit Kanjium asks for (see [Defaults](#defaults)).

`pnpm admin defaults --only Kanjium --replace` downloads the latest `accents.txt` again.

### Hanja (libhangul)

[libhangul](https://github.com/libhangul/libhangul)'s `hanja.txt` has no Yomitan release, so `add` converts it to a Yomitan kanji dictionary of Hanja, with meanings in Korean:

- Each line is `hangul:hanja:meanings`, e.g. `광:光:빛 광`; the meanings are `, `-separated, each a meaning-reading (훈음).
- Single characters only: words, and the few lines that don't parse, are left out.
- A character's readings are those a character dictionary lists, as in the Characters view's header (see [Korean Hanja readings](#korean-hanja-readings)): hanja_hangul's main reading first, and none the initial-sound rule makes from another (樂 악 락 요, not also 낙).
- Its meanings are those of the readings kept: 樂 풍류 악, 즐길 악, 즐거울 락, ...
- A character with no meaning, e.g. a simplified form (学), is left out: about 7,400 characters are kept.
- The conversion is deterministic; the revision is `hanja.txt` and the start of its SHA-256.
- `index.json`'s `attribution` is the copyright and license notice `hanja.txt` starts with (BSD-3-Clause).

`pnpm admin defaults --only Hanja --replace` downloads the latest `hanja.txt` again.

## `defaults`

The default dictionaries (the list the app used to download itself), each downloaded from the project that converts it to Yomitan:

| Name                                | What                                                                                                                            | Yomitan conversion                                                                                                                                                                                                                                      | From                                                                                                              |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| KANJIDIC                            | kanji: Japanese, Chinese and Korean readings, English meanings; no source language, as it has all three                         | [scriptin/jmdict-simplified](https://github.com/scriptin/jmdict-simplified) (JSON), converted by `add`                                                                                                                                                  | [KANJIDIC2](https://www.edrdg.org/wiki/index.php/KANJIDIC_Project) (EDRDG)                                        |
| Pixiv                               | Japanese encyclopedia entries                                                                                                   | [MarvNC/pixiv-yomitan](https://github.com/MarvNC/pixiv-yomitan)                                                                                                                                                                                         | [pixiv百科事典](https://dic.pixiv.net/)                                                                           |
| `ja`/`zh`/`ko.Wikipedia`            | Wikipedia articles' short abstracts                                                                                             | [MarvNC/wikipedia-yomitan](https://github.com/MarvNC/wikipedia-yomitan)                                                                                                                                                                                 | Wikipedia, via [DBpedia](https://dbpedia.org/)'s short-abstract dumps                                             |
| CC-CEDICT                           | Chinese–English                                                                                                                 | [MarvNC/cc-cedict-yomitan](https://github.com/MarvNC/cc-cedict-yomitan)                                                                                                                                                                                 | [CC-CEDICT](https://cc-cedict.org/wiki/)                                                                          |
| CC-CEDICT.Hanzi                     | Chinese characters: pinyin, English meanings, for the Characters view (a kanji dictionary)                                      | [MarvNC/cc-cedict-yomitan](https://github.com/MarvNC/cc-cedict-yomitan)                                                                                                                                                                                 | [CC-CEDICT](https://cc-cedict.org/wiki/)                                                                          |
| KO-EN.KRDICT                        | Korean–English                                                                                                                  | [Lyroxide/yomitan-ko-dic](https://github.com/Lyroxide/yomitan-ko-dic)                                                                                                                                                                                   | the National Institute of Korean Language's [Basic Korean Dictionary](https://krdict.korean.go.kr/eng/mainAction) |
| Kanjium                             | Japanese pitch accents (about 124,000 words)                                                                                    | converted by `add` from `accents.txt`                                                                                                                                                                                                                   | [Kanjium](https://github.com/mifunetoshiro/kanjium) (CC BY-SA 4.0)                                                |
| Hanja                               | Korean Hanja: Hangul readings and meaning-readings (훈음), about 7,400 characters, for the Characters view (a kanji dictionary) | converted by `add` from `hanja.txt`                                                                                                                                                                                                                     | [libhangul](https://github.com/libhangul/libhangul) (BSD-3-Clause)                                                |
| `wty-ja-en`/`wty-ko-en`/`wty-zh-en` | Japanese, Korean and Chinese–English                                                                                            | [yomidevs/wiktionary-to-yomitan](https://github.com/yomidevs/wiktionary-to-yomitan), released on [Hugging Face](https://huggingface.co/datasets/daxida/wty-release) (its [download page](https://yomidevs.github.io/wiktionary-to-yomitan/) links them) | Wiktionary, via [kaikki.org](https://kaikki.org/) ([wiktextract](https://github.com/tatuylonen/wiktextract))      |

These dictionaries aren't part of cjk-dict:

- cjk-dict doesn't include or redistribute them; `dict-admin` downloads them from the projects above.
- Each belongs to its authors and keeps its own license and terms; see its project. cjk-dict isn't affiliated with them.
- The app's About page credits them ([app.md](app.md#about)): a new default needs its credit in `app/src/util/credits.ts` too.
- Check those terms before sharing a build or the library with anyone (see [Security](../README.md#security)).
- Kanjium asks apps that use it to credit its additions, e.g. on an About page, which the app's About page does ([app.md](app.md#about)): "The pitch accent notation, verb particle data, phonetics, homonyms and other additions or modifications to EDICT, KANJIDIC or KRADFILE were provided by Uros O. through his free database."

Options and behavior:

- `defaults` adds those not in the library yet, skipping the rest; `--replace` downloads them again, e.g. to update to the latest release.
- `--only <name>` (repeatable) picks some of them; `--list` prints them with their language overrides and download URLs.
- It records each one's language overrides in `dicts.json`, where the dictionary's own `index.json` falls short.
- GitHub-hosted ones resolve to the asset of the repo's latest release. Set `GITHUB_TOKEN` if you hit the API's unauthenticated rate limit.
- Kanjium, Hanja and the wty dictionaries have fixed URLs: Kanjium's is `accents.txt` on its repo's `master`, Hanja's `data/hanja/hanja.txt` on libhangul's `main`.

## `build` and `rebuild`

- `build` updates `search.db` incrementally:
  - It indexes new or changed dictionaries, and removes deleted ones.
  - It hashes the zips against the previous manifest to detect replacements.
- `rebuild` discards the current index and rebuilds it from scratch.
- Both write `search.db.zst`, `search.db` zstd-compressed (level 19), for the app to download:
  - compressed again only when `search.db` changed since the last one (the manifest has its unpacked checksum);
  - about a minute for a 1.3 GB `search.db`, which becomes \~310 MB.
- `search.db` records its compatibility level (`meta.compat`, e.g. `1`), and the manifest copies it (`search_db_zst.compat`):
  - the level is `SEARCH_DB_COMPAT` in `yomitan-parser/src/search.rs`: an app reads a `search.db` of that level or lower;
  - an app that reads a lower level keeps the library it has (see [app.md](app.md#downloading-the-library)), so a device needn't get every new app version;
  - it's bumped only when an older app would read the new `search.db` wrongly: a table or column dropped or renamed, or data stored differently;
  - new tables, columns and `meta` keys don't bump it: older apps don't query them;
  - `build` records it only when it differs from what `search.db` says (none means 1), so an unchanged library keeps the same `search.db`.
- Only terms, and where kanji entries are, are indexed. Definitions, kanji meta, frequencies and media stay in the zips.
- Pitch accents (term meta of mode `pitch`, e.g. Kanjium's) are copied into `search.db`'s `term_pitch`, every entry, with its term and reading as the dictionary has them, so a search returns them with the words. An existing `search.db` gets those of the dictionaries it has on the next `build`.
- Terms are tokenized with [lindera](https://github.com/lindera/lindera), so that `~` finds words containing them.
  - Its unidic, ko-dic and cc-cedict dictionaries, the same as the app's, are downloaded from [lindera's 6.2.0 release](https://github.com/lindera/lindera/releases/tag/v6.2.0) to `tmp/lindera` when indexing is needed (`--lindera-dir`, `CJK_DICT_LINDERA_DIR`).

### Characters

`build` also indexes dictionaries' kanji banks: single characters only, e.g. KANJIDIC's, or CC-CEDICT.Hanzi's, which also has whole words there.

It fills `search.db`'s character tables from the [CJKVI IDS database](https://github.com/cjkvi/cjkvi-ids)'s `ids.txt`:

- how each character breaks down into components;
- which characters each component is in;
- its Hangul readings (eum), as a character dictionary lists them (see [Korean Hanja readings](#korean-hanja-readings));
- how many terms use it: in all, and of each language (Japanese, Chinese, Korean), for the Characters view's counts and "most used first";
  - a term counts in each language Words finds it in, so one from a dictionary of no language counts in all three;
- each character's own readings (`han_reading`), for the Characters view's search by reading:
  - from the kanji dictionaries: on, kun and name readings in kana, KANJIDIC's pinyin and Korean, CC-CEDICT.Hanzi's pinyin;
  - and its Hangul readings above.

It fills `han_variant` from [Unihan](https://www.unicode.org/reports/tr38/)'s `Unihan_Variants.txt`: each character's variant forms, for the Characters view.

| Kind          | Unihan field                               | Example              |
| ------------- | ------------------------------------------ | -------------------- |
| `traditional` | `kTraditionalVariant`                      | 学 → 學, 钟 → 鍾, 鐘 |
| `simplified`  | `kSimplifiedVariant`                       | 學 → 学              |
| `kyujitai`    | `kJapaneseOldVariant`                      | 広 → 廣              |
| `shinjitai`   | `kJapaneseNewVariant`                      | 廣 → 広              |
| `semantic`    | `kSemanticVariant`                         | 學 → 斈              |
| `other`       | `kSpecializedSemanticVariant`, `kZVariant` | 繪 → 繢              |

- A pair related in several ways has a row for each: 學 is 学's `traditional` and `kyujitai`.
- Left out: a character as its own variant (万's traditional forms are 万 and 萬), and `kSpoofingVariant` (characters that only look alike).
- Semantic variants are loose: many pairs aren't interchangeable in a given language.

Notes:

- Japanese words' character readings need a kanji dictionary with Japanese readings. `build` warns when there is none; KANJIDIC is one of the defaults.
- `ids.txt` is downloaded to `tmp/cjkvi-ids` (`--ids-dir`, `CJK_DICT_IDS_DIR`); delete it to get the latest.
- `Unihan_Variants.txt` is taken from `Unihan.zip` (8 MB, the latest Unicode release on unicode.org) into `tmp/unihan` (`--unihan-dir`, `CJK_DICT_UNIHAN_DIR`); the zip isn't kept. Delete the file to get the latest.
- The tables are refilled only when `ids.txt`, `Unihan_Variants.txt`, the dictionaries or how `dict-admin` fills them change, so an unchanged library keeps the same `search.db`.
  - `han_variant` follows `Unihan_Variants.txt` only, not the dictionaries.

### Korean Hanja readings

Korean headwords written in Hanja get Hangul readings when `search.db` is built (光速 → 광속, 光나다 → 광나다), since Korean dictionaries give none.

- They come from [hanja_hangul](https://crates.io/crates/hanja_hangul):
  - a reading per character;
  - the initial-sound rule (李 → 이);
  - a table of dictionary words that read otherwise.
- Of a character's several readings (樂: 락, 낙, 악, 요), the one that is also a Hangul headword is kept.
- `build` fills in the readings of an existing `search.db` too.
- A character's own readings (the Characters view) are different: those a character dictionary lists, not every way it reads in words.
  - hanja_hangul's main reading comes first, then the others from [libhangul](https://github.com/libhangul/libhangul)'s `hanja.txt` (BSD-3-Clause): 樂 악 락 요.
  - A reading the initial-sound rule makes from another is left out, unless it's the main reading: 樂 락, not also 낙; 李 리.
  - The rule goes one way (ㄹ → ㄴ → ㅇ), so an ㄹ or ㄴ reading beside an ㄴ or ㅇ main reading is kept, e.g. a customary reading: 諾 낙 락 (受諾 수락), 怒 노 로 (喜怒哀樂 희로애락).
  - Not hanja_hangul's other readings, which are candidates for reading words, not the character's own:
    - readings of a few words: 道 is 도, not also 또 (使道 사또);
    - forms before the initial-sound rule, which hanja_hangul applies itself: 二 is 이, not also 니, 리 (二世安樂 is 이세안락).
  - libhangul's readings are in `yomitan-parser/data/hanja_eum.txt`, made by `node scripts/hanja-eum.mjs`.
- hanja_hangul's word tables are CC BY-SA 2.0 KR (derived from the National Institute of Korean Language's dictionaries). Only `dict-admin` uses it (yomitan-parser's `hangul-readings` feature), not the app.

## `remove` and `list`

- `remove <name>` takes a dictionary out of `dicts.json` and deletes its zip. Run `build` and `upload` to publish that.
- `list` shows each dictionary:
  - its source and target languages (L1, L2): as built, or as `dicts.json` sets them before a build; `*` marks those set in `dicts.json`;
  - its zip's size;
  - its terms, characters and pitch accents, or `not built`;
  - where it came from.

## `size`

- Estimates the whole library's R2 mirror size, grouping dictionary zips by language and counting shared files once.
- `--lang` filters the zip breakdown only; it does not change what `upload` sends.
- Run it after `build`. Removing dictionaries and rebuilding lets you compare the updated estimate.

## `upload`

- Refuses a stale build.
- Copies the zips first, then `search.db.zst`, then `manifest.json`, so the app never sees a manifest pointing at missing files.
- `search.db` itself isn't uploaded.
- Then deletes from R2 what was removed from the library.
- It runs rclone, so the destination can be another S3-compatible service; only R2 is tested (see the [README](../README.md#other-s3-compatible-buckets-not-tested)).
- Behind a TLS-intercepting proxy, set `SSL_CERT_FILE`.
