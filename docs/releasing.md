# Releasing

Releases are automatic; nobody clones, tags or builds by hand.
`.github/workflows/release.yml` does everything.

## How to release

From the GitHub website:

1. Make sure `CHANGELOG.md` on `master` describes the changes under `## Unreleased`
   (contributors add a line there with each pull request).
2. Open **Actions, Release, Run workflow** on `master` and set **bump** to `patch`,
   `minor`, `major` or an exact version such as `1.0.0`.

The workflow then sets the version in every manifest and `Cargo.lock`, turns
`## Unreleased` into `## 1.0.0 (date)` (leaving a fresh `## Unreleased` above it),
commits that to `master` as "Release 1.0.0", builds everything, tags `v1.0.0` and
publishes the GitHub Release with that changelog section as its notes.

Merging a pull request that changes the version (for example one made with
`cargo xtask bump minor`) releases it the same way: on every push to `master` the
workflow checks whether the version in `Cargo.toml` has been released, and releases
it if not. Pushes that leave the version alone build nothing.

An exact version with a suffix, such as `1.0.0-rc.1`, becomes a GitHub pre-release,
and the Homebrew tap is left alone.

If the version fields or the changelog disagree, the run fails before anything is
built or published.

## What the workflow does

| Job | Result |
|---|---|
| prepare | Optional version bump commit; decides whether to release; fails unless every version field, `Cargo.lock` and `CHANGELOG.md` agree |
| build | Archives for Linux, macOS and Windows (x86_64 and ARM64) with PDFium, SHA-256 files and signed build provenance. Linux links against glibc 2.17; each binary is run before upload |
| vsix | VS Code extension, tested and packed as `proofback-vscode.vsix` |
| node | Node package, checked against the native result and packed as `proofback-node.tgz` |
| publish | Tag `vX.Y.Z` and GitHub Release with the changelog section as notes and all of the above, plus `install.sh`, `install.ps1` and `proofback.rb` |
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

Run the Release workflow with **bump** left empty, on any branch. It builds every
file and runs the checks but publishes nothing.

## Requirements

The workflow pushes the "Release" commit to `master` with the built-in token. If a
branch protection rule blocks pushes to `master`, allow GitHub Actions to bypass it,
or release by merging a version bump instead.
