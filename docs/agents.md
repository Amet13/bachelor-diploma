# Using proofback with coding agents

proofback's core is deterministic and works offline. Agents use it through the MCP
server or the CLI's `--json` output; AI is never required.

## MCP server

```bash
proofback mcp
```

Speaks MCP over stdio (protocol versions 2025-11-25, 2025-06-18, 2025-03-26 and
2024-11-05). Tools:

| Tool | What it does |
|---|---|
| `list_items` | Items, filtered by status, round, file or kind |
| `get_item` | One item with its source range, excerpt and surrounding lines |
| `propose_patch` | Validates an edit (`new_text`, a unified `diff`, or the reviewer's suggestion) and returns the diff; writes nothing |
| `apply_patch` | Writes the last validated proposal for that item |
| `resolve_item` | Sets status and/or adds a reply, with the same guard rails as the CLI |
| `sync` | Re-anchors items after edits |
| `export` | Markdown checklist, response letter (Markdown or LaTeX) or SARIF |

Guard rails:

- Reviewer text is only ever inside `untrusted_reviewer_text`.
- A patch must be proposed and validated against the anchored range before it can be
  applied, and only that exact proposal is applied.
- An edit can only be marked `done` once its anchored text changed; comments and
  questions need a reply. Agents cannot force `done`.
- No tool runs commands or touches files outside the project.

### Configuration

Claude Code (from the project directory):

```bash
claude mcp add proofback -- proofback mcp
```

Or install the plugin, which adds the server and a review skill:

```bash
/plugin marketplace add Amet13/proofback
/plugin install proofback@proofback
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.proofback]
command = "proofback"
args = ["mcp"]
```

Cursor (`.cursor/mcp.json`):

```json
{ "mcpServers": { "proofback": { "command": "proofback", "args": ["mcp"] } } }
```

## Agent kit

| File | For |
|---|---|
| `integrations/skills/proofback-review/SKILL.md` | Any agent that reads Agent Skills |
| `integrations/claude-code/` | Claude Code plugin (skill and MCP server) |
| `integrations/cursor/proofback.mdc` | Cursor project rule |
| `integrations/AGENTS.snippet.md` | Paste into your project's `AGENTS.md` |

## Optional built-in suggestions

See `proofback suggest --help`. Off by default; see the plan, section 8.
