# Changelog

All notable changes to proofback. Versions follow [Semantic Versioning](https://semver.org/);
the JSON schema is versioned separately (`schema_version` in round files).

## 0.2.0 (2026-11-25)

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

- A version tag now publishes to npm (with provenance), the Homebrew tap and the VS Code
  Marketplace, in addition to the GitHub Release. Each step is skipped, with a notice,
  until its token is configured.
- `cargo xtask bump <version>` sets the version in every manifest, refreshes
  `Cargo.lock` and dates the changelog. The release workflow refuses a tag that does not
  match, and uses the changelog section as the release notes.
- The VS Code extension package (`.vsix`) is attached to every GitHub Release.

### CI

- The Pages deploy runs in its own job after the site artifact is uploaded. Doing both
  in one job could race and fail the deploy, as happened on `master`.
- Runs on `master` are no longer cancelled by a newer push; only superseded
  pull-request runs are.
- The fuzz job uses a pinned nightly and a pinned `cargo-fuzz`.
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
