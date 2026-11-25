# proofback

**Every margin note, back in your source.**

Your supervisor, co-author or reviewer marked up the PDF. proofback finds the exact
**file, line and characters** in your LaTeX source that each highlight, strike-out,
caret and note refers to, tracks what you did about it across drafts, and sends the
answers back: the reviewer's own PDF with a reply on every note, or a response letter.

> **Status: v0.1.0, all planned milestones implemented.** Website and browser app:
> <https://amet13.github.io/proofback/>. Plan and decision log: [`docs/PLAN.md`](docs/PLAN.md).

## Why

Reviewers don't edit `.tex`. They annotate PDFs in Acrobat, Preview, Zotero or on a
tablet, or send an email of page numbers. Authors then hunt for every mark by hand,
lose some, and write replies that drift from what changed. Existing tools either
extract annotations without source locations, jump to one location at a time,
regenerate your source from Word, or require every reviewer to join a platform.

## Install

From a release (Linux and macOS; PowerShell installer for Windows in the release):

```bash
curl -fsSL https://github.com/Amet13/proofback/releases/latest/download/install.sh | sh
```

From source (Rust 1.92 or later):

```bash
cargo install --git https://github.com/Amet13/proofback proofback-cli
```

Positioned PDF text needs PDFium; the release bundles it. Without it, proofback falls
back to the text stored in the annotations. No install at all: open the
[browser app](https://amet13.github.io/proofback/app/), which runs on your machine.
Node: `npm install proofback`.

## Usage

```bash
proofback import review-ch3.pdf --round supervisor-2026-11
proofback list
# id          where                          kind      note                       anchor
# pb_7k2m9q   chapters/method.tex:41:18-23   replace   Replace with "robust"      exact
# pb_3xq1fd   chapters/method.tex:44:1-13    question  Why this threshold?        exact
# pb_d0r4kl   chapters/method.tex:45:1-46:34 delete    (strike-out, no note)      exact

proofback patch pb_7k2m9q pb_d0r4kl | git apply
proofback resolve pb_3xq1fd --status discuss --reply "Chosen by 5-fold CV, see 3.4"
proofback answer -o review-ch3-answered.pdf
```

- **Precise.** SyncTeX narrows each mark to a few lines; a TeX-aware aligner finds the
  characters. Precision comes first: proofback says "page only" rather than guess.
- **Git-native.** State lives in `.proofback/` in your repository, and anchors follow
  your edits into the next draft.
- **Agent-ready.** `--json` everywhere, a published schema, an MCP server and an agent
  kit. Reviewer text is passed to agents as untrusted data. AI is optional.
- **Local and safe.** Rust core, native and WebAssembly. The browser app runs on your
  machine; nothing is uploaded. Untrusted PDFs and archives are parsed with limits.
- **Measured.** Accuracy and performance are checked in CI and published per release.

## More inputs

```bash
proofback import notes-from-email.txt          # "p. 12, para 2: unclear"
proofback docx export -o thesis.docx           # send to a Word reviewer
proofback import thesis-reviewed.docx          # tracked changes and comments come back
proofback import paper.pdf                     # Typst projects work the same way
```

Agents: `proofback mcp` runs an MCP server on stdio; see [`docs/agents.md`](docs/agents.md)
and the agent kit in [`integrations/`](integrations/). Pull requests: see
[`docs/github-action.md`](docs/github-action.md). VS Code: [`integrations/vscode`](integrations/vscode).

## Accuracy

Measured in CI on every pull request (details in [`docs/PLAN.md`](docs/PLAN.md#21-implementation-notes)):

| Input | Exact range | Wrong anchor |
|---|---|---|
| Annotated PDF with SyncTeX | 97.8% | 0.4% |
| Annotated PDF, text only | 43.3% (95.3% correct line) | 0% |
| Word round trip | 100% | 0% |
| Typst | 100% | 0% |

A 322-page thesis with 500 notes imports in about 0.3 s.

## Repository layout

```
crates/                 Rust workspace: core, CLI, MCP server, WebAssembly bindings
fixtures/               LaTeX and Typst projects with built PDFs and SyncTeX files
site/                   website and browser app, static
npm/                    npm package
integrations/           agent kit, VS Code extension
packaging/              installers, Homebrew formula, GitHub Action helper
docs/                   plan, agent guide, GitHub Action guide
```

Develop:

```bash
cargo xtask pdfium
PROOFBACK_REQUIRE_PDFIUM=1 cargo test --workspace
```

Check the website locally:

```bash
cd tools && npm ci && npx playwright install chromium && npm run check
```

## Related

[overfull](https://github.com/Amet13/overfull) checks a paper **before** you submit
it. proofback starts **after** someone has read it.

## License

Code: [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Bundled website fonts: SIL Open Font License 1.1 (see `site/fonts/`).
