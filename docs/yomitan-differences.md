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

Pitch accents are found and shown as Yomitan does, with its default settings:

- found by an entry's term and reading exactly; an entry without a reading is read as written, so 端 with no reading gets none of はし's or はた's pitches;
- every entry a dictionary has for the word, the same pitch of the same reading once;
- once per result, above its entries, as Yomitan's default result mode (`group`) shows them once per word;
  - a result can have a word in katakana and in hiragana (チリチリ, ちりちり), which Yomitan's results don't: such spellings and readings count as one, so a pitch of ちりちり isn't marked as for ちりちり only;
- dictionaries in a numbered list, each with its tag in Yomitan's color for pronunciation dictionaries, then its pitches:
  - tags, with what the dictionary's tag banks say;
  - the terms a pitch is for, when not all of the result's;
  - the reading, with a line over the high morae that turns down at the drop, and nasal and devoiced morae marked;
  - the downstep, e.g. `[2]`;
- the headword's patterns (heiban, atamadaka, nakadaka, odaka, kifuku) as `data-pronunciation-categories`.

It differs in:

|       | Yomitan                                                                       | cjk-dict               | Why                                             |
| ----- | ----------------------------------------------------------------------------- | ---------------------- | ----------------------------------------------- |
| Forms | Text and downstep by default; a graph, and each of the three, set in Settings | Text and downstep only | The app has no settings for them yet (to come). |

### Same result, different implementation

These differ in how they work, not in what you see. They're listed so that a rendering difference can be traced back to them.

- **Image sizes.** Yomitan's importer stores a dictionary's `width`/`height` as `preferredWidth`/`preferredHeight`, and the image file's pixel size as `width`/`height`. The app reads dictionaries as they are, without an import step. So it treats `width`/`height` as the preferred size, and gets the pixel size from the image once it loads. Until then, an image the dictionary gives no size for takes a 100×100 px placeholder.
- **Monochrome images** (`appearance: "monochrome"`) are drawn as a CSS mask in the text color. Yomitan draws a canvas and recolors it with CSS filters.
- **`title`** on structured content (e.g. a tag's explanation) and on images shows as the app's own tooltip, not the browser's, so that a touchscreen shows it too: on a long press (see [app.md](app.md#tooltips)).
- **Pitch accents** come from `search.db`, where its build copies them (see [dict-admin.md](dict-admin.md#build-and-rebuild)), rather than from an import into a database of every term meta entry, as in Yomitan.
- **Media** is served from the dictionary zips through the `dict:` protocol. The responses allow any origin (`Access-Control-Allow-Origin: *`), because browsers fetch CSS mask images with CORS.
