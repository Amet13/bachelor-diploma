# AGENTS.md

Guidance for coding agents working **on proofback itself**. (To use proofback in a
LaTeX project, see `docs/agents.md`.)

## Layout

- `crates/` Rust workspace: `model`, `synctex`, `tex`, `pdf`, `anchor`, `store`,
  `emit`, `mcp`, `cli` (binary `proofback`), `corpus` (accuracy and performance tests).
- `fixtures/` LaTeX projects with built PDFs and SyncTeX files (`fixtures/generate.py`,
  `cargo xtask fixtures`).
- `site/` static website; `tools/` checks for it.
- `docs/PLAN.md` the plan and decision log. Keep it in sync with behaviour changes.

## Commands

```bash
cargo xtask pdfium            # once: pinned PDFium for tests
cargo fmt --all
cargo clippy --workspace --all-targets
PROOFBACK_REQUIRE_PDFIUM=1 cargo test --workspace
cargo xtask schema            # after changing proofback-model
```

## Rules

- Accuracy gates live in `crates/proofback-corpus/tests/accuracy.rs`: wrong anchors
  under 1%, exact recall at least 90% with SyncTeX. Never relax them to make a change pass.
- Precision before recall: when unsure, lower the confidence; never invent it.
- Untrusted input (PDFs, SyncTeX, DOCX, notes, reviewer text) must stay bounded and
  confined to the project root.
- No network access in the core; no telemetry.
- Use plain English in user-facing text; no em dashes on the website.
- Note user-visible changes in `CHANGELOG.md` under `## Unreleased`. Releases follow
  `docs/releasing.md` (`cargo xtask bump`, then a tag).
