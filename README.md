# proofback

**Every margin note, back in your source.**

Your supervisor (or co-author, or reviewer 2) marked up the PDF. proofback lands
each highlight, strike-out, caret and sticky note on the exact **file, line and
character range** of your LaTeX source, tracks what you did about it across drafts,
and sends the answers back — as the reviewer's own PDF with a reply on every note, or
as a response-to-reviewers letter.

> **Status: in design.** Nothing to install yet. The full plan — scope, architecture,
> matching pipeline, data model, roadmap and decision log — is in
> [`docs/PLAN.md`](docs/PLAN.md). Website: <https://amet13.github.io/proofback/>

## Why

Reviewers don't edit `.tex`. They annotate PDFs in Acrobat, Preview, Zotero or on a
tablet, or send an email of page numbers. Authors then hunt for every mark by hand,
lose some, and write replies that drift from what actually changed. Existing tools
either extract annotations without source locations (pdfannots), jump to one location
at a time (SyncTeX in editors), regenerate your source from Word (pandoc), or require
the reviewer to join a platform (Overleaf, Prism).

## What it will look like

```bash
proofback import review-ch3.pdf --round supervisor-2026-11
proofback list
#  where                         kind     note
#  chapters/method.tex:41:18-23  replace  use 'robust' not 'strong'   exact
#  chapters/method.tex:44        question why this threshold?          exact
#  chapters/method.tex:45-46     delete   —                            exact

proofback patch pb_7k2m9q | git apply
proofback resolve pb_3xq1fd --status discuss --reply "Chosen by 5-fold CV, see §3.4"
proofback answer -o review-ch3-answered.pdf
```

- **Precise.** SyncTeX narrows to lines; a TeX-aware aligner finds the characters,
  through macros, `\ref`, ligatures and hyphenation. Every item carries an honest
  confidence level.
- **Git-native.** State lives in `.proofback/` in your repo. Anchors follow your
  edits into the next draft.
- **Agent-ready.** `--json` everywhere, a published schema, an MCP server, and an
  agent kit for Claude Code, Codex and Cursor. AI is optional, never required.
- **Local.** Rust core, native and WebAssembly. The browser app on the website runs
  entirely on your machine — nothing is uploaded.
- **Measured.** Accuracy on a CI-built corpus is published with every release.

## Roadmap

| Version | Scope |
|---|---|
| v0.1 | PDF import, anchoring, `list` / `show` |
| v0.2 | `resolve`, `patch`, `apply`, `sync`, answered PDF, response letter |
| v0.3 | MCP server, agent kit, GitHub Action (PR review comments) |
| v0.4 | Browser app (WASM) on GitHub Pages, npm package |
| v0.5 | Email / free-text notes, optional AI suggestions |
| v0.6 | Word round trip (export with anchors, import tracked changes) |
| v0.7+ | VS Code extension, Typst, schema v1.0 |

## Related

[overfull](https://github.com/Amet13/overfull) checks a paper **before** you submit
it. proofback starts **after** someone has read it. Separate tools, shared conventions.

## Repository layout (today)

```
docs/PLAN.md                 the project plan
site/                        GitHub Pages website (static, no build step)
.github/workflows/pages.yml  deploys site/ on push to the default branch
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
