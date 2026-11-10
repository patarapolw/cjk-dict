# Agent instructions

## Branches

- Before pushing to `origin`, and before opening a pull request, rename the working
  branch to a short, meaningful kebab-case name that describes the change, e.g.
  `android-clipboard-html` or `fix-download-resume` — not a generated name like
  `ccr-1a2b3c4d-xyz`.

  ```sh
  git branch -m <meaningful-name>
  git push -u origin <meaningful-name>
  ```

- If the generated branch was already pushed, push the renamed branch and delete the
  old remote branch (`git push origin --delete <old-name>`) only when nothing else,
  such as an open pull request, uses it.
