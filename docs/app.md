# The app

How the app gets the library and shows entries. The [README](../README.md#app) has how to run and build it; [search.md](search.md) has how searching works.

## Downloading the library

- On start, the app compares the remote `manifest.json` with its own copy, and downloads only the changed files.
- Downloads run in the background and resume where they stopped.
- Files go to a staging folder, and replace the old ones only when all of them have arrived and their checksums match. Until then, the app keeps searching the previous version.
- It signs its R2 requests itself, with presigned URLs.
- On first launch, it also downloads the [lindera](https://github.com/lindera/lindera) tokenizer dictionaries in the background, used to tokenize `~` queries:
  - unidic (Japanese), ko-dic (Korean) and cc-cedict (Chinese), from [lindera's 6.2.0 release](https://github.com/lindera/lindera/releases/tag/v6.2.0);
  - \~73 MB to download, \~310 MB unpacked.
- It also downloads fonts for the Han characters the system's fonts lack, such as rare components in a character's breakdown:
  - [HanaMinAFDKO](https://github.com/cjkvi/HanaMinAFDKO) 8.030 (Hanazono Mincho, [GlyphWiki license](https://glyphwiki.org/wiki/GlyphWiki:License)), as the [CJKVI IDS](https://github.com/cjkvi/cjkvi-ids#description) data recommends;
  - three files to `fonts/` in the app data folder, \~77 MB:

| File           | Size    | Covers                                                                                                                             |
| -------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `HanaMinA.otf` | 31.6 MB | Extension A, the main CJK block's additions after U+9FA5 (鿐), compatibility ideographs, radicals, strokes, Hangzhou numerals (〢) |
| `HanaMinB.otf` | 32.2 MB | Extension B (𠀀)                                                                                                                   |
| `HanaMinC.otf` | 13.0 MB | Extensions C to F (𬄋), the compatibility supplement                                                                               |

- The fonts come after the system's own (`font-family: sans-serif, "HanaMin"`):
  - a listed font is tried before the system's own fallback: where the first font has no Han characters (e.g. Segoe UI, with no CJK `lang`), HanaMin would draw every one, in a serif style;
  - so they leave out what every system CJK font has, the main CJK block up to U+9FA5 and 々〆〇: those keep the system's sans-serif fonts and their language's forms;
  - each file is read only for the characters it covers (`unicode-range`), and only once one is shown;
  - each is used as soon as it's downloaded, without a restart.
- They cover every character the IDS data uses: it goes up to Extension F, as the fonts do. Characters of later extensions (G and up) in a dictionary are drawn only if the system has a font for them.

## Showing entries

- Definitions render as in Yomitan:
  - structured content: only the schema's elements and styles, `data` as `data-sc-*` attributes;
  - each dictionary's own `styles.css`, scoped to its entries;
  - images sized from `width`/`height`/`sizeUnits`, including monochrome images.
- Where the app differs from Yomitan, for example showing images that dictionaries mark collapsed, is listed in [yomitan-differences.md](yomitan-differences.md).
- Images in definitions are served from the zips through a `dict://` protocol.
- Pitch accents (from pitch accent dictionaries, e.g. Kanjium) are found and shown above a word's entries as in Yomitan (see [yomitan-differences.md](yomitan-differences.md#pitch-accents)):
  - those of its entries' terms and readings; an entry without a reading is read as written;
  - by dictionary: its tag, then each pitch with its tags (their notes in a tooltip), the terms it is for when not all of the word's, the reading and the downstep, e.g. `[2]`;
  - the reading has a line over the high morae that turns down where the pitch drops, a small circle on nasal morae (鼻濁音) and a dotted one around devoiced morae;
  - the headword has its patterns as `data-pronunciation-categories`;
  - pitch accent dictionaries are in the **Dictionaries** list, to leave out like any other.
- Terms and definitions are marked with their dictionary's languages (`lang`), so Han characters take the forms of their language (see [dict-admin](dict-admin.md#languages)).
  - So are dictionary names, in the **Dictionaries** list and on entries: with the terms' language.
- The app follows the device's light or dark color-scheme preference.
- Bold, e.g. on tags, is made up from the regular face where a font has no bold (`font-synthesis: weight`), as Android's CJK font often has none. Italics aren't made up.
- Selecting text (e.g. a long press on Android):
  - an entry's headword selects as a whole, without its reading;
  - tags (`.tag`) and elements with a tooltip can't be selected. They sit next to text with no space between, so a word selected beside one would take its text too, e.g. `vt` + `to` as "vtto";
  - elsewhere, the system picks the word. For Chinese and Japanese, it's often a single character.

## Tooltips

- Buttons and labels explain themselves with tooltips (`v-tooltip`, [`app/src/util/tooltip.ts`](../app/src/util/tooltip.ts)), in place of `title`, which a touchscreen never shows.
- On a device without hover (`(hover: none)`, e.g. a phone), the search hint says, before anything is typed, to long-press a button or option to see what it does.
  - In **Words**, it leaves out "or press `{}` to search by reading only" there, to save room: a long press on `{}` explains it.
- They follow [WCAG 2.1 SC 1.4.13](https://www.w3.org/WAI/WCAG21/Understanding/content-on-hover-or-focus.html) and the [ARIA Authoring Practices' tooltip pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tooltip/).

| Input        | Shows                                  | Hides                                               |
| ------------ | -------------------------------------- | --------------------------------------------------- |
| Mouse or pen | after resting on the element for 0.4 s | when the pointer leaves the element and the tooltip |
| Keyboard     | while the element has keyboard focus   | when focus moves, or on Esc                         |
| Touch        | on a long press                        | on the next touch elsewhere, a scroll, or Esc       |

- Nothing hides a tooltip on a timer: it stays until dismissed.
- The pointer can move onto the tooltip (e.g. with a screen magnifier): there are 0.15 s to cross the gap.
- 0.4 s is a convention, about as long as a browser's own `title` tooltip; no guideline sets it.
- A long press:
  - is the platform's own where the webview reports it (`contextmenu`): Android and Windows. It then follows the system's delay, e.g. Android's Accessibility → Touch & hold delay, which Android's webview reads when the app starts;
  - is timed at 0.5 s elsewhere, e.g. WebKit on iOS. A webview that turns out to report long presses is then waited for instead;
  - doesn't count as a tap, and opens no system menu (text selection, "open link");
  - is not a swipe: moving the finger more than 10 px shows nothing.
- The tooltip goes above what it explains, where a finger doesn't cover it, or below when there's no room.
- It's dark in both themes: in the dark theme, lighter than the page and with a border, so it stands out from it.
- Screen readers read the text as the element's description (`aria-describedby`, to a hidden element), whether or not the tooltip is shown; the bubble itself is hidden from them, so it isn't read twice.
- Dictionaries' own `title`s, in definitions and on images, are tooltips too.

## Security policy

- The CSP allows fonts from the `font:` protocol (`font-src`), which serves only font files from `fonts/` in the app data folder.

The CSP allows inline styles (`style-src 'unsafe-inline'`), used for:

- the download progress indicator;
- structured content styles;
- dictionaries' `styles.css`.
