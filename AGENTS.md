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
