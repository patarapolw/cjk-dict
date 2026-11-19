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

## Security policy

The CSP allows inline styles (`style-src 'unsafe-inline'`), used for:

- the download progress indicator;
- structured content styles;
- dictionaries' `styles.css`.
