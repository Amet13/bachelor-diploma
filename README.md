# proofback

**Every margin note, back in your source.**

Your supervisor sent the PDF back with forty-seven marks on chapter 3. proofback
reads them and tells you, for each one, the file, line and characters in your LaTeX
source it refers to. It keeps a record of what you changed and what you didn't (and
why), and writes your replies back into the reviewer's own PDF or a response letter.

Website and browser app: <https://amet13.github.io/proofback/>. Changes in each release:
[`CHANGELOG.md`](CHANGELOG.md). Design notes and decision log: [`docs/PLAN.md`](docs/PLAN.md).

## Why this exists

Reviewers don't edit `.tex` files. They mark up the PDF in Acrobat, Preview, Zotero
or on a tablet, or they send an email full of page numbers. You then search for each
sentence by hand across a tree of `\input` files, miss a few, and write replies that
no longer match what you changed. The tools that exist pull annotations out without
saying where they belong in the source, jump to one location at a time, rebuild your
source from Word, or ask every reviewer to sign up for a platform.

## Install

macOS and Linux:

```bash
curl -fsSL https://github.com/Amet13/proofback/releases/latest/download/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://github.com/Amet13/proofback/releases/latest/download/install.ps1 | iex
```

From source (Rust 1.92 or later):

```bash
cargo install --git https://github.com/Amet13/proofback proofback-cli
```

Positioned PDF text needs PDFium; the release bundles it. Without it, proofback falls
back to the text stored in the annotations. No install at all: open the
[browser app](https://amet13.github.io/proofback/app/), which runs on your machine.

Each [release](https://github.com/Amet13/proofback/releases/latest) also carries the
VS Code extension (`proofback-vscode.vsix`) and a Node package:

```bash
npm install https://github.com/Amet13/proofback/releases/latest/download/proofback-node.tgz
```

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

SyncTeX narrows each mark to a few lines and a TeX-aware aligner finds the
characters. When proofback isn't sure, it says "line" or "page only" instead of
guessing; a confident wrong answer is the one thing it is built to avoid.

The record lives in `.proofback/` next to your sources, so it is versioned with them,
and `sync` carries every item into the next draft. Each command takes `--json`; the
schema is published, and an MCP server lets a coding agent work through the list with
you. Reviewer text reaches the agent marked as untrusted. The core makes no network
calls, and the browser app reads your files without uploading them.

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
