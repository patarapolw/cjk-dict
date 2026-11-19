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

## Showing entries

- Definitions render as in Yomitan:
  - structured content: only the schema's elements and styles, `data` as `data-sc-*` attributes;
  - each dictionary's own `styles.css`, scoped to its entries;
  - images sized from `width`/`height`/`sizeUnits`, including monochrome images.
- Where the app differs from Yomitan, for example showing images that dictionaries mark collapsed, is listed in [yomitan-differences.md](yomitan-differences.md).
- Images in definitions are served from the zips through a `dict://` protocol.
- Terms and definitions are marked with their dictionary's languages (`lang`), so Han characters take the forms of their language (see [dict-admin](dict-admin.md#languages)).
- The app follows the device's light or dark color-scheme preference.

## Tooltips

- Buttons and labels explain themselves with tooltips (`v-tooltip`, [`app/src/util/tooltip.ts`](../app/src/util/tooltip.ts)), in place of `title`, which a touchscreen never shows.
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
  - is the platform's own where the webview reports it (`contextmenu`): Android and Windows. It then follows the system's delay, e.g. Android's Accessibility → Touch & hold delay;
  - is timed at 0.5 s elsewhere, e.g. WebKit on iOS. A webview that turns out to report long presses is then waited for instead;
  - doesn't count as a tap, and opens no system menu (text selection, "open link");
  - is not a swipe: moving the finger more than 10 px shows nothing.
- The tooltip goes above what it explains, where a finger doesn't cover it, or below when there's no room.
- Screen readers read the text as the element's description (`aria-describedby`, to a hidden element), whether or not the tooltip is shown; the bubble itself is hidden from them, so it isn't read twice.
- Dictionaries' own `title`s, in definitions and on images, are tooltips too.

## Security policy

The CSP allows inline styles (`style-src 'unsafe-inline'`), used for:

- the download progress indicator;
- structured content styles;
- dictionaries' `styles.css`.
