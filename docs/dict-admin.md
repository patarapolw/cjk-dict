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

## `defaults`

The default dictionaries (the list the app used to download itself), each downloaded from the project that converts it to Yomitan:

| Name                                | What                                                                                                    | Yomitan conversion                                                                                                                                                                                                                                      | From                                                                                                              |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| KANJIDIC                            | kanji: Japanese, Chinese and Korean readings, English meanings; no source language, as it has all three | [scriptin/jmdict-simplified](https://github.com/scriptin/jmdict-simplified) (JSON), converted by `add`                                                                                                                                                  | [KANJIDIC2](https://www.edrdg.org/wiki/index.php/KANJIDIC_Project) (EDRDG)                                        |
| Pixiv                               | Japanese encyclopedia entries                                                                           | [MarvNC/pixiv-yomitan](https://github.com/MarvNC/pixiv-yomitan)                                                                                                                                                                                         | [pixiv百科事典](https://dic.pixiv.net/)                                                                           |
| `ja`/`zh`/`ko.Wikipedia`            | Wikipedia articles' short abstracts                                                                     | [MarvNC/wikipedia-yomitan](https://github.com/MarvNC/wikipedia-yomitan)                                                                                                                                                                                 | Wikipedia, via [DBpedia](https://dbpedia.org/)'s short-abstract dumps                                             |
| CC-CEDICT                           | Chinese–English                                                                                         | [MarvNC/cc-cedict-yomitan](https://github.com/MarvNC/cc-cedict-yomitan)                                                                                                                                                                                 | [CC-CEDICT](https://cc-cedict.org/wiki/)                                                                          |
| KO-EN.KRDICT                        | Korean–English                                                                                          | [Lyroxide/yomitan-ko-dic](https://github.com/Lyroxide/yomitan-ko-dic)                                                                                                                                                                                   | the National Institute of Korean Language's [Basic Korean Dictionary](https://krdict.korean.go.kr/eng/mainAction) |
| Kanjium                             | Japanese pitch accents (about 124,000 words)                                                            | converted by `add` from `accents.txt`                                                                                                                                                                                                                   | [Kanjium](https://github.com/mifunetoshiro/kanjium) (CC BY-SA 4.0)                                                |
| `wty-ja-en`/`wty-ko-en`/`wty-zh-en` | Japanese, Korean and Chinese–English                                                                    | [yomidevs/wiktionary-to-yomitan](https://github.com/yomidevs/wiktionary-to-yomitan), released on [Hugging Face](https://huggingface.co/datasets/daxida/wty-release) (its [download page](https://yomidevs.github.io/wiktionary-to-yomitan/) links them) | Wiktionary, via [kaikki.org](https://kaikki.org/) ([wiktextract](https://github.com/tatuylonen/wiktextract))      |

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
- Kanjium and the wty dictionaries have fixed URLs: Kanjium's is `accents.txt` on its repo's `master`.

## `build` and `rebuild`

- `build` updates `search.db` incrementally:
  - It indexes new or changed dictionaries, and removes deleted ones.
  - It hashes the zips against the previous manifest to detect replacements.
- `rebuild` discards the current index and rebuilds it from scratch.
- Only terms, and where kanji entries are, are indexed. Definitions, kanji meta, frequencies and media stay in the zips.
- Pitch accents (term meta of mode `pitch`, e.g. Kanjium's) are copied into `search.db`'s `term_pitch`, every entry, with its term and reading as the dictionary has them, so a search returns them with the words. An existing `search.db` gets those of the dictionaries it has on the next `build`.
- Terms are tokenized with [lindera](https://github.com/lindera/lindera), so that `~` finds words containing them.
  - Its unidic, ko-dic and cc-cedict dictionaries, the same as the app's, are downloaded from [lindera's 6.2.0 release](https://github.com/lindera/lindera/releases/tag/v6.2.0) to `tmp/lindera` when indexing is needed (`--lindera-dir`, `CJK_DICT_LINDERA_DIR`).

### Characters

`build` also indexes dictionaries' kanji banks: single characters only, e.g. KANJIDIC, or the Hanzi edition of CC-CEDICT, which also has whole words there.

It fills `search.db`'s character tables from the [CJKVI IDS database](https://github.com/cjkvi/cjkvi-ids)'s `ids.txt`:

- how each character breaks down into components;
- which characters each component is in;
- its Hangul readings (eum), as a character dictionary lists them (see [Korean Hanja readings](#korean-hanja-readings));
- how many terms use it.

Notes:

- Japanese words' character readings need a kanji dictionary with Japanese readings. `build` warns when there is none; KANJIDIC is one of the defaults.
- `ids.txt` is downloaded to `tmp/cjkvi-ids` (`--ids-dir`, `CJK_DICT_IDS_DIR`); delete it to get the latest.
- The tables are refilled only when `ids.txt`, the dictionaries or how `dict-admin` fills them change, so an unchanged library keeps the same `search.db`.

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
- Copies the zips first, then `search.db`, then `manifest.json`, so the app never sees a manifest pointing at missing files.
- Then deletes from R2 what was removed from the library.
- Behind a TLS-intercepting proxy, set `SSL_CERT_FILE`.
