# Releasing

A release is one tag. Everything after the tag is automated by
`.github/workflows/release.yml`.

## Steps

1. Make sure `CHANGELOG.md` has an `## Unreleased` section describing the release.
2. Set the version everywhere and date the changelog:

   ```bash
   cargo xtask bump 0.2.0
   ```

   This updates `Cargo.toml`, every crate's internal dependency versions, `Cargo.lock`,
   `npm/package.json`, the VS Code extension and the Claude Code plugin manifest, and
   renames `## Unreleased` to `## 0.2.0 (date)`. It ends by running the same check
   the release workflow runs.
3. Open a pull request with that change and merge it once CI is green.
4. Tag the merge commit on `master` and push the tag:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

A tag with a suffix, such as `v0.3.0-rc.1`, becomes a GitHub pre-release, and the
Homebrew tap is left alone.

## What the workflow does

| Job | Result |
|---|---|
| verify | Fails unless the tag, every version field, `Cargo.lock` and `CHANGELOG.md` agree |
| build | Archives for Linux, macOS and Windows (x86_64 and ARM64) with PDFium, SHA-256 files and signed build provenance. Linux links against glibc 2.17; each binary is run before upload |
| vsix | VS Code extension, tested and packed as `proofback-vscode.vsix` |
| node | Node package, checked against the native result and packed as `proofback-node.tgz` |
| publish | GitHub Release with the changelog section as notes and all of the above, plus `install.sh`, `install.ps1` and `proofback.rb` |
| homebrew | Optional: commits `Formula/proofback.rb` to a tap repository |

Everything ships through the GitHub Release; no npm or Marketplace account is used.
The installers and the files above have stable "latest" links:

```bash
npm install https://github.com/Amet13/proofback/releases/latest/download/proofback-node.tgz
code --install-extension proofback-vscode.vsix   # after downloading it from the release
```

## Optional: Homebrew tap

A tap is an ordinary repository on the same GitHub account, so no new account is
needed. To turn it on, create the repository (default `Amet13/homebrew-tap`; set the
`HOMEBREW_TAP_REPO` variable to use another) and add a fine-grained token with Contents
read and write on that repository only as the `HOMEBREW_TAP_TOKEN` secret. Users then
install with `brew install amet13/tap/proofback`. Without the secret the job prints a
notice and the formula is still attached to the release.

## Checking a release without publishing

Run the Release workflow by hand (Actions, Release, Run workflow) on any branch. It
builds every file but publishes nothing.
