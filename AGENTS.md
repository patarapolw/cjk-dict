# Agent instructions

## Branches

- Before pushing to `origin`, and before opening a pull request, rename the working
  branch to `<agent>/<description>`:
  - `<agent>` is the agent's name in lowercase, e.g. `claude`, `codex`, `copilot`.
  - `<description>` is a short, meaningful kebab-case summary of the change, e.g.
    `android-clipboard-html` or `fix-download-resume`, not a generated name like
    `ccr-1a2b3c4d-xyz`.

  ```sh
  git branch -m claude/android-clipboard-html
  git push -u origin claude/android-clipboard-html
  ```

- If the generated branch was already pushed, push the renamed branch and delete the
  old remote branch (`git push origin --delete <old-name>`) only when nothing else,
  such as an open pull request, uses it.

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
