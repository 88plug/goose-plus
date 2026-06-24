# AGENTS Instructions

goose is an AI agent framework in Rust with CLI and Electron desktop interfaces.

## Setup
```bash
source bin/activate-hermit
cargo build
```

## Commands

### Build
```bash
cargo build                   # debug
cargo build --release         # release  
just release-binary           # release + openapi
```

### Test
```bash
cargo test                   # all tests
cargo test -p goose          # specific crate
cargo test --package goose --test mcp_integration_test
just record-mcp-tests        # record MCP
```

### Lint/Format
```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
```

### UI
```bash
just generate-openapi        # after server changes
just run-ui                  # start desktop
cd ui/desktop && pnpm test   # test UI
```

## Structure
```
crates/
├── goose              # core logic
├── goose-acp-macros   # ACP proc macros
├── goose-cli          # CLI entry
├── goose-server       # backend (binary: goosed)
├── goose-mcp          # MCP extensions
├── goose-test         # test utilities
└── goose-test-support # test helpers

evals/open-model-gym/  # benchmarking / evals
ui/desktop/            # Electron app
```

## Development Loop
```bash
# 1. source bin/activate-hermit
# 2. Make changes
# 3. cargo fmt
```

### Run these only if the user has asked you to build/test your changes:
```
# 1. cargo build
# 2. cargo test -p <crate>
# 3. cargo clippy --all-targets -- -D warnings
# 4. [if server] just generate-openapi
```

## Rules

- Test: Prefer tests/ folder, e.g. crates/goose/tests/
- Test: When adding features, update goose-self-test.yaml, rebuild, then run `goose run --recipe goose-self-test.yaml` to validate
- Error: Use anyhow::Result
- Provider: Implement Provider trait see providers/base.rs
- MCP: Extensions in crates/goose-mcp/
- Server: Changes need just generate-openapi

## Code Quality

- Comments: Write self-documenting code - prefer clear names over comments
- Comments: Never add comments that restate what code does
- Comments: Only comment for complex algorithms, non-obvious business logic, or "why" not "what"
- Simplicity: Don't make things optional that don't need to be - the compiler will enforce
- Simplicity: Booleans should default to false, not be optional
- Errors: Don't add error context that doesn't add useful information (e.g., `.context("Failed to X")` when error already says it failed)
- Simplicity: Avoid overly defensive code - trust Rust's type system
- Logging: Clean up existing logs, don't add more unless for errors or security events

## Ink / Terminal UI (ui/text)

- Ink renders React to a fixed character grid — not a browser. Content that exceeds a Box's dimensions is NOT clipped; it visually overflows into neighboring cells and breaks the layout.

- Ink-Text: Never use `wrap="wrap"` inside a fixed-height Box — wrapped text can exceed the Box height and bleed into adjacent components. Use `wrap="truncate"` and pre-truncate the string to fit the available character budget (lines × width).
  
- Ink-Layout: When changing card/cell dimensions, always recalculate how much content fits. Account for borders (2 chars), padding, margins, and sibling elements when computing the
remaining space for dynamic text.
  
- Ink-Overflow: Ink has no `overflow: hidden`. The only way to prevent overflow is to ensure content never exceeds the container size — truncate text, limit list items, or cap height.
  
- Ink-FlexGrow: Avoid `flexGrow={1}` on text containers inside fixed-height cards — the text will try to fill available space but Ink won't clip it if it exceeds the boundary.
  
- Ink-HeightBudget: When computing how many rows/items fit vertically, count EVERY line used by headers, footers, margins, borders, and scroll indicators. Under-reserving vertical space (e.g., `height - 8` when chrome actually uses 16 lines) causes Ink to squeeze out margins between items, making borders collapse. Always audit the actual line count.
  
- Ink-TrailingMargin: Don't apply `marginBottom` to the last item in a list — it wastes a line and can push content out of the container. Use conditional margins or container `gap`.

## Never

- Never: Edit ui/desktop/openapi.json manually
- Cargo.toml: For human-authored dependency changes, use `cargo add` instead of manually editing dependency entries unless there is a specific reason not to.
- Cargo.toml: Automated dependency bump PRs are exempt; when manual edits are necessary, keep `Cargo.lock` consistent.
- Never: Skip cargo fmt
- Never: Merge without running clippy
- Never: Comment self-evident operations (`// Initialize`, `// Return result`), getters/setters, constructors, or standard Rust idioms

## Entry Points
- CLI: crates/goose-cli/src/main.rs
- Server: crates/goose-server/src/main.rs
- UI: ui/desktop/src/main.ts
- Agent: crates/goose/src/agents/agent.rs

## goose-plus maintenance (88plug fork — we are the maintainer)

- Cross-platform: `#[cfg(windows)]` / macOS-only code is NEVER compiled in
  Linux dev. The fix for a platform-conditional unused item is to cfg-gate it
  properly (gate the `use`, or call the macro fully-qualified at its one site) —
  NOT to relax the lint. We keep `[workspace.lints.rust] unused_imports /
  unused_must_use = "deny"` AND enforce `-D warnings` on every OS preflight
  (host/Windows/macOS). A warning IS a release blocker; the answer is always to
  fix the code, never to allow the warning.
- Preflight feature surface: the preflights must compile the SAME feature-gated
  modules the real build does, or a platform-only error in a `#[cfg(feature =
  "...")]` module (e.g. `commands/tui.rs`, `commands/update.rs`) sails through
  the fast gate and fails ~25 min into the matrix. So the CLI preflight enables
  the full `default` set MINUS `local-inference` (the only feature that pulls
  llama-cpp's C++): `cargo check -p goose-cli --no-default-features --features
  code-mode,tui,update,aws-providers,telemetry,nostr,otel,system-keyring,rustls-tls
  --all-targets`, with `goose-server` checked separately. `--no-default-features`
  ALONE is a blind spot — it skips every default-but-gated module.
- flatpak is best-effort in the Linux desktop bundle: its maker needs the
  freedesktop runtime + Electron BaseApp published for `runtimeVersion`
  (external availability flake). deb/rpm build first and are the guaranteed
  installers — `bundle-desktop-linux.yml` hard-fails only if deb or rpm is
  missing and uploads flatpak with `if-no-files-found: warn`. Never block the
  whole multi-platform release on flatpak.
- winapi: each `winapi::um::*` / `shared::*` module needs its matching feature
  in the `winapi` workspace dep features list, or it's an unresolved-import
  error on Windows only.
- Release: tag `plus-v*` to trigger `.github/workflows/release-plus.yml` (CLI +
  unsigned desktop bundles + keyless build-provenance attestation, published to
  88plug/goose-plus). Distinct from upstream `release.yml` (`v1.*`, signed). OS
  signing is opt-in via repo var `DESKTOP_SIGNING=true` (+ signing secrets).
- a2a-rs: goose pins the 88plug/a2a-rs fork by git rev; fix the fork first, push
  main+88plug-plus, then bump the pinned rev in the workspace `Cargo.toml`.
