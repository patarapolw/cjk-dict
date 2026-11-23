# Differences from Yomitan

The app renders definitions the way [Yomitan](https://github.com/yomidevs/yomitan) does, so that dictionaries look as their authors intended. The renderer is in [`app/src/components/yomitan/`](../app/src/components/yomitan/); its CSS follows Yomitan's `structured-content.css` and `display.css`. This page lists where it deliberately differs, and why.

Keep differences few, and add each new one here.

## Images

|                                          | Yomitan                                                    | cjk-dict                                                                                | Why                                                                                                                                               |
| ---------------------------------------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Images marked `collapsed`                | Shown as `[Image]`. Clicking opens the image in a new tab. | Shown expanded. Tapping one collapses it to `[Image]`, and tapping that shows it again. | A Japanese dictionary marks all of its illustrations collapsed, so they were hidden behind a link.                                                |
| `width` and `height` the wrong way round | Used as given.                                             | Swapped when both are given and the image's own shape matches them swapped.             | Some images in vertical-text (`縦中横`) spans, such as fractions, give the size for vertical text. Used as given, they shrink to a 0.25em sliver. |
| Media paths written with `\`             | Not checked.                                               | The path is tried as given, then with `\` replaced by `/`.                              | The ja, zh and ko Wikipedia dictionaries refer to `assets\wikipedia-icon.png`, but their zips store `assets/wikipedia-icon.png`.                  |

## Pitch accents

The layout follows Yomitan's: dictionaries in a numbered list, each with its tag (in Yomitan's color for pronunciation dictionaries), then its pitches; the reading with a line over the high morae that turns down at the drop, nasal and devoiced morae marked; then the downstep, e.g. `[2]`. It differs in:

|                        | Yomitan                                                               | cjk-dict                                            | Why                                                                                |
| ---------------------- | --------------------------------------------------------------------- | --------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Forms                  | Text and downstep by default; a graph, and each form, set in Settings | Text and downstep only                              | The app has no settings for it yet.                                                |
| Where                  | In each entry                                                         | Once per word, above its entries                    | The app shows a word's entries from every dictionary together, under one headword. |
| A word with no reading | Pitches whose reading is the word itself                              | Pitches of every reading, each drawn on its reading | Wikipedia's and Pixiv's entries have no readings: they would get none.             |
| The pattern's name     | On the headword, as `data-pronunciation-categories`, for custom CSS   | In a tooltip on the reading                         | Learners can see it; the app has no custom CSS.                                    |

### Same result, different implementation

These differ in how they work, not in what you see. They're listed so that a rendering difference can be traced back to them.

- **Image sizes.** Yomitan's importer stores a dictionary's `width`/`height` as `preferredWidth`/`preferredHeight`, and the image file's pixel size as `width`/`height`. The app reads dictionaries as they are, without an import step. So it treats `width`/`height` as the preferred size, and gets the pixel size from the image once it loads. Until then, an image the dictionary gives no size for takes a 100×100 px placeholder.
- **Monochrome images** (`appearance: "monochrome"`) are drawn as a CSS mask in the text color. Yomitan draws a canvas and recolors it with CSS filters.
- **`title`** on structured content (e.g. a tag's explanation) and on images shows as the app's own tooltip, not the browser's, so that a touchscreen shows it too: on a long press (see [app.md](app.md#tooltips)).
- **Media** is served from the dictionary zips through the `dict:` protocol. The responses allow any origin (`Access-Control-Allow-Origin: *`), because browsers fetch CSS mask images with CORS.
