# GitHub Action

The action imports a review PDF committed to the repository, writes a job summary,
optionally uploads SARIF to code scanning, and can post the items as a pull-request
review.

Posting comments needs a write token, which a pull request from a fork must never
get. So the work is split in two workflows: `import` runs on the pull request with
a read-only token, and `comment` runs afterwards through `workflow_run` with
`pull-requests: write`.

## Import (on the pull request)

```yaml
# .github/workflows/review-import.yml
name: Review import
on:
  pull_request:
    paths: ["reviews/**.pdf"]
permissions:
  contents: read
jobs:
  import:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Amet13/proofback@v0.1.0
        with:
          pdf: reviews/supervisor-ch3.pdf
          synctex: build/main.synctex.gz
      - uses: actions/upload-artifact@v4
        with:
          name: proofback-review
          path: proofback-review.json
```

## Comment (after the import finished)

```yaml
# .github/workflows/review-comment.yml
name: Review comments
on:
  workflow_run:
    workflows: ["Review import"]
    types: [completed]
permissions:
  contents: read
  pull-requests: write
  actions: read
jobs:
  comment:
    if: github.event.workflow_run.conclusion == 'success'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: proofback-review
          run-id: ${{ github.event.workflow_run.id }}
          github-token: ${{ github.token }}
      - uses: Amet13/proofback@v0.1.0
        with:
          mode: comment
```

Items on lines that are part of the pull request's diff become inline comments; the
rest are listed in the review body with links to the exact lines. Reviewer text is
always quoted, never interpreted.

## Inputs

| Input | Default | Meaning |
|---|---|---|
| `mode` | `import` | `import` or `comment` |
| `pdf` | | Annotated PDF (import) |
| `synctex` | auto | SyncTeX file of the build the reviewer read |
| `round` | date + name | Round name |
| `working-directory` | `.` | LaTeX project directory |
| `version` | `latest` | proofback release to install |
| `fail-on-open` | `false` | Fail when open items remain |
| `upload-sarif` | `false` | Upload to code scanning (`security-events: write`) |
| `review-file` | `proofback-review.json` | Hand-over file between the two jobs |

Outputs: `items`, `open`, `sarif`.

In production, pin actions by commit SHA rather than tag.
