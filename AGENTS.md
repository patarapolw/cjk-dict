# Agent instructions

## Branches

- Before pushing to `origin`, and before opening a pull request, rename the working
  branch to `<agent>/<description>`:
  - `<agent>` is the agent's name in lowercase, e.g. `claude`, `codex`, `copilot`.
  - `<description>` is a short, meaningful kebab-case summary of the change, e.g.
    `android-clipboard-html` or `fix-download-resume`, not a generated name like
    `ccr-1a2b3c4d-xyz`.
  - Rename even when the generated name already has the right prefix: a branch
    like `claude/sleepy-mayer-l3d1hm` still needs a meaningful description, e.g.
    `claude/untrack-kotlin-error-logs`.

  ```sh
  git branch -m claude/android-clipboard-html
  git push -u origin claude/android-clipboard-html
  ```

- If the generated branch was already pushed, push the renamed branch and delete the
  old remote branch (`git push origin --delete <old-name>`) only when nothing else,
  such as an open pull request, uses it.

## Formatting

- Before every commit, format every code file in the repository, not only the files
  you changed:

  ```sh
  pnpm format      # Prettier: TypeScript, Vue, JSON, Markdown, ... (see .prettierignore)
  cargo fmt --all  # Rust workspace
  # the Tauri app is its own Rust workspace, which the one above excludes
  cargo fmt --all --manifest-path app/src-tauri/Cargo.toml
  ```

- Commit formatting changes to files you didn't otherwise touch too (leftovers from
  earlier commits), in the same commit. Don't revert them.

## Checking the app

Run the frontend unit tests (Vitest, with Tauri's IPC mocked by `@tauri-apps/api/mocks`)
with `pnpm test`; they live next to the code as `app/src/**/*.test.ts`.

Check a UI change in the running app, not in a page made to look like it. On Windows the
app's webview is WebView2, which can expose the Chrome DevTools Protocol, and
[`scripts/webview.mjs`](scripts/webview.mjs) drives it with no dependencies:

```sh
# start the app with remote debugging (Git Bash; run it in the background).
# The first build takes a few minutes.
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm app start
# PowerShell: $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"; pnpm app start

curl -s http://127.0.0.1:9222/json/list                  # ready once it lists the page
node scripts/webview.mjs open "#/dictionary?q=逆数&qState=exact"
node scripts/webview.mjs eval "document.querySelectorAll('.gloss-image-link').length"
node scripts/webview.mjs screenshot shot.png [css-selector]
```

- The app uses hash routes: `#/dictionary?q=<query>&qState=exact|fts&reading=1`.
- It searches the dictionaries already downloaded to `%APPDATA%\cc.polv.cjk-dict`.
- `tauri dev` hot-reloads frontend changes, and rebuilds and restarts the app on Rust
  changes. Stop the app when you're done: stopping `pnpm app start` can leave
  `cjk-dict-app.exe` and Vite (port 1420) running, so end those too.
- Save screenshots outside the repository.
- For debugging by hand, the user has VS Code launch configurations (see the README), and
  devtools open with `Ctrl+Shift+I`; see also
  [Tauri's debugging guide](https://tauri.app/develop/debug/).

### On Linux

The Tauri app (`app/src-tauri`) links against WebKitGTK, so building or testing it on
Linux needs
[Tauri's system packages](https://tauri.app/start/prerequisites/#linux) first. On
Debian or Ubuntu:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

cargo check --manifest-path app/src-tauri/Cargo.toml --all-targets --locked
cargo test --manifest-path app/src-tauri/Cargo.toml --locked
```

The root Rust workspace and `pnpm test` need none of these.

WebKitGTK doesn't speak the Chrome DevTools Protocol, so `scripts/webview.mjs` works only
on Windows. On Linux, run the tests above and say that the UI change wasn't checked in
the running app, rather than checking it in a page made to look like it.

## What you can't check

Some checks need what your environment doesn't have: a host its network blocks (e.g.
`huggingface.co`, where the wty dictionaries are), the dictionaries downloaded to
`%APPDATA%\cc.polv.cjk-dict`, or the running app on Windows. Don't guess the result,
and don't write code that relies on a guess without saying so. Instead:

- Say in your reply what you couldn't check and why, e.g. which host was blocked.
- List each such check in the pull request body under a `## To check locally` heading,
  as a task list the user ticks off. Give each item what to run or look at, and what
  result is expected or would change the code:

  ```md
  ## To check locally

  - [ ] `wty-zh-en`'s `index.json` (huggingface.co was blocked): is `targetLanguage`
        `en`? If not, the `target_lang` override in `dict-admin/src/defaults.rs` is needed.
  - [ ] By hand, in the app: search `學習` (exact); the headword is drawn in traditional
        Chinese forms.
  ```

- For each check, pick the cheapest way for the user, and say which in the item:
  - **By hand**, when it's a quick look: what to open and what to see, e.g. "search
    學習: the headword is in traditional forms", or a devtools (`Ctrl+Shift+I`)
    expression to run. Often this is enough; don't make a script for it.
  - **A local agent session** (Claude Code on the user's machine), when the checks are
    many, need iterating, or need the running app: it can drive the app with
    [`scripts/webview.mjs`](scripts/webview.mjs) and fix what it finds. Say what it
    should check, but only suggest one when that's worth more than a few minutes by
    hand. `scripts/webview.mjs` is for such sessions: don't ask the user to run it and
    paste its output to you.
  - **A script whose output the user pastes back**, when the answer is data to read
    (e.g. what dictionaries' `index.json` says), as below.
- Where a script is the way, write one rather than steps to follow by hand, so the
  user runs one command and pastes its output back to you. Write it in Node (no
  dependencies, like [`scripts/webview.mjs`](scripts/webview.mjs)) or Rust (e.g. a
  `dict-admin` subcommand), put it in `scripts/` (or the crate), commit it, and have it:
  - print plain text or a Markdown table, short enough to paste, with what each line
    means; no secrets, tokens or the user's paths beyond what's needed;
  - report a failure per item (with its cause, e.g. `ENOTFOUND huggingface.co`) and go
    on with the rest, rather than stopping at the first;
  - not download what the user already has (see below).

  Check it as far as your environment allows (local files, reachable hosts), and put the
  command in the checklist item, asking for its output.

- Don't write scripts that download again what is already on the user's machine:
  - Read local copies first: the `dict-admin` library (`CJK_DICT_LIBRARY`, also from
    `.env`, else `library/`) has the default dictionaries and `dicts.json`; the app's
    downloads are in `%APPDATA%\cc.polv.cjk-dict`.
  - Fetch only what is missing, through the tool that already skips what's there
    (`pnpm admin defaults`, without `--replace`), not a fetch of your own.
  - Where something must come from the network, fetch the least: one file of a zip by
    HTTP range requests, not the zip; `index.json`, not the term banks.
  - Never re-download on every run, and don't keep a second copy of a dictionary
    outside the library.
- A script made for a pull request's checks is temporary: once the user has run it and
  the results are in, remove it (and what mentions it) before the pull request is merged.
  Keep only what is useful beyond the pull request, e.g. a `dict-admin` option the admin
  will run again, and say which you kept and why.
- Keep the list in step with later commits: remove items a commit made moot.
- Read the list before changing it (see
  [Pull requests and issues](#pull-requests-and-issues)): keep ticked items ticked, and
  the notes or results the user added under them, unless a commit made an item moot.

## Dictionaries

- The library the user runs `dict-admin` on may hold dictionaries besides the defaults
  (`dict-admin/src/defaults.rs`). Those are personal: don't name them in code, tests,
  comments, docs, commit messages or pull request bodies, even when a script's output
  shows them. Use a made-up name such as `MyDict` in examples and tests.
- Only default dictionaries get language overrides in code. Overriding others
  (`dicts.json`, `add --lang` / `--target-lang`) is up to whoever runs `dict-admin`; don't
  tell them what to set for a personal one in a pull request.
- Squash merges keep only the pull request's title and body: put what matters there,
  not only in commit messages.

## Documentation

- [`README.md`](README.md) is what the project is, setup, and the commands at a glance.
  How things work in detail goes in [`docs/`](docs/), one page per area, listed near the
  top of the README:
  - [`docs/dict-admin.md`](docs/dict-admin.md): `dict-admin`'s commands, dictionary
    languages, the default dictionaries;
  - [`docs/search.md`](docs/search.md): searching in the app;
  - [`docs/app.md`](docs/app.md): how the app downloads the library, shows entries, and its tooltips;
  - [`docs/yomitan-differences.md`](docs/yomitan-differences.md): where rendering
    differs from Yomitan.
- A change to what a page describes updates the page in the same pull request. An area
  without a page gets a new one, added to the README's list and the list above.
- Write in short bullets, one point each, nesting details under them, rather than long
  paragraphs or sentences strung together with semicolons. Use a table for settings and
  their values, and give an example value for each.
- Check what you write against the code: options, defaults, limits (e.g. how far pinyin
  is expanded) and file names. Remove what a change made untrue.
- Leave migration steps (e.g. `pnpm admin defaults --langs` for a library from before a
  change) out of the README and docs: put them in the pull request's "After merging".

## Pull requests and issues

- Read a pull request's or issue's body right before you change it, and work from what
  it says now, not from the version you wrote or remember:
  - the user may have edited it since: ticked items, reworded them, or added notes,
    results or sections;
  - keep what the user changed, unless your change makes it untrue;
  - rewrite only the parts your change is about, rather than replacing the whole body.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```
<type>(<optional scope>): <short imperative summary>

<optional body: what changed and why>
```

- `<type>` is one of `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
  `build`, `ci`, `chore`, `revert`.
- `<scope>` names the area touched, e.g. `app`, `dict-admin`, `yomitan-parser`,
  `clipboard`.
- Keep the summary under about 72 characters, lowercase after the colon, with no
  trailing period.
- Mark breaking changes with `!` after the type or scope (`feat(app)!: ...`) and a
  `BREAKING CHANGE:` footer.

Examples:

```
feat(clipboard): read HTML clips on Android
fix(app): recover from a complete .dl-tmp when resuming a download
docs: add agent instructions for branches and commits
```

Use the same format for pull request titles.
