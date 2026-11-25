# Security policy

proofback reads files written by other people (PDFs, Word documents, notes) and can
write to your source files, so security reports are taken seriously.

## Reporting a vulnerability

Please report vulnerabilities privately through
[GitHub security advisories](https://github.com/Amet13/proofback/security/advisories/new).
Do not open a public issue.

Include what you found, how to reproduce it (a minimal PDF, DOCX or project if
relevant), and the impact you expect. You should get a first reply within 7 days.

## Scope

- The `proofback` CLI, MCP server, GitHub Action, browser app and npm package.
- The website at <https://amet13.github.io/proofback/>.

The threat model and the mitigations proofback commits to are in
[`docs/PLAN.md` §10](docs/PLAN.md#10-security-and-privacy-d-16).

## Supported versions

Before 1.0, only the latest release receives security fixes.
