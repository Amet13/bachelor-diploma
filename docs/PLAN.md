# proofback: project plan

**Every margin note, back in your source.**

proofback takes the feedback a reviewer leaves on a compiled PDF (highlights,
strike-outs, carets, sticky notes, and later email notes and Word tracked changes),
finds the exact file, line and character range of the LaTeX source each item refers
to, tracks what the author did about it across drafts, and sends the answers back.

| | |
|---|---|
| Status | Implemented through M8 (v1.1 of this document). Deviations from v1.0 and measured results: [§21](#21-implementation-notes). |
| Website | <https://amet13.github.io/proofback/> |
| Licence | MIT OR Apache-2.0 |
| Decisions | Numbered `D-xx`, collected in [§20](#20-decision-log) |
| Audit | Findings from the plan review and how each was resolved: [§19](#19-audit-and-grill-log) |

---

## 1. Problem

Supervisors, co-authors, peer reviewers and proofreaders do not edit `.tex` files.
They read the PDF and mark it up in Acrobat, Preview, Zotero, Okular, PDF Expert or
a tablet app, or they send an email that lists page numbers. The author then:

1. opens the annotated PDF next to the editor,
2. finds each mark by eye,
3. searches for the matching sentence across a tree of `\input` files,
4. edits, and tries to remember what was done, skipped or disputed,
5. writes the replies by hand.

This repeats for every chapter and every draft of a thesis, and ends in a
hand-written response letter for a paper. Comments get lost, replies drift away
from what actually changed, and the reviewer cannot check.

### 1.1 What exists (surveyed November 2026)

| Tool | What it does | What is missing |
|---|---|---|
| pdfannots | Extracts PDF annotations to Markdown with page and quoted text | No source file or line |
| LaTeX Workshop, TeXstudio, Skim, Okular, Emacs pdf-tools | SyncTeX jump for one location at a time | No bulk import, no tracking |
| pandoc `--track-changes` | DOCX to LaTeX keeping insertions and deletions | Produces a new `.tex`; the author's source and formatting are lost |
| Overleaf, OpenAI Prism | Comments and track changes inside the platform | Every reviewer must adopt the platform; no PDF or Word input |
| Small 2026 AI "skills" | An LLM re-applies Word edits to `.tex` | Non-deterministic, no anchors, no tracking |

**Gap:** nothing turns feedback in the formats reviewers actually use into a
precise, trackable list of changes against the author's existing source.

### 1.2 Non-goals

- Pre-submission checking (references, PDF problems, venue rules, templates). That is
  [overfull](https://github.com/Amet13/overfull). proofback starts after someone has
  read the document. The tools share conventions (JSON shape, `--json`, agent docs),
  not code or features (D-01).
- Being an editor, a collaboration platform, or a hosted service.
- Grammar or style checking, or generating content.

---

## 2. Users and workflows

- **Author** (primary): keeps LaTeX in a folder or git repository, possibly synced
  with Overleaf.
- **Reviewer**: never installs anything and keeps their own tools.
- **Coding agent**: acts for the author through the CLI or MCP server.

Theses are the flagship story. Any reviewed LaTeX document is supported (D-02).

### W1. Annotated PDF to checklist (v0.1)

```bash
proofback import review-ch3.pdf --round supervisor-2026-11
proofback list
```

```
id          where                          kind      note                          anchor
pb_7k2m9q   chapters/method.tex:41:18-23   replace   Replace with "robust"         exact
pb_q81fza   chapters/method.tex:42:17-40   comment   Cite the field study here     exact
pb_3xq1fd   chapters/method.tex:44:1-13    question  Why this threshold?           exact
pb_d0r4kl   chapters/method.tex:45:1-46:34 delete    (strike-out, no note)         exact
pb_m2h7tq   p. 15, figure 3.2              comment   Axis labels unreadable        region
```

### W2. Work through it, alone or with an agent (v0.2, v0.3)

```bash
proofback patch pb_7k2m9q pb_d0r4kl | git apply
proofback resolve pb_3xq1fd --status discuss --reply "Chosen by 5-fold CV, see 3.4"
proofback resolve pb_m2h7tq --status wontfix --reply "Figure is regenerated in the next draft"
proofback triage          # keyboard UI for confirming low-confidence items
```

IDs accept unique prefixes, like git (`pb_7k2`). In Claude Code, Codex or Cursor:
"work through the open proofback items in chapter 3". The agent uses the MCP server,
proposes edits and records replies.

### W3. Send the answers back (v0.2)

```bash
proofback answer -o review-ch3-answered.pdf   # reviewer's own PDF, reply on every note
proofback letter --format latex > response.tex
```

### W4. Next draft (v0.2)

`proofback sync` re-anchors every open item in the current source and reports
items that moved, changed (probably addressed) or were lost.

### W5. Notes from an email (v0.5)

```bash
proofback import notes.txt   # "p. 12, para 2: unclear", "3.1: 'state of the art' -> 'current'"
```

### W6. No terminal (v0.4)

Open <https://amet13.github.io/proofback/app/>, drop the annotated PDF, the
`.synctex.gz` and the sources (folder or Overleaf zip). Everything runs locally in
WebAssembly. Nothing is uploaded.

### W7. Word round trip (v0.6)

```bash
proofback docx export -o thesis-for-review.docx   # hidden source anchors embedded
proofback import thesis-reviewed.docx              # tracked changes and comments become items
```

### W8. Pull request review (v0.3)

A GitHub Action imports a review PDF committed to the repository and posts each item
as an inline review comment on the right line.

---

## 3. Product surfaces (D-03)

| Surface | Version | Notes |
|---|---|---|
| CLI `proofback` | v0.1 | Single binary for Linux, macOS, Windows (x86-64 and arm64); `--json` everywhere |
| `proofback triage` | v0.2 | Terminal UI (ratatui) for confirming and resolving items |
| MCP server `proofback mcp` | v0.3 | stdio transport only |
| Agent kit | v0.3 | Agent Skill (`SKILL.md`), Claude Code plugin, `AGENTS.md` snippet, Cursor rules, `llms.txt` |
| GitHub Action | v0.3 | Review comments and job summary |
| Browser app | v0.4 | Same Rust core as WebAssembly, under `/app` on the website |
| npm package | v0.4 | WebAssembly build for Node 20+ and `npx proofback` |
| VS Code extension | v0.7 | Tree view, gutter marks, apply, reply |
| Typst | v0.8 | Through the Typst compiler's span API |

---

## 4. Architecture

### 4.1 Language (D-04)

One Rust core, built for native targets (CLI, MCP, Action) and `wasm32-unknown-unknown`
(browser, npm). The browser and the CLI therefore give identical answers and share one
golden test suite. Rust over TypeScript: speed on large theses, a single static binary
without a runtime, and a direct WebAssembly path. Rust over Python: distribution to
authors who are not programmers.

### 4.2 Workspace

```
crates/
  proofback-model     items, rounds, selectors, JSON Schema (serde, schemars)
  proofback-synctex   streaming .synctex(.gz) parser and index
  proofback-pdf       annotations and positioned text (pdfium adapter behind a trait)
  proofback-tex       TeX-aware tokenizer with a source <-> rendered-text offset map
  proofback-anchor    matching pipeline and confidence scoring
  proofback-store     .proofback/ files, rounds, sync
  proofback-emit      list, patch, answered PDF, letter, SARIF, review comments
  proofback-notes     free-text notes (v0.5)
  proofback-ai        optional model calls for `suggest` and notes (v0.5)
  proofback-docx      Word export and import (v0.6)
  proofback-typst     Typst projects through the embedded Typst compiler (v0.8)
  proofback-cli       clap CLI and ratatui triage
  proofback-mcp       MCP server (own JSON-RPC over stdio)
  proofback-wasm      wasm-bindgen bindings for browser and npm
  proofback-corpus    synthetic corpus, accuracy gates and performance test
xtask/                schema, fixtures, pinned pdfium, WebAssembly and npm builds
site/                 website and the browser app (/app)
fixtures/             golden corpus
integrations/         agent kit and VS Code extension
npm/                  npm package (Node)
docs/                 plan, agent guide, GitHub Action guide
```

### 4.3 PDF layer (D-05)

Two libraries, each used for what it does best (changed from v1.0, see §21):

- `lopdf` (pure Rust) reads Highlight, Underline, StrikeOut, Squiggly, Text (sticky note), FreeText,
  Caret, Ink, Square, Circle and Line annotations with QuadPoints, Rect, Contents,
  RichContents, author, dates, colour, `/NM` and reply chains (`/IRT`), and writes
  the reply annotations and metadata stripping that `answer` needs;
- pdfium through `pdfium-render` returns per-character boxes, so the text under a
  highlight is known even when the annotating app did not store it.

pdfium is Apache-2.0/BSD. Native builds bundle a pinned prebuilt pdfium (checksum
verified; see §10). The browser loads pdfium's WebAssembly build on demand. Without
pdfium, import falls back to the text stored in the annotations.

### 4.4 SyncTeX layer (D-06)

Own streaming parser. The format is small and documented, and proofback needs every
box record on a page (not only the best hit) to score several candidate lines.
Decompression uses `miniz_oxide` (pure Rust, WebAssembly-friendly). The parsed index
is cached in `.proofback/cache/` (git-ignored), keyed by file hash.

Paths inside SyncTeX files are normalised (absolute paths, `./` prefixes, Windows
separators) and must resolve inside the project root.

### 4.5 TeX normaliser (D-07)

`proofback-tex` produces an approximation of the text the PDF shows, with a map from
every output character back to a source byte range:

- drops comments, most commands and braces; keeps arguments of text commands
  (`\emph{x}` becomes `x`); turns `\cite`, `\ref`, `\label` into placeholders;
- handles `~`, `\,`, `--`, `---`, TeX quotes, accents, ligatures, `\\`;
- turns inline math into one placeholder token and marks display math, figures and
  tables as blocks;
- expands simple parameterless `\newcommand` macros; other macros become opaque tokens;
- follows `\input`, `\include` and `\subfile`, preferring the file list from SyncTeX.

It is deliberately not a TeX engine. It only has to support fuzzy alignment inside
the small window SyncTeX has already found.

### 4.6 Source text rules

- Sources are read as UTF-8. Files in another encoding are reported by `doctor` and
  skipped (never written).
- Line endings are preserved on write (LF or CRLF per file).
- Canonical positions are UTF-8 byte ranges. Lines are 1-based. Columns are 1-based
  Unicode code points. The MCP server and VS Code extension convert to UTF-16 where
  their protocols need it.

### 4.7 WebAssembly build

`wasm-bindgen` CLI plus `wasm-opt`, driven by a small `xtask` (wasm-pack is
unmaintained since the rustwasm organisation was archived in 2025). The core runs in
a Web Worker so the UI never blocks. The build is single-threaded because GitHub
Pages cannot send the COOP/COEP headers that `SharedArrayBuffer` threads require.

---

## 5. Matching pipeline

**Principle: precision before recall (D-14).** A wrong confident anchor costs the
author more than an honest "page only". Every stage may lower confidence; none may
invent it.

For each annotation:

1. **Extract** type, page, quads or rect, note text, author, date, colour, `/NM`,
   replies.
2. **Read the marked text** under the quads plus about 80 characters of context on
   each side, handling hyphenation at line ends and multi-column layouts.
3. **Coarse locate.** SyncTeX inverse search for each quad (centre and corners) gives
   a ranked set of `(file, line)` candidates. Point annotations use the nearest text
   line on the page.
4. **Fine locate.** Normalise the candidate lines plus a margin with `proofback-tex`
   and align the marked text with its context (banded Smith-Waterman, bitap for
   short quotes). Map the hit back through the offset map to a source byte range.
5. **Classify**: `comment`, `question`, `delete`, `insert`, `replace`, `highlight`,
   `figure`, `math`, `general`. StrikeOut with a note becomes `replace` with a
   suggested replacement; Caret with text becomes `insert`; StrikeOut alone becomes
   `delete`.
6. **Score**: `exact` (character range), `line`, `region` (figure, table, display
   math), or `page` (needs a human). Each level carries a readable reason. An
   alignment with two near-equal hits is never `exact`.

### 5.1 Hard cases designed for from the start

Two-column layouts; hyphenation; ligatures; text produced by macros; `\input` trees;
floats placed far from their source; captions; footnotes; display math; tables;
generated text (bibliography, table of contents), flagged `generated` and pointed at
the `.bib` key or heading where possible; XeLaTeX and LuaLaTeX with non-Latin scripts
(Greek, Arabic, CJK); and a reviewer who annotated an older draft.

### 5.2 Draft mismatch

Each round records the git commit (when available) and hashes of the PDF and SyncTeX
file it was imported against. For an older PDF:
`proofback import review.pdf --synctex old.synctex.gz --at <commit>` anchors against
that commit's files (read through git plumbing, never a shell), then `sync` carries
the anchors forward to the working tree. With only a PDF, step 4 runs against the
whole project (text-only mode).

### 5.3 Text-only mode

Without SyncTeX, proofback searches the whole project for the marked text and context,
using page order as a prior. Confidence is capped at `line` unless the match is
unique and long. Accuracy is reported separately for this mode.

### 5.4 Anchors that survive edits

Each item stores W3C Web Annotation style selectors:

- `TextQuoteSelector`: normalised quote, prefix and suffix;
- `TextPositionSelector`: file, byte range, line and column at the recorded commit;
- `PdfSelector`: page, quads, PDF hash (provenance only, never used to re-anchor).

`sync` tries quote and context first, then position, and sets the item's
`anchor.state` to `anchored`, `moved`, `changed` or `lost`.

---

## 6. Data model

All state is plain files in the author's project, committed with it (D-08):

```
.proofback/
  config.toml                  project root, main file, build directory, defaults
  rounds/
    2026-11-25-supervisor.json
    2027-01-10-reviewer-2.json
  cache/                       git-ignored; parsed SyncTeX and text indexes
```

Round files are written deterministically (items sorted by id, stable key order,
two-space indent, trailing newline) so git diffs and merges stay readable.

### 6.1 Item

Schema: `https://amet13.github.io/proofback/schema/v1.json`. Every round file names
it in `$schema` and carries `schema_version`.

```jsonc
{
  "id": "pb_7k2m9q",
  "round": "2026-11-25-supervisor",
  "kind": "replace",
  "author": "Prof. Hale",
  "created": "2026-11-24T18:03:00Z",
  "body": "Replace with \"robust\"",
  "suggestion": { "replace_with": "robust" },
  "target": {
    "file": "chapters/method.tex",
    "range": { "start": 1832, "end": 1838 },
    "line": 41, "column": 18,
    "quote": { "exact": "strong", "prefix": "The estimator is ", "suffix": " under" }
  },
  "anchor": { "state": "anchored", "confidence": "exact",
              "reason": "SyncTeX line 41 and a unique text match" },
  "origin": { "type": "pdf", "page": 14, "nm": "a7f3...", "quads": [[0, 0, 0, 0, 0, 0, 0, 0]],
              "pdf_sha256": "..." },
  "status": "open",
  "replies": [{ "author": "me", "body": "...", "at": "2026-11-26T09:12:00Z" }],
  "resolution": { "commit": null }
}
```

- **IDs** are `pb_` plus 6 to 8 base32 characters of a hash of the round and the
  annotation's identity (`/NM` when present, otherwise page, type and quads).
  Re-importing the same PDF is therefore idempotent and never duplicates items.
- **Status** (the author's decision): `open`, `done`, `wontfix`, `discuss`,
  `duplicate`. **Anchor state** (proofback's knowledge): separate, see §5.4.
- **Authors**: stored as written in the PDF. `--anonymize-authors` replaces them with
  `Reviewer 1`, `Reviewer 2` for blind review (see §10).

---

## 7. Command reference

| Command | Purpose |
|---|---|
| `init` | Create `.proofback/config.toml` and the cache ignore rule |
| `import <file>` | Import a PDF (v0.1), notes file (v0.5) or DOCX (v0.6) as a round |
| `list` | Items as a table, `--json`, or `--md` (GitHub task list); filters by round, status, file, kind |
| `show <id>` | One item with source context |
| `triage` | Terminal UI to confirm, resolve and reply (v0.2) |
| `resolve <id>` | Set status and reply |
| `patch [ids]` | Unified diff for `replace`, `delete`, `insert` items |
| `apply [ids]` | Write those edits (refuses if the anchored text changed since import) |
| `sync` | Re-anchor items to the current source |
| `answer -o <pdf>` | Reviewer's PDF with a reply annotation on every note |
| `letter` | Response letter as LaTeX or Markdown |
| `inline` | Optional `% proofback pb_xxx:` comments or `\todo{}` in the source |
| `sarif` | SARIF for GitHub code scanning |
| `stats` | Counts by round, status, file and reviewer |
| `mcp` | Run the MCP server on stdio (v0.3) |
| `doctor` | Check SyncTeX presence, encodings and project root |

Global flags: `--json`, `--project <dir>`, `--quiet`, `--color auto|always|never`.

**Exit codes** (D-15): `0` success; `1` open items remain (only with `--fail-on-open`);
`2` usage error; `3` unreadable or invalid input; `4` refused write (dirty anchor,
encoding, path outside project).

`--json` output is stable within a schema version and documented per command.

---

## 8. AI and agents (D-09)

Principle: **the core is deterministic and offline; AI is a client, not a dependency.**

- **Context for agents.** `show <id> --json` returns the item, the exact source
  range, configurable surrounding lines, the reviewer's quote and the PDF page text.
- **MCP server** (`proofback mcp`, stdio): `list_items`, `get_item`, `propose_patch`
  (checks the diff applies and touches the anchored range), `apply_patch`,
  `resolve_item`, `sync`, `export`. Targets the current MCP specification; no
  network transport.
- **Agent kit.** An Agent Skill ("work through review feedback") in the open
  `SKILL.md` format, a Claude Code plugin, an `AGENTS.md` snippet, Cursor rules, and
  `llms.txt` on the website.
- **Guard rails.**
  - `done` on an edit item (`replace`, `delete`, `insert`) requires the anchored text
    to have changed, unless `--force` is given by a human.
  - `done` on a `comment` or `question` requires a reply.
  - Reviewer text is returned to agents inside a clearly delimited
    `untrusted_reviewer_text` field and documented as data, never instructions
    (prompt-injection defence; see §10).
  - proofback never runs commands and only touches files inside the project root.
- **Optional suggestions** (v0.5). `proofback suggest <id>` calls a model the user
  configures (Anthropic, any OpenAI-compatible endpoint, or a local model). Off by
  default. Shows exactly what will be sent before the first call per project. Never
  applies anything automatically.
- **AI-assisted parsing** only for messy inputs: free-text notes (v0.5) and photos of
  handwritten marks (experimental, after v0.8). Results start at `page` confidence
  until the author confirms them.

---

## 9. Outputs

| Output | Notes |
|---|---|
| Terminal list | Grouped by file; colour only on a TTY |
| JSON | Schema-validated, stable per schema version |
| Markdown | GitHub task list, one line per item with a link to file and line |
| Unified diff | Standard `git apply` format |
| Answered PDF | New file; the original is never modified; status and reply added as replies to each note |
| Response letter | LaTeX, Markdown; quotes the reviewer, states the change, links the commit |
| SARIF 2.1.0 | For code scanning annotations in pull requests |
| Review comments | Through the GitHub Action |

---

## 10. Security and privacy (D-16)

proofback reads files written by other people (PDFs, DOCX, notes) and can write to
the author's source. The threat model:

| Threat | Mitigation |
|---|---|
| Malicious PDF exploiting the PDF parser | pdfium is fuzzed upstream and pinned to a recent Chromium branch; proofback updates it on every Chromium security release. In the browser it runs inside the WebAssembly sandbox. Native imports parse the PDF in a child process with memory and time limits. proofback's own parsers are fuzzed with `cargo-fuzz` in CI. |
| Decompression bombs (`.synctex.gz`, DOCX zip, PDF streams) | Hard limits on decompressed size, entry count, page count and annotation count, all configurable. |
| XML attacks in DOCX | `quick-xml` with no DTD or external entity resolution. |
| Path traversal (SyncTeX paths, `\input`, zip entries, `--at` checkouts) | Every path is canonicalised and must stay inside the project root; symlinks leaving the root are refused. |
| Unwanted writes | Only `apply`, `inline` and `resolve` write. `apply` checks the anchored text is unchanged and never writes outside the root. `--dry-run` everywhere. |
| Prompt injection through reviewer comments | Reviewer text is marked untrusted in JSON and MCP output; the agent kit tells agents to treat it as data; proofback exposes no tool that runs commands. |
| Leaking source to an AI provider | `suggest` is off by default, needs per-project consent, shows the payload, and reads API keys from an environment variable named in the config, never from committed files. |
| Reviewer identity in blind review | `--anonymize-authors` on import; `answer` and `letter` strip PDF metadata (author, creator tool, local paths). |
| GitHub Action abuse | Runs on `pull_request` with a read-only token by default; posting comments uses a separate `workflow_run` job; all third-party actions pinned by commit SHA; least-privilege `permissions`. |
| Supply chain | `cargo-deny` (licences, advisories, sources) in CI, Dependabot, release artefacts with SHA-256 checksums and signed build provenance (GitHub attestations), npm provenance, pinned pdfium binaries with checksums. |
| Website | No third-party requests (fonts self-hosted), Content Security Policy, no cookies or analytics. |

Vulnerabilities are reported privately through GitHub security advisories
(`SECURITY.md`). No telemetry anywhere in the project.

---

## 11. Performance (D-17)

Reference machine: a GitHub-hosted `ubuntu-latest` runner (4 vCPU). Reference input:
a 300-page thesis, 40 source files, 500 annotations.

| Operation | Budget (native) | Budget (browser) |
|---|---|---|
| `import`, cold cache | under 1 s | under 3 s |
| `import`, warm cache | under 300 ms | under 1 s |
| `list` | under 50 ms | not applicable |
| `sync` | under 300 ms | under 1 s |
| Peak memory | under 300 MB | under 400 MB |
| Browser app first load (before an import starts) | not applicable | under 150 KB compressed |

How:

- SyncTeX is streamed and indexed by page; only pages that carry annotations are
  fully decoded.
- PDF text is extracted only for annotated pages.
- Pages are processed in order, in one pass; the budget is met without threads. In
  the browser this runs in a Web Worker.
- Indexes are cached by content hash in `.proofback/cache/`.
- pdfium's WebAssembly build is fetched only when a PDF is dropped and then cached.
- A performance test in CI (`crates/proofback-corpus/tests/perf.rs`) generates the
  reference input and fails when a budget is exceeded.

---

## 12. Quality and accuracy (D-10)

- **Golden corpus** in `fixtures/`: about 15 projects (article, two-column paper,
  150-page thesis with an `\input` tree, beamer, maths-heavy, tables and figures,
  XeLaTeX with Greek and Arabic, LuaLaTeX CJK, biblatex, custom macros), built in CI
  with pinned TeX Live images from 2023 to 2026.
- **Synthetic ground truth.** A generator uses SyncTeX forward search to place every
  annotation type on known source ranges and writes it the way Acrobat, Preview,
  Okular, Zotero and PDF Expert do (each has quirks: quad order, missing `/Contents`,
  flattened replies).
- **Real-world set.** Donated, anonymised review PDFs, opt-in, under CC BY 4.0.
- **Metrics**, per annotation type, layout and mode:
  - exact-range precision and recall,
  - correct-line rate,
  - wrong-anchor rate (any confident anchor that is wrong),
  - `page` fallback rate.
- **Release gates**: wrong-anchor rate under 1%; no metric regresses. Results are
  published on the website at every release.
- Property tests (`proptest`) for the offset map: every normalised character maps
  back inside its source token. Fuzzing for SyncTeX, notes and DOCX parsers.

---

## 13. Browser app (v0.4)

- Lives at `/app` on the website. Loads the core in a Web Worker and pdfium on demand.
- Inputs: annotated PDF, `.synctex.gz`, and sources as a folder
  (`<input webkitdirectory>`, which every current browser supports) or a zip such as
  Overleaf's download. The Overleaf location of the SyncTeX file is documented once verified
  against the current Overleaf UI.
- Layout: reviewer's PDF with numbered marks, item list, source with the anchored
  range highlighted. Keyboard first: `j`/`k` move, `d` done, `w` won't fix, `r` reply.
- Exports: round JSON, patch, Markdown, answered PDF, letter.
- Offline after first load (service worker). No network requests with user data.

---

## 14. Word round trip (v0.6)

Mapping Word edits back is solved at export time:

- `docx export` builds the DOCX with its own writer (no dependencies) or pandoc and
  embeds a hidden bookmark per paragraph carrying the source byte range.
- `import` reads tracked changes (`w:ins`, `w:del`), comments
  (`comments.xml`, `commentRangeStart`/`End`) and replies straight from the XML, and
  resolves them through the nearest bookmarks and the §5 alignment. DOCX items are
  ordinary items.
- A DOCX not exported by proofback still works in text-only mode.

---

## 15. Website (D-13)

- Static HTML, CSS and a small script; no build step; deployed by GitHub Actions to
  GitHub Pages. The browser app joins under `/app` in v0.4.
- Visual language taken from the product: PDF-viewer grey, highlighter yellow for
  reviewer marks and the main action, red only for strike-outs, blue for anything
  proofback links. The demo page is set in Libertinus Serif to look like a typeset
  paper. Common AI-generated design defaults are avoided on purpose.
- Fonts self-hosted and subset (about 120 KB total). No third-party requests.
- Content Security Policy via `<meta>`; the one inline script is allowed by hash.
- Accessibility: WCAG 2.2 AA contrast in both themes, keyboard-operable demo,
  `aria-pressed` state, live region for the demo output, reduced-motion respected.
- `llms.txt`, Open Graph image, 404 page.
- CI checks every pull request: HTML validation and a headless-browser smoke test
  (no console errors, no horizontal overflow at 390 px).

---

## 16. Roadmap

| Milestone | Scope | Exit criteria |
|---|---|---|
| **M0** ✓ | Repository reset, plan, website, Pages deploy | Site live, plan final |
| **M1 Foundations** ✓ | Workspace, `model`, `synctex`, `tex` offset map, fixture builder in CI, fuzzing harness | SyncTeX parser fuzzed for 1 CPU-hour without crashes; offset-map property tests green |
| **M2 = v0.1 Import** ✓ | `pdf`, `anchor`, `init`, `import`, `list`, `show`, `doctor` | Synthetic corpus with SyncTeX: exact-range recall at least 90%, wrong-anchor rate under 1%; performance budgets met |
| **M3 = v0.2 Close the loop** ✓ | `store`, `triage`, `resolve`, `patch`, `apply`, `sync`, `answer`, `letter`; release pipeline (GitHub Releases, installers, Homebrew formula, build provenance) | A full review round done end to end on a fixture thesis |
| **M4 = v0.3 Agents and CI** ✓ | MCP server, agent kit, GitHub Action, SARIF | An agent completes a review round on fixtures in an automated eval |
| **M5 = v0.4 Browser** ✓ | WebAssembly build, `/app`, npm package | Browser results identical to native on the corpus |
| **M6 = v0.5 Notes** ✓ | Notes parser, optional `suggest` | Notes corpus: wrong-anchor rate under 1% |
| **M7 = v0.6 Word** ✓ | DOCX export with anchors, DOCX import | Round trip on fixtures: exact-range recall at least 90% |
| **M8 = v0.7, v0.8** ✓ | VS Code extension, Typst | Same golden suite passes for Typst fixtures |
| **v1.0** | Schema v1 frozen, documentation complete, accuracy report public | No open security advisories; all gates green |

---

## 17. Engineering conventions

- Rust 2024 edition, pinned MSRV, `rustfmt`, `clippy -D warnings`.
- Semantic versioning for the tool; the JSON schema is versioned separately.
- CI: lint, test, golden accuracy, benchmarks, fuzz smoke run, WebAssembly build,
  site checks, Pages deploy. Every third-party action pinned by commit SHA.
- Releases are automatic through `.github/workflows/release.yml` (see
  [`docs/releasing.md`](releasing.md)): started from the GitHub website or by merging a
  version bump; every version field must match a dated `CHANGELOG.md` section; GitHub Release with checksums and provenance, shell and
  PowerShell installers, the Node package and the VS Code extension as release files,
  and an optional Homebrew tap. No package registry accounts (npm, Marketplace,
  crates.io) are used: everything installs from GitHub.
- Changes are recorded in `CHANGELOG.md` under `## Unreleased` as they land.
- Licence MIT OR Apache-2.0 (D-11). Fixtures CC BY 4.0. Bundled fonts SIL OFL 1.1.
- Docs are written for people and agents: each command has a runnable example and
  its JSON shape.

---

## 18. Risks

| Risk | Mitigation |
|---|---|
| pdfium size and build complexity in the browser | On-demand load and caching; the PDF trait keeps a pure-Rust fallback possible |
| pdfium security updates lag | Track Chromium releases; automated update PR; child-process isolation natively |
| SyncTeX is line-level and noisy around macros and floats | Text alignment does the fine work; honest confidence; measured on the corpus |
| PDF apps write annotations inconsistently | Per-app fixtures and a documented quirks table |
| Draft drift between review and current source | `--at <commit>` plus quote-and-context re-anchoring |
| Overleaf users without git | Browser app accepts Overleaf zips; state exported as files |
| Perception as "another AI wrapper" | Deterministic core, published accuracy, AI strictly optional |
| Scope creep into overfull's area | §1.2 boundary: no document-quality checks |

---

## 19. Audit and grill log

The plan was reviewed for correctness, performance, UX, security, privacy and
current tooling before being marked final. Each finding and its resolution:

| # | Area | Finding | Resolution |
|---|---|---|---|
| 1 | Correctness | Example output used numeric IDs in one place and `pb_` IDs elsewhere | `pb_` IDs everywhere, with unique-prefix matching (§2, §6.1) |
| 2 | Correctness | Item status mixed the author's decision with anchoring state | Separate `status` and `anchor.state` (§5.4, §6.1) |
| 3 | Correctness | "`done` requires the text to change" made no sense for questions | Rule applies to edit kinds only; questions need a reply (§8) |
| 4 | Correctness | Re-importing a PDF could duplicate items | Deterministic IDs from `/NM` or annotation identity (§6.1) |
| 5 | Correctness | Offsets were undefined for non-ASCII text and editors | UTF-8 byte ranges, code-point columns, UTF-16 at protocol edges (§4.6) |
| 6 | Correctness | Non-UTF-8 sources and CRLF files could be corrupted by `apply` | Encoding detection, refusal without `--encoding`, line endings preserved (§4.6) |
| 7 | Correctness | Exit codes claimed to match another project without checking | Own exit codes defined (§7) |
| 8 | Correctness | Overleaf SyncTeX location stated without verification | Marked to verify before documenting (§13) |
| 9 | Correctness | Browser app URL pointed at a domain we do not own | Uses the GitHub Pages URL (§2 W6) |
| 10 | Quality | Accuracy targets measured recall only | Wrong-anchor rate under 1% is a release gate; precision before recall (§5, §12) |
| 11 | Security | No threat model | Added (§10) |
| 12 | Security | Untrusted PDFs, archives and XML parsed without limits | Size and count limits, no XML entities, child-process isolation, fuzzing (§10) |
| 13 | Security | Paths from SyncTeX, `\input` and zips could escape the project | Canonicalised and confined to the root (§4.4, §10) |
| 14 | Security | Reviewer comments passed to agents could carry prompt injection | Marked untrusted; no command-running tools (§8, §10) |
| 15 | Security | GitHub Action permissions and action pinning unspecified | Read-only default, `workflow_run` for comments, SHA pinning (§10, §17) |
| 16 | Security | Supply chain unspecified | cargo-deny, cargo-vet, signed artefacts, provenance, pinned pdfium (§10) |
| 17 | Privacy | Reviewer names committed to git break blind review | `--anonymize-authors`; metadata stripped from outputs (§6.1, §10) |
| 18 | Privacy | `suggest` could send source to a provider silently | Off by default, consent, payload preview, keys never committed (§8) |
| 19 | Privacy | Website loaded fonts from Google and jsDelivr | Fonts self-hosted and subset; CSP added (§15) |
| 20 | Performance | Budgets had no reference hardware or memory limit | Reference runner, cold and warm budgets, memory caps (§11) |
| 21 | Performance | Whole-document parsing on every run | Annotated pages only, content-hash cache, parallel pages (§11) |
| 22 | Performance | Browser threading assumed | Single-threaded worker; GitHub Pages cannot send COOP/COEP (§4.7) |
| 23 | Modernity | Plan relied on wasm-pack | `wasm-bindgen` CLI and `wasm-opt` via `xtask` (§4.7) |
| 24 | Modernity | Tool names out of date | `dist` (formerly cargo-dist); Agent Skills format; current MCP spec (§8, §17) |
| 25 | Modernity | File System Access API is Chromium-only | Fallback to directory input and zip (§13) |
| 26 | UX | No way to confirm low-confidence items quickly | `triage` terminal UI (§3, §7) |
| 27 | UX | Command set undocumented | Command reference with flags and exit codes (§7) |
| 28 | UX | Round files would produce noisy git diffs | Deterministic serialisation (§6) |
| 29 | Website | Default AI-generated look | Redesigned from the product's own visual language (§15) |
| 30 | Website | Demo marks hid their text from screen readers; theme toggle had no state | Visible text kept, `aria-pressed` added, live region kept (§15) |
| 31 | Website | No validation in CI, actions pinned by tag only | HTML validation and browser smoke test on pull requests; SHA-pinned actions (§15) |
| 32 | Content | Example names were not neutral | Neutral placeholder names (`Prof. Hale`, `Reviewer 1`) |

---

## 20. Decision log

| ID | Decision |
|---|---|
| D-01 | Independent of overfull; shared conventions only |
| D-02 | Any reviewed LaTeX document; theses are the flagship story; Typst later |
| D-03 | Order of surfaces: CLI, then MCP/agents/Action, then browser app, then VS Code |
| D-04 | One Rust core for native and WebAssembly |
| D-05 | pdfium (via pdfium-render) for annotations, text boxes and writing replies |
| D-06 | Own streaming SyncTeX parser |
| D-07 | TeX normaliser with an offset map, not a TeX engine |
| D-08 | State in `.proofback/` in the author's repository; no server |
| D-09 | Deterministic core; AI through MCP and agents; built-in suggestions optional |
| D-10 | Accuracy measured in CI and published every release |
| D-11 | MIT OR Apache-2.0 |
| D-12 | Name **proofback**; free on GitHub, npm and crates.io when checked (2026-11-25) |
| D-13 | Website visual language taken from the product; static, self-hosted, no third parties |
| D-14 | Precision before recall; never a confident wrong anchor |
| D-15 | Exit codes 0 to 4 as in §7 |
| D-16 | Threat model and mitigations in §10 are requirements, not aspirations |
| D-17 | Performance budgets in §11 are CI-enforced |

---

## 21. Implementation notes

Status as of 2026-11-25: M1 to M8 are implemented and tested. v1.0 waits for the first
tagged release and the public accuracy report.

### 21.1 Measured results

All numbers come from the tests in `crates/proofback-corpus` and the crates' own test
suites, run in CI on every pull request.

| Corpus | Items | Exact | Wrong | Gate |
|---|---|---|---|---|
| LaTeX PDF annotations, with SyncTeX | 256 | 97.8% (line 99.6%) | 0.4% | exact at least 90%, wrong under 1% |
| LaTeX PDF annotations, text only | 256 | 43.3% (line 95.3%, page 4.7%) | 0% | wrong under 1% |
| Notes in an email | 150 | 100 exact, 50 region | 0 | wrong under 1% |
| Word round trip | 60 | 60 | 0 | exact at least 90% |
| Typst | 120 | 120 | 0 | same as LaTeX |
| Browser and npm against native | 40 | identical | | identical |

Performance on the reference input (322 pages, 500 notes): cold import 336 ms, warm
import 34 ms, sync 0.2 ms, peak memory 99 MB. All within §11.

Browser app: the page itself is about 14 KB compressed (plus fonts). The engine is
fetched when the first import starts and then cached: the core is 670 KB and pdfium
2419 KB, compressed.

### 21.2 Changes from v1.0 of the plan

| Area | v1.0 said | Implemented | Why |
|---|---|---|---|
| MSRV | pinned, unspecified | Rust 1.92 | Required by the embedded Typst compiler |
| PDF layer | pdfium for everything | `lopdf` for annotations and writing, pdfium for text boxes | Pure-Rust annotation code is easier to fuzz and works without pdfium |
| MCP | `rmcp` | Own JSON-RPC over stdio (spec 2025-11-25, older versions negotiated) | Small surface, no async runtime, fully tested |
| Benchmarks | Criterion, fail on 10% regression | Budget test in CI | Shared runners are too noisy for a 10% threshold |
| Parallelism | `rayon` | Single pass | Budgets met with large margin; same code path as WebAssembly |
| Releases | `dist` | Own release workflow | Needed custom steps (pdfium, provenance, Homebrew, Node and VS Code packages) |
| Supply chain | `cargo-vet`, Sigstore | `cargo-deny`, GitHub build attestations | Same guarantees with less maintenance |
| API keys | environment or OS keychain | environment variable named in config | No native keychain dependency |
| Encodings | `--encoding` flag | Non-UTF-8 sources skipped and reported | Rare in practice; never writes a file it cannot round-trip |
| Response letter | LaTeX, Markdown, DOCX | LaTeX, Markdown | Markdown converts to DOCX with pandoc if needed |
| Word export | make4ht or pandoc | Built-in writer or pandoc | No TeX tool needed at export time |
| Browser folders | File System Access API with fallback | `webkitdirectory` everywhere, plus zip | One path that works in every browser |
| Typst | `typst-ide` jump API | Embedded compiler with a glyph to source span index | Character-level precision, no extra dependency |
| Corpus | about 15 TeX Live projects | 3 pdflatex fixtures plus 1 Typst project with synthetic annotations | Grows with donated real-world reviews |

### 21.3 Additional audit findings during implementation

| # | Area | Finding | Resolution |
|---|---|---|---|
| 33 | Correctness | Sticky notes anchored to the wrong line | Anchor point is the icon's top-left corner |
| 34 | Correctness | Items reported `lost` after a replacement edit | Sync moves them to the nearest changed location |
| 35 | Correctness | TeX normaliser span bugs found by property tests | Fixed; regressions kept in `proptest-regressions` |
| 36 | Portability | Windows checkouts converted fixture line endings | `.gitattributes` keeps fixtures byte-exact; paragraph splitting accepts CRLF |
| 37 | Supply chain | `zip` advisories in an older version | Upgraded to `zip` 9.7.3; two Typst-only `quick-xml` advisories ignored with reasons |
| 38 | Release | The Pages deploy shipped `/app` without its engine, and the browser and npm parity tests did not run on pull requests | The Website workflow builds the engine, runs both parity tests on every pull request and deploys the built engine |
| 39 | Release | Versions were bumped by hand in six files and only the GitHub Release was automated, while the docs promised npm | `cargo xtask bump` and `release-check`; the workflow verifies the tag and attaches the Node package and VS Code extension to the release; no registry accounts needed |
| 40 | Website | 19 font sizes, about 50 hard-coded colours, 7 weights, decorative filler | One token set for site and app; filler removed |

