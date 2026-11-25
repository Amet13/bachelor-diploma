# Changelog

All notable changes to proofback. Versions follow [Semantic Versioning](https://semver.org/);
the JSON schema is versioned separately (`schema_version` in round files).

## Unreleased

## 0.2.0 (2026-11-25)

### Fixes

- Releases follow the repository's default branch, so the rename from `master` to
  `main` (or any later one) cannot stop them.
- PDF reading uses lopdf 0.47, which bounds stream decompression by default and fixes
  several crashes on malformed files.
- SyncTeX: a file with a huge `X Offset` or `Y Offset` overflowed when offsets were
  added to coordinates (a panic in debug builds, wrong positions in release builds).
  Found by fuzzing.
- LaTeX sources: a macro that uses itself several times expanded exponentially, so a
  500-byte file could take close to a minute to read. Macro expansion now has a fixed
  budget per file; beyond it, a macro counts as unknown text. Found by fuzzing.

### Website and app

- One type scale (7 sizes, 3 weights) and one set of colour tokens for the website and
  the browser app, replacing 19 font sizes and about 50 hard-coded colours.
- Landing page: plain section headings, an install section, and no filler (captions,
  revision table, colophon).
- Browser app: consistent buttons, aligned header, and the setup form keeps its width.
- Browser app: each deploy now gets its own offline cache. Before, a returning visitor
  could keep an old version of the app indefinitely.
- New social preview image matching the current site.

### Releases

- Every release now carries the Node package (`proofback-node.tgz`) and the VS Code
  extension (`proofback-vscode.vsix`), both tested before packing, with stable
  "latest" download links. Nothing is published to npm or the VS Code Marketplace.
- Optional Homebrew tap update when `HOMEBREW_TAP_TOKEN` is set.
- Linux release binaries link against glibc 2.17, so they run on every maintained
  distribution. A build on the newest runner image required glibc 2.43 and failed to
  start on Ubuntu 24.04 and older. The release now checks this, and runs each packaged
  binary before uploading it.
- Releases are automatic. From the GitHub website, run the Release workflow with
  patch, minor, major or an exact version: it bumps every version field, dates the
  changelog, commits to the default branch (`main`), builds, tags and publishes. Merging a version bump
  releases it too. Nothing is tagged or built by hand.
- `cargo xtask bump <patch|minor|major|X.Y.Z>` and `cargo xtask release-check` back
  this up locally; the release notes come from the changelog.

### CI

- The Pages deploy runs in its own job after the site artifact is uploaded. Doing both
  in one job could race and fail the deploy.
- Runs on the main branch are no longer cancelled by a newer push; only superseded
  pull-request runs are.
- Fuzzing on pull requests is deterministic (fixed seed and run count, 10-second limit
  per input), so it fails only for a bug the change can reproduce. Random fuzzing runs
  nightly in its own workflow, with a pinned nightly and `cargo-fuzz`.
- Dependabot groups updates into one pull request per ecosystem and now also covers
  Cargo and the VS Code extension.

## 0.1.0 (2026-11-25)

First release.

- Import reviewer feedback from annotated PDFs, notes in an email, and Word documents
  (tracked changes and comments), and map each item to the exact file, line and
  characters of the LaTeX or Typst source.
- Track decisions and replies in `.proofback/`, carry items across drafts with `sync`,
  apply suggested edits, and send answers back as an annotated PDF or a response letter.
- MCP server with guard rails, agent instructions, GitHub Action and SARIF output.
- Browser app (WebAssembly, runs offline), npm package and VS Code extension.
- Accuracy gates in CI: 97.8% exact and 0.4% wrong with SyncTeX on the synthetic corpus.
