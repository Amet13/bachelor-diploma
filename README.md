# proofback

**Every margin note, back in your source.**

Your supervisor, co-author or reviewer marked up the PDF. proofback finds the exact
**file, line and characters** in your LaTeX source that each highlight, strike-out,
caret and note refers to, tracks what you did about it across drafts, and sends the
answers back: the reviewer's own PDF with a reply on every note, or a response letter.

> **Status: plan final, implementation not started.** Nothing to install yet.
> Read the plan in [`docs/PLAN.md`](docs/PLAN.md).
> Website: <https://amet13.github.io/proofback/>

## Why

Reviewers don't edit `.tex`. They annotate PDFs in Acrobat, Preview, Zotero or on a
tablet, or send an email of page numbers. Authors then hunt for every mark by hand,
lose some, and write replies that drift from what changed. Existing tools either
extract annotations without source locations, jump to one location at a time,
regenerate your source from Word, or require every reviewer to join a platform.

## What it will look like

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

## Roadmap

| Version | Scope |
|---|---|
| v0.1 | PDF import, anchoring, `list`, `show`, `doctor` |
| v0.2 | `triage`, `resolve`, `patch`, `apply`, `sync`, answered PDF, response letter |
| v0.3 | MCP server, agent kit, GitHub Action |
| v0.4 | Browser app (WebAssembly), npm package |
| v0.5 | Email notes, optional AI suggestions |
| v0.6 | Word round trip |
| v0.7+ | VS Code extension, Typst, schema 1.0 |

## Repository layout

```
docs/PLAN.md            the project plan (final), including the audit log
site/                   website, static, no build step
tools/                  website checks: HTML validation and a headless smoke test
.github/workflows/      website check on pull requests, deploy to GitHub Pages
SECURITY.md             how to report vulnerabilities
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
