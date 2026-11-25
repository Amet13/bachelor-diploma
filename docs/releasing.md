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

A tag with a suffix, such as `v0.3.0-rc.1`, becomes a GitHub pre-release and an npm
`next` release; Homebrew and the VS Code Marketplace are skipped for it.

## What the workflow does

| Job | Result | Needs |
|---|---|---|
| verify | Fails unless the tag, every version field, `Cargo.lock` and `CHANGELOG.md` agree | nothing |
| build | Archives for Linux, macOS and Windows (x86_64 and ARM64) with PDFium, SHA-256 files and signed build provenance | nothing |
| vsix | Tested and packaged VS Code extension | nothing |
| publish | GitHub Release with the changelog section as notes, the archives, `install.sh`, `install.ps1`, `proofback.rb` and the `.vsix` | nothing |
| npm | `npm publish --provenance` of the WebAssembly package, after checking it against the native result | `NPM_TOKEN` secret |
| homebrew | Commits `Formula/proofback.rb` to the tap repository | `HOMEBREW_TAP_TOKEN` secret |
| vscode | Publishes the `.vsix` to the VS Code Marketplace | `VSCE_PAT` secret |

Without its secret, a registry job prints a notice and does nothing; the GitHub
Release still ships. The installers (`install.sh`, `install.ps1`) download from the
latest GitHub Release, so they work as soon as the publish job finishes.

## One-time setup

Add these in the repository's Settings, Secrets and variables, Actions.

- **`NPM_TOKEN`**: an npm automation token (granular, publish access to the `proofback`
  package). The first publish creates the package.
- **`HOMEBREW_TAP_TOKEN`**: a fine-grained GitHub token with Contents read and write on
  the tap repository only. Create the tap repository first (default
  `Amet13/homebrew-tap`; set the `HOMEBREW_TAP_REPO` variable to use another). Users
  then install with `brew install amet13/tap/proofback`.
- **`VSCE_PAT`**: an Azure DevOps personal access token with the Marketplace, Manage
  scope, for the publisher `proofback` (create the publisher at
  <https://marketplace.visualstudio.com/manage> first).

Once a registry has its first release, add its install command to the README and the
website.

## Checking a release without publishing

Run the Release workflow by hand (Actions, Release, Run workflow) on any branch. It
builds every archive and the `.vsix` but publishes nothing.
