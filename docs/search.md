# Searching

How the app's search works. The [README](../README.md#app) has how to run the app.

## The search box

- One search box. The search language in the header narrows the search to one language; it is remembered per device, as are the [dictionaries chosen](#choosing-dictionaries).
- Search modes:
  - prefix (default);
  - `=` exact;
  - `~` token search. It needs the tokenizer; until that has downloaded, `~` is disabled and falls back to prefix search.
- The `{}` button searches by reading only: type the reading, no `{}` needed.
- A space marks a word boundary: only the first word is looked up, and "Next" moves on to the rest.
- When nothing matches, the search is tried the other way, in every search language; the hint says so:
  - a word, by reading: `zhongguo` finds 中国, `にほんご` finds 日本語;
  - a reading alone (`{}`, or the `{}` button), as a word: `{sake}` finds sake, and `日本` typed with the `{}` button on finds 日本.
  - Not a pattern (`*`, `?`, `[...]`), `~`, a word with a reading of its own (`<日本>{にほん}`), nor Korean romanization, a reading already.
- When neither way matches a longer text (e.g. a copied sentence), it is split into words with the tokenizer.
- Each result links to:
  - an exact search for itself;
  - `~` words containing it;
  - words with the same reading;
  - its characters.
- In-dictionary links and the back button keep a search history.

## Readings

- Readings use furigana syntax: `<日本>{にほん}語` matches 日本語 read にほん…
- `{reading}` alone searches by reading, e.g. `{zhong1 guo2}`.
- Pinyin without tones (`{zhong guo}`) is expanded to every tone, for readings of up to four syllables; a longer one matches as typed.
- Katakana and hiragana find each other: `こーひー` finds コーヒー, and `<興>{キョウ}` finds 興味 (きょうみ).
  - `search.db` stores terms, tokens and readings with katakana as hiragana.
  - `dict-admin build` rebuilds a `search.db` from before this from scratch; the app searches one as before until then.

## Wildcards

In a term and in a term's reading:

- `*` any characters, `?` one character:
  - `日?` finds 日本 and 日米…
  - `<日>{に?}本` finds 日本 read にほん or にっぽん…
  - a leading `*` matches anywhere in the word: `*語` finds 日本語.
- `[...]` one of the characters:
  - in a term, `[...]` and `[^...]` (none of them): `[日月]曜日`;
  - in a term's reading and in a reading alone, `[...]`: `{にほ[んっ]`.
- Full-width brackets and wildcards, as a Japanese IME types them, work too: `＜＞｛｝＊？［］＾`.

Not supported:

- `*` or `?` in a reading alone: `{に?ほん}` is two reading words.
- Wildcards with `=` or `~`.

Without the term's index to narrow it, a search starting with a wildcard walks the terms by score: quick when matches are common, seconds on a big library when they are few.

## A character's reading

`*<興>{キョウ}` (one character, a reading without wildcards) finds the words with 興 read キョウ anywhere in them: 興味, 余興…

- It looks the character and reading up in `search.db`'s `term_char` index.
- `dict-admin build` fills the index by splitting each term's reading among its characters:
  - Chinese has a pinyin syllable per character, and Korean a Hangul syllable per Hanja.
  - Japanese uses the kanji dictionaries' readings (KANJIDIC's on, kun and name readings), so it needs one in the library.
  - It allows for compounds' sound changes (学校 がっこう: 学 is がく), so `*<学>{ガク}` finds 学校 too.
  - Words whose reading belongs to the whole word (馬酔木 あせび) are not in it.
- Korean readings match with the initial-sound rule either way: `*<林>{림}` finds 林業 임업.
- Without the index, in a `search.db` from before it, the same query is the wildcard search.

## Characters

The Characters view (`#/kanji?q=<characters>`) shows, for each Han character:

- **Variants**, from Unihan, each linking to its own page, with how it relates: 学 lists 學 (traditional, kyūjitai); 広 lists 廣 (kyūjitai);
  - traditional, simplified, kyūjitai or shinjitai; "semantic" (the same meaning, not always interchangeable) and "other" only when a variant is none of those;
- how it breaks down: each IDS, by region, the one for the search language first;
- every component, all the way down; components link to their own page;
- **Used in**: up to 30 characters made directly of it (one level up), the most used first: 口 lists 可, 呆, ... but not 河, made of 可;
  - "5 more" after them when there are more;
  - a radical also lists the characters written with its component forms: 水 lists 河;
  - **Search `*口`** below it searches `*口`, which shows them all, each in full (see below);
- its kanji dictionary entries: readings, meanings, and stats with the dictionary's own notes;
  - of the kanji dictionaries [chosen](#choosing-dictionaries), and only those of the search language and those that don't say theirs: KANJIDIC (Japanese, Chinese and Korean readings) in every language, CC-CEDICT.Hanzi (Chinese) with Chinese or all, Hanja (Korean) with Korean or all;
  - the search language's first;
  - with none shown, its variants' entries, each marked with the variant and how it relates, e.g. 钟 with Japanese shows 鍾's and 鐘's KANJIDIC entries; their readings search the words with the variant;
- Hangul readings;
- its single-character word entries.

Each reading searches the words with the character read so, anywhere in the word:

- on: `*<興>{キョウ}`;
- kun: `*<興>{おこ}`, up to the okurigana, which the dot marks (興る and 興す);
- pinyin: `*<興>{xīng}`;
- Hangul: `*<興>{흥}`.

A Chinese or Korean reading switches the search language to its own, unless it is Auto-detect.

Below, with several characters typed, the characters made of all of them, at any depth, the most used first: `木口` lists 呆, 杏, ...

`*` before the characters lists the characters made directly of them instead, each in full, as a card: `*口` shows 可, 呆, ..., 20 at a time, and **More** shows the next 20.

- `*木口`: made directly of both 木 and 口.
- A full-width `＊` works too.

In both:

- A radical also matches the forms it takes as a component: 水 finds 河, written with 氵.
- Kangxi radicals (`⼝`) are read as the ideograph.

### By reading

A reading instead of characters lists the characters read so, each in full, as a card, the most used first, 20 at a time: `きょう` shows 京, 強, 教, ...

- Text without Han characters is a reading; `{}` around it works too: `{xing}`.
- Kana: on, kun and name readings, katakana and hiragana alike.
  - A kun reading matches with or without its okurigana: `おこ` and `おこる` find 興.
  - As typed, not as a compound's sound change: `ぎょう` finds 行, not 京.
- Pinyin: `xīng`, `xing1`; without a tone, every tone.
- Hangul, with the initial-sound rule either way: `이` finds 李, listed as 리.
- Latin letters, by the search language:

  | Search language | Latin letters are                            | e.g.                                     |
  | --------------- | -------------------------------------------- | ---------------------------------------- |
  | Auto-detect     | romaji, pinyin and Korean romanization alike | `kyou` finds 京, `heung` finds 興        |
  | Japanese        | romaji, or else pinyin                       | `kyou` finds 京, `okoru` finds 興        |
  | Chinese         | pinyin                                       | `xing` finds 興                          |
  | Korean          | Korean romanization, or else pinyin          | `heung` finds 興, `xīng` is still pinyin |
  - Romaji is looked up in hiragana. A long vowel's macron is spelled as kana spell it: `kyō` is きょう.
  - Korean romanization is Revised Romanization, as in [Words](#korean):
    - with the same loose spellings: `eu`/`u`, `r`/`l`, ...; `hung` finds 興 too;
    - with the initial-sound rule: `i` finds 李.
  - The hint says what the reading was taken as: "Characters read kyou as きょう, pinyin or Korean romanization".
  - With Auto-detect, the characters of every way are listed together, the most used first.

- The readings are the kanji dictionaries' (KANJIDIC, CC-CEDICT.Hanzi) and the cards' Hangul readings, whichever dictionaries are ticked.
- With a `search.db` from before this, the view says to update the library.

## Korean

- With Korean selected, typing Revised Romanization finds Hangul: `hangugeo`, `hangukeo` or `hangug-eo` → 한국어.
- Terms are romanized at search time, both:
  - as pronounced: 국물 → gungmul, 신라 → silla;
  - letter by letter: gukmul.
- Loose spellings match: `eo`/`o`, `eu`/`u`, `ae`/`e`, `k`/`g`, `t`/`d`, `p`/`b`, `r`/`l`.
- `search.db` keeps these romanization keys in `term_roman`, so each keystroke is an index lookup.
  - `dict-admin build` adds the table to an existing `search.db`.
  - A `search.db` built before that still works, more slowly, by romanizing the terms as it searches.
- Hanja headwords get Hangul readings from `dict-admin` (see [dict-admin](dict-admin.md#korean-hanja-readings)). A Hangul word is its own reading, so:
  - `광속`, `{광속}`, `gwangsok` and `{gwangsok}` find both 광속 and 光速;
  - `<光速>{광속}` finds 光速.
- With **Search by romanization** on, a reading alone in romanization (`{gwangsok}`, or `gwangsok` with the `{}` button) is searched as romanization too.
  - The romanization keys come from the Hangul readings, so it is a search by reading either way.
  - A reading after a term (`<光速>{gwangsok}`) is not: write it in Hangul.

## Input options

Optional, below the search box:

- **Auto-convert to kana**: romaji become kana as you type. A syllable still being typed searches every kana it can become, by the converter's extended rules and with obsolete kana such as ゐ:
  - `にほんg` is `にほん[がぎぐげごっ]`;
  - `{にほn` is `{にほ[なにぬねのん]`;
  - `<食>{たb}べる` is `<食>{た[っばびぶべぼ]}べる`.
  - Words doesn't otherwise match romaji to kana, unlike the Characters view's [search by reading](#by-reading):
    - Latin letters mean something in Words: a Latin headword, Korean romanization or pinyin (`sake`, `rin`); matching them to kana would quietly change what a search means.
    - This option already does it, and shows the kana in the search box.
    - Romaji leaves long vowels unclear (`kyo`: きょ or きょう; `ono`: おの or おおの): cheap for a character's few readings, but many unwanted results across words.
    - Korean romanization is matched (see [Korean](#korean)), since Hangul has no input method as common as a Japanese IME.
- **Get reading from IME**: a Japanese IME's reading is kept as `<漢字>{かんじ}` when you convert.
- **Clipboard monitor**: checks the clipboard every second, and searches copied text that starts with Chinese, Japanese or Korean.
  - It searches in the normal mode: it turns off `~`, `=` and `{}`.
  - It stays on when you go to Characters and back to Words, until the app closes.
    - It doesn't check the clipboard while Characters is shown.
    - Back in Words, it doesn't search again the text it last searched, only a new copy.

## Choosing dictionaries

**Dictionaries** below the search box lists the word dictionaries, each with a checkbox:

- Which are listed:
  - with a search language chosen, only that language's, and those that don't say theirs;
  - kanji dictionaries (no words) aren't listed;
  - with only one to list, there is no list, unless it is unticked.
- They are ordered by language: those that don't say theirs first, then by language tag (`ja-JP`, `ko-KR`, `zh`, `zh-TW`), each language's in build order.
- An unticked dictionary is left out of word searches:
  - its entries aren't shown;
  - a word only it has isn't found.
- It counts the ones searched, e.g. "(3 of 5)": when some are unticked, or when opened.
- At least one stays ticked: the last one ticked can't be unticked.
- The choice is kept as you switch the search language, so every dictionary of a language may be unticked. The app then says so, and leaves the choosing to you.
- **Search all** ticks those listed again.
- The choice is remembered per device, as the dictionaries left out: one added to the library later is searched.
- The Characters view has its own **Dictionaries**, of the kanji dictionaries, chosen the same way:
  - listed by the same language rule: KANJIDIC, which doesn't say its language, always; CC-CEDICT.Hanzi (Chinese) with Chinese or all; Hanja (Korean) with Korean or all;
  - an unticked one's entries aren't shown, and at least one stays ticked;
  - **Show all** ticks those listed again;
  - the choice is remembered per device, apart from Words'.
