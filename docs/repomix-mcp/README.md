# repomix-mcp (goose-plus)

Repomix packs a local directory or a remote GitHub repository into a single consolidated
file for AI analysis — directory tree, file contents, and metrics in one document, with
optional Tree-sitter compression and multiple output formats (XML/Markdown/JSON/plain).

Embedded natively the same way `searxng` is: a built-in extension the agent can load
in-process (`crates/goose-mcp/src/repomix/`, registered in `BUILTIN_EXTENSIONS`), not just
documented as an external MCP server the user has to configure by hand.

## Why native instead of reimplemented

Repomix already ships its own MCP server backed by a real packing engine (fast-glob
matching, Tree-sitter compression across a dozen+ languages, git cloning). Reimplementing
that from scratch in Rust isn't worth it. So this extension is a thin split:

- **Pure Rust, no subprocess** — `attach_packed_output`, `read_repomix_output`,
  `grep_repomix_output`, `file_system_read_directory`, `file_system_read_directory_with_sizes`,
  `file_system_read_file`. These are just filesystem/regex operations; doing them natively
  is simpler and faster than shelling out.
- **Shells out to the real `repomix` CLI** — `pack_codebase`, `pack_remote_repository`,
  `generate_skill`. These depend on repomix's packing engine and aren't worth reimplementing.

## Install / auto-detection

Mirrors the `computercontroller` → peekaboo pattern exactly (`crates/goose-mcp/src/peekaboo/`):
a cached existence check, an auto-install attempt, then a friendly, actionable error if that
fails — never a raw "command not found."

1. Check `which repomix`.
2. If missing and `npm` is available, attempt `npm install -g repomix` (the vanilla published
   package — the only thing programmatically auto-installable).
3. If that fails too, return an error naming both the manual npm install command and how to
   build/link the community fork instead.

Nothing is vendored or bundled into goose-plus's own build/release pipeline — `repomix` is an
external runtime dependency resolved on `PATH`, exactly like peekaboo is for `computercontroller`.

**Recommended: use the `repomix-plus` fork** for the best experience (snake_case param
robustness, MCP server hardening, a Tree-sitter Go-brace-preservation fix, on top of vanilla
repomix). Build and link it instead of the npm-published version:

```bash
git clone https://github.com/yamadashy/repomix
cd repomix
git checkout claude/repomix-plus   # or your own fork/branch of the improved version
npm run build
npm link
```

## Tools exposed

| Tool | Implementation | Notes |
|---|---|---|
| `pack_codebase` | shells out to `repomix` | local directory → consolidated output |
| `pack_remote_repository` | shells out to `repomix` | clones + packs a remote GitHub repo |
| `attach_packed_output` | pure Rust | registers an existing output file for `read`/`grep` |
| `read_repomix_output` | pure Rust | reads a packed output by id, optional line range |
| `grep_repomix_output` | pure Rust (`regex` crate) | regex search with context lines |
| `file_system_read_directory` | pure Rust | lists a directory's immediate contents |
| `file_system_read_directory_with_sizes` | pure Rust | same, with sizes, sortable |
| `file_system_read_file` | pure Rust | reads a file; refuses if it looks like it contains a secret |
| `generate_skill` | shells out to `repomix` | generates a Claude Agent Skill from a directory |

The `outputId` registry is capped at 100 entries with oldest-evicted, mirroring upstream
repomix's own in-process `outputFileRegistry` (`src/mcp/tools/mcpToolRuntime.ts`) so behavior
matches exactly.

## Security

`file_system_read_file` refuses to serve a file's content if it detects a high-entropy token
that looks like a secret (API key, token) — a small self-contained port of the Shannon-entropy
heuristic already used for session-diagnostics redaction (`crates/goose/src/session/diagnostics.rs`),
matching upstream repomix's own "security scanning is always on" behavior for that tool.
