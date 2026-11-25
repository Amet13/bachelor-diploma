# proofback — project plan

> **Every margin note, back in your source.**
> proofback takes the feedback a reviewer leaves on a compiled PDF — highlights,
> strike-outs, sticky notes, inserted text, an email full of "p. 12, ¶2: unclear" —
> and lands each one on the exact file, line and character range of your LaTeX
> source. Then it helps you (or your coding agent) work through them, and sends the
> answers back.

Status: **planning** (M0). This document is the single source of truth for scope,
design and roadmap. Decisions are recorded inline as `D-xx` so later changes can
reference them.

---

## 1. The problem

Supervisors, co-authors, reviewers and proofreaders do not edit `.tex` files.
They read the PDF and mark it up in Acrobat, Preview, Zotero, Okular, an iPad app,
or they reply with an email listing page numbers. The author then:

1. opens the annotated PDF next to the editor,
2. finds every mark by eye,
3. hunts for the corresponding sentence in a tree of `\input` files,
4. edits, and tries to remember what was done, skipped or disputed,
5. writes a reply ("fixed", "I disagree because…") — by hand, again.

For a thesis this repeats for every chapter and every draft; for a paper it ends in
a hand-written response-to-reviewers letter. Comments get lost, replies drift from
what was actually changed, and the supervisor has no way to check.

### What exists (researched Nov 2026)

| Tool | What it does | What's missing |
|---|---|---|
| pdfannots (≈680★) | Extracts PDF annotations to Markdown with page + quoted text | No mapping to source files or lines |
| LaTeX Workshop, TeXstudio, Skim, Okular, Emacs pdf-tools | SyncTeX click-to-jump, one location at a time | No bulk import of annotations, no tracking |
| pandoc `--track-changes` | DOCX → LaTeX with insertions/deletions kept | Regenerates a *new* `.tex`; your source formatting is gone |
| Overleaf, OpenAI Prism | In-platform comments and track changes | Reviewer must adopt the platform and read raw LaTeX; no PDF/Word input |
| 2026 AI "skills" (0–3★) | LLM re-applies Word edits to `.tex` | Non-deterministic, no anchors, no tracking |

**Gap:** nothing turns feedback in the formats reviewers *actually* use into a
precise, trackable list of changes against the *existing* source.

### Non-goals (and the boundary with overfull)

[overfull](https://github.com/Amet13/overfull) is the pre-submission check:
references, compiled-PDF problems, venue rules, templates. proofback never does any
of that. proofback starts **after** someone has read your document and has opinions.
The two tools share conventions (JSON output shape, exit codes, agent-first docs) so
one agent can drive both, but no code and no feature overlap (D-01).

Also out of scope: being an editor, being a collaboration platform, hosting anything,
grammar checking, generating content.

---

## 2. Users and core workflows

Primary persona: **the author** (student, researcher) who keeps the LaTeX source in a
folder or git repo, possibly on Overleaf. Secondary: **the reviewer** who never
installs anything, and **the coding agent** acting for the author.

Thesis is the flagship story; any reviewed LaTeX document is supported (D-02).

### W1 — Annotated PDF → checklist (v0.1)
```bash
proofback import review-ch3.pdf --round "supervisor-2026-11"
proofback list
```
```
#  where                         kind     note
1  chapters/method.tex:42:17-58  replace  "use 'robust' not 'strong'"        ✓ exact
2  chapters/method.tex:57        comment  "Why this threshold?"              ✓ line
3  chapters/results.tex:12:1-88  delete   —                                  ✓ exact
4  p.14 (figure)                 comment  "axis labels unreadable"           ? figure
```

### W2 — Work through it, with or without an agent (v0.2–0.3)
```bash
proofback patch 1 3 | git apply          # mechanical edits as a reviewable diff
proofback resolve 2 --reply "Chosen by cross-validation, see §3.4" --status done
proofback resolve 4 --status wontfix --reply "Figure regenerated in next draft"
```
Or in Claude Code / Codex / Cursor: *"work through the open proofback items in
chapter 3"* — the agent uses the MCP server, proposes edits, and records replies.

### W3 — Send the answers back (v0.2)
```bash
proofback answer -o review-ch3-answered.pdf   # the reviewer's own PDF, every note
                                              # now carrying status + reply
proofback letter --format latex > response.tex  # response-to-reviewers letter
```

### W4 — The next draft (v0.2)
After edits, items stay anchored: `proofback sync` re-locates every open item in the
current source (quote + context selectors, see §5.4) and reports anything it can no
longer find.

### W5 — Email / free-text notes (v0.5)
```bash
proofback import notes.txt   # "p. 12, ¶2: unclear", "§3.1 'state of the art' → 'current'"
```

### W6 — No terminal at all (v0.4)
Open **proofback.app on GitHub Pages**, drop the annotated PDF, the `.synctex.gz`
and the source folder (or Overleaf zip). Everything runs locally in WebAssembly —
nothing is uploaded. Export the checklist, patch, answered PDF or letter.

### W7 — Word round trip (v0.6)
```bash
proofback docx export -o thesis-for-review.docx   # with invisible source anchors
proofback import thesis-reviewed.docx              # tracked changes + comments → items
```

### W8 — In the PR (v0.3)
The GitHub Action imports a review PDF committed to the repo (or attached to an
issue) and posts each item as an inline pull-request review comment on the right
line.

---

## 3. Product surfaces (D-03)

| Surface | Version | Notes |
|---|---|---|
| `proofback` CLI | v0.1 | Native single binary: Linux, macOS, Windows; `--json` everywhere |
| MCP server (`proofback mcp`) | v0.3 | stdio; tools listed in §7 |
| Agent kit | v0.3 | `AGENTS.md` snippet, Claude Code plugin + skill, Codex/Cursor rules, `llms.txt` |
| GitHub Action | v0.3 | PR review comments + job summary |
| Browser app (GitHub Pages) | v0.4 | Same Rust core compiled to WASM; zero upload |
| npm package | v0.4 | WASM build for Node and `npx proofback` |
| VS Code extension | v0.7 | Tree view, gutter marks, quick-fix apply, reply box |
| Typst support | v0.8 | Via the Typst compiler's span/jump API |

---

## 4. Architecture

### 4.1 Language and runtime (D-04)

**Rust core, compiled twice:** native (CLI, MCP, Action) and `wasm32` (browser,
npm). One implementation means the browser app and the CLI give identical answers,
and one golden test suite covers both.

Why not TypeScript like overfull: performance on 300-page theses with hundreds of
annotations, a single static binary with no Node requirement, and a clean WASM
story. Why not Python: distribution to non-programmer authors.

### 4.2 Workspace layout

```
crates/
  proofback-model     # FeedbackItem, Round, selectors, JSON schema (serde + schemars)
  proofback-synctex   # streaming .synctex(.gz) parser, inverse/forward search
  proofback-pdf       # annotation + positioned text extraction (pdfium adapter)
  proofback-tex       # TeX-aware tokenizer: source ⇄ "rendered text" offset map
  proofback-anchor    # matching pipeline (§5), confidence scoring
  proofback-store     # .proofback/ state, rounds, re-anchoring (sync)
  proofback-emit      # checklist, patch, answered PDF, letter, SARIF, PR comments
  proofback-notes     # free-text/email note parser (v0.5)
  proofback-docx      # Word export with anchors + tracked-change import (v0.6)
  proofback-cli       # clap CLI
  proofback-mcp       # MCP server (rmcp)
  proofback-wasm      # wasm-bindgen API for browser + npm
site/                 # GitHub Pages: landing page, docs, and /app (browser app)
fixtures/             # golden corpus (§9)
docs/                 # this plan, ADRs, schema docs
```

### 4.3 PDF layer (D-05)

**pdfium via `pdfium-render`** for both native and WASM:
- reads every annotation type we need (Highlight, Underline, StrikeOut, Squiggly,
  Text/sticky note, FreeText, Caret, Ink, Square/Circle, Line) with QuadPoints,
  Rect, Contents, RichContents, author, dates, colour, and reply chains (`/IRT`);
- gives per-character text boxes, so we can read the text *under* a highlight even
  when the annotating app didn't store it;
- can **write** annotations, which `answer` needs.

Licences: Apache-2.0 / BSD. Native builds bundle a prebuilt pdfium; the browser
lazy-loads the pdfium WASM (~4–5 MB, cached by a service worker) only when a PDF is
dropped, so the landing page stays tiny.

Fallback considered: `lopdf` (pure Rust) for annotation dictionaries + own text
extraction. Rejected for v1: font/encoding/ToUnicode handling is a project in itself.

### 4.4 SyncTeX layer (D-06)

Own streaming parser (`proofback-synctex`): the format is small and well specified,
and we need things generic parsers don't expose — every box record for a page, not
just the "best" hit, so we can score multiple candidate lines. Gzip via
`miniz_oxide` (WASM-friendly). Target: parse a 300-page thesis's synctex in <150 ms
native.

If there is no synctex file, the pipeline still works using text alignment only
(lower confidence, §5.3).

### 4.5 TeX normalizer (D-07)

`proofback-tex` turns source into an approximation of the text that appears in the
PDF, **keeping a byte-offset map back to the source**:
- strips comments, most commands, braces; keeps arguments of text commands
  (`\emph{x}` → `x`), drops `\label`, `\cite`, `\ref` bodies to placeholders;
- `~`, `\,`, `--`, `---`, quotes, accents (`\'e`), ligatures, `\\`;
- inline math → a single placeholder token; display math/figures/tables marked as
  blocks;
- user macros: simple `\newcommand` without parameters are expanded; others become
  opaque tokens;
- follows `\input` / `\include` / `\subfile`, using the synctex file list when present.

It is deliberately **not** a TeX engine. It only needs to be good enough for fuzzy
alignment inside a small window that SyncTeX has already narrowed down.

---

## 5. Matching pipeline

For each annotation:

1. **Extract** — type, page, quads/rect, note text, author, date, colour, replies.
2. **Read the marked text** — characters inside the quads, plus ~80 chars of context
   before and after (from the PDF text layer). Handle hyphenation at line ends and
   two-column layouts.
3. **Coarse locate (SyncTeX)** — inverse search for each quad (centre + corners) →
   ranked candidate `(file, line)` set. For point annotations (sticky notes), take
   the nearest text line on the page.
4. **Fine locate (text alignment)** — normalise the candidate lines ±N lines with
   `proofback-tex`, then align the marked text + context using a banded
   Smith–Waterman / bitap search. Map the hit back through the offset map to an
   exact source byte range.
5. **Classify** the item: `comment`, `question`, `delete`, `insert`, `replace`,
   `highlight`, `figure`, `math`, `general`. StrikeOut + note → `replace` with a
   suggested replacement; Caret + FreeText → `insert`; strike-out alone → `delete`.
6. **Score** confidence: `exact` (character range) / `line` / `region` (figure,
   table, display math) / `page` (gave up; needs a human). Each score comes with a
   human-readable reason.

### 5.1 Hard cases we design for from day one
Two-column layouts; hyphenated words across lines; ligatures; text produced by macros;
`\input` trees; floats placed far from their source; captions; footnotes; display
math; tables; generated text (bibliography, TOC — flagged as `generated` and pointed
at the `.bib` key or heading where possible); Cyrillic/CJK via XeLaTeX/LuaLaTeX;
reviewer annotated an **older draft** than the current source.

### 5.2 Draft mismatch
Each round records the git commit (if any) and a hash of the PDF and synctex it was
imported against. If the reviewer's PDF is older than the current source:
`proofback import review.pdf --synctex old.synctex.gz --at <commit>` resolves
against that commit's files, then `sync` carries the anchors forward to `HEAD`.
If only the PDF exists, step 4 runs against the whole project (text-only mode).

### 5.3 Text-only mode
Without synctex: full-project fuzzy search for the marked text + context, using
page order as a prior. Expect lower `exact` rates; the accuracy report (§9) measures
both modes separately.

### 5.4 Anchors (the data that survives edits)
Each item stores W3C Web Annotation–style selectors:
- `TextQuoteSelector` — exact quote + prefix + suffix (normalised),
- `TextPositionSelector` — file + byte range + line/column at the recorded commit,
- `PdfSelector` — page, quads, PDF hash (provenance; never used to re-anchor).

`sync` re-anchors by quote+context first, position second, and reports `moved`,
`changed` (quote edited — probably addressed), or `lost`.

---

## 6. Data model and state

All state lives in the author's project, committed to git (D-08), never on a
server:

```
.proofback/
  config.toml                 # project root, main file, build dir, default author
  rounds/
    2026-11-25-supervisor.json
    2027-01-10-reviewer-2.json
```

`FeedbackItem` (JSON, versioned schema published at `site/schema/v1.json`):
```jsonc
{
  "id": "pb_7k2m9q",                 // stable, short, human-typeable
  "round": "2026-11-25-supervisor",
  "kind": "replace",
  "author": "Dr. Ivanova",
  "created": "2026-11-24T18:03:00Z",
  "body": "use 'robust' not 'strong'",
  "suggestion": { "replace_with": "robust" },
  "target": {
    "file": "chapters/method.tex",
    "range": { "start": 1832, "end": 1838 },
    "line": 42, "column": 17,
    "quote": { "exact": "strong", "prefix": "the estimator is ", "suffix": " under" }
  },
  "origin": { "type": "pdf", "page": 14, "quads": [[…]], "pdf_sha256": "…" },
  "confidence": { "level": "exact", "reason": "synctex line + unique text match" },
  "status": "open",                  // open | done | wontfix | discuss | duplicate
  "replies": [ { "author": "me", "body": "…", "at": "…" } ],
  "resolution": { "commit": null }
}
```

Exit codes follow overfull's convention so CI scripts can treat both alike:
`0` ok, `1` open items remain (only with `--fail-on-open`), `2` usage error,
`3` input error.

---

## 7. AI compatibility (D-09)

Principle: **the core is deterministic and offline; AI is a client, not a
dependency.**

- **Structured context for agents.** `proofback show <id> --json` returns the item,
  the exact source range, ±20 lines of context, the reviewer's quote and the PDF
  page text — everything an agent needs to make a good edit without opening a PDF.
- **MCP server** (`proofback mcp`), tools:
  `list_items(filter)`, `get_item(id, context_lines)`, `propose_patch(id, diff)`
  (validates the diff applies and touches the anchored range), `apply_patch(id)`,
  `resolve_item(id, status, reply)`, `sync()`, `export(format)`.
- **Agent kit:** a Claude Code plugin + skill ("work through review feedback"),
  an `AGENTS.md` snippet for any agent, Codex/Cursor rule files, and `llms.txt` /
  `llms-full.txt` on the website.
- **Guard rails:** agents may *propose* edits and replies; `resolve` with status
  `done` requires the anchored range to have actually changed (or `--force`), so an
  agent can't silently mark things fixed.
- **Optional built-in suggestions** (v0.5): `proofback suggest <id>` calls a model
  the user configures (Anthropic, OpenAI-compatible, or local via Ollama) and
  returns a patch for review. Off by default, never applied automatically, never
  sends anything unless configured.
- **AI-assisted parsing** for the messy inputs only: free-text notes (v0.5) and
  handwritten scans (experimental, v0.9) — always shown with low confidence until
  confirmed.

---

## 8. Outputs

| Command | Output |
|---|---|
| `list` | Terminal table, `--json`, `--md` (GitHub task list) |
| `show <id>` | Item with source context |
| `patch [ids]` | Unified diff for mechanical items (replace / delete / insert) |
| `apply [ids]` | Same, written to disk (refuses on dirty anchored ranges) |
| `inline` | Optional `% proofback pb_7k2m9q: …` comments or `\todo{}` (todonotes) in source |
| `answer -o file.pdf` | The reviewer's PDF with a reply annotation on every note: status + response |
| `letter` | Response-to-reviewers / supervisor reply: LaTeX, Markdown, or DOCX |
| `sarif` | SARIF for GitHub code scanning (inline marks on PR diffs) |
| `gh review` | PR review comments (Action) |
| `stats` | Per round: open/done/wontfix, by chapter, by reviewer |

---

## 9. Quality: accuracy you can check (D-10)

Accuracy is the product. We publish it.

- **Golden corpus** in `fixtures/`: ~15 real-shaped LaTeX projects (article,
  two-column paper, 150-page thesis with `\input` tree, beamer, math-heavy,
  tables/figures, XeLaTeX Cyrillic, LuaLaTeX CJK, biblatex, custom macros).
  Built in CI with pinned TeX Live containers (2023–2026) to catch engine drift.
- **Synthetic ground truth:** a generator uses SyncTeX *forward* search to place
  annotations of every type on known source ranges, writing them into the PDF the
  way Acrobat, Preview, Okular, Zotero and PDF Expert do (each app has quirks:
  quad order, missing `/Contents`, flattened replies).
- **Real-world set:** donated, anonymised reviewer PDFs (opt-in, with licence).
- **Metrics:** exact-range precision/recall, line accuracy, `page`-fallback rate —
  per annotation type, per layout, with/without synctex. Published on the site as
  `/accuracy` and regenerated by CI on every release. Release gate: no regression.
- **Performance budget** (criterion benches, CI-gated): 300-page thesis + 500
  annotations — import < 1 s native, < 3 s in browser; `sync` < 300 ms.
- Unit + property tests (`proptest`) for the offset map: every normalised character
  must map back inside its source token.

---

## 10. Browser app (v0.4)

- Lives at `/app` on the GitHub Pages site; loads the WASM core and pdfium on demand.
- Inputs: annotated PDF + `.synctex.gz` + sources (folder via File System Access
  API, or a `.zip` such as Overleaf's download). Overleaf: the synctex file is under
  *Logs and output files → Other logs and files*.
- UI: three panes — reviewer's PDF with numbered marks · item list · source with the
  anchored range highlighted. Keyboard-first (j/k, d = done, w = won't fix, r = reply).
- Exports: `.proofback/` round JSON, patch, Markdown checklist, answered PDF, letter.
- Zero network after load; works offline (service worker); no analytics.

---

## 11. Word round trip (v0.6)

The hard part of Word is mapping edits back. proofback solves it at **export**:
- `docx export` builds the DOCX with make4ht (best fidelity) or pandoc, and embeds an
  invisible bookmark per paragraph/sentence carrying `file:byte-range`.
- On import, tracked changes (`w:ins`, `w:del`), comments (`comments.xml`,
  `commentRangeStart/End`) and replies are read directly from the XML and resolved
  via the nearest bookmarks + text alignment — the same pipeline as §5, so DOCX
  items are first-class `FeedbackItem`s.
- A DOCX not exported by proofback still works in text-only mode.

---

## 12. Roadmap

| Milestone | Scope | Exit criteria |
|---|---|---|
| **M0 — Reset** (this PR) | Repo reset, plan, website, Pages deploy | Site live; plan agreed |
| **M1 — Foundations** | Workspace, `model`, `synctex`, `tex` offset map, fixture builder in CI | Synctex parser fuzzed; offset-map property tests green |
| **M2 — v0.1 Import** | `pdf`, `anchor`, `import` / `list` / `show` | ≥90 % exact-range on synthetic corpus with synctex; perf budget met |
| **M3 — v0.2 Close the loop** | `store`, `resolve`, `patch`, `apply`, `sync`, `answer`, `letter`, binaries via cargo-dist + Homebrew | An entire review round done end-to-end on a fixture thesis |
| **M4 — v0.3 Agents & CI** | MCP server, plugin/skill, AGENTS.md kit, GitHub Action, SARIF | Agent completes a review round on fixtures in an eval |
| **M5 — v0.4 Browser** | WASM build, `/app`, npm package | Same golden results in browser as native |
| **M6 — v0.5 Notes & suggest** | Free-text notes parser, optional `suggest` | — |
| **M7 — v0.6 Word** | DOCX export with anchors + import | Round trip on fixtures ≥90 % exact |
| **M8 — v0.7–0.8** | VS Code extension, Typst | — |
| **v1.0** | Schema v1 frozen, accuracy report public, docs complete | — |

---

## 13. Engineering conventions

- Rust 2024 edition, MSRV pinned; `clippy -D warnings`, `rustfmt`, `cargo deny`.
- Releases: cargo-dist (GitHub Releases, shell/PowerShell installers, Homebrew tap),
  crates.io, npm (WASM). Signed artefacts + SLSA provenance.
- CI: lint · test · golden accuracy · benches · WASM build · site build/deploy.
- Conventional commits; changelog generated on release.
- Licence: **MIT OR Apache-2.0** (D-11). Fixtures donated under CC BY 4.0.
- Privacy: no telemetry, no network calls unless the user configures `suggest`.
- Docs are agent-first: every command documented with a runnable example and its
  JSON output shape.

---

## 14. Risks

| Risk | Mitigation |
|---|---|
| pdfium WASM size / build complexity | Lazy load + cache; `lopdf` fallback path kept possible behind the `proofback-pdf` trait |
| SyncTeX is line-level and noisy with macros/floats | Text alignment does the fine work; confidence levels are honest; corpus measures it |
| Reviewers' PDF apps write annotations inconsistently | Per-app fixtures; quirks table in docs |
| Draft drift between review and current source | `--at <commit>` + quote/context re-anchoring |
| Overleaf users without git | Browser app accepts Overleaf zips; state exported as a file |
| "Yet another AI wrapper" perception | Deterministic core, published accuracy, AI strictly optional |
| Scope creep into overfull's territory | §1 boundary; no checks of document quality, ever |

---

## 15. Decision log

| ID | Decision |
|---|---|
| D-01 | Independent of overfull; shared conventions only |
| D-02 | Any reviewed LaTeX document; thesis is the flagship story; Typst later |
| D-03 | CLI → MCP/agents/Action → browser app → VS Code |
| D-04 | Rust core, native + WASM |
| D-05 | pdfium (pdfium-render) for annotations, text boxes and writing replies |
| D-06 | Own streaming SyncTeX parser |
| D-07 | TeX normaliser with offset map, not a TeX engine |
| D-08 | State in `.proofback/` in the author's repo; no server |
| D-09 | Deterministic core; AI via MCP/agents; built-in suggestions optional |
| D-10 | Published, CI-gated accuracy metrics |
| D-11 | MIT OR Apache-2.0 |
| D-12 | Name **proofback** — free on GitHub, npm and crates.io (checked 2026-11-25) |
| D-13 | Website styled after the product itself (PDF-viewer grey, highlighter yellow, strike-out red, mapping blue; Computer Modern on the demo page), avoiding common AI-generated design defaults; dependency-free static HTML/CSS/JS on GitHub Pages; app added under `/app` in M5 |
