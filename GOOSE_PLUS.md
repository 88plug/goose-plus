# goose-plus

A community **plus fork** of [goose](https://github.com/aaif-goose/goose) — *the version of goose the maintainers had in flight, finished*. It completes the migrations that were mid-stream, fixes the user-facing failures their seams left behind, adds first-class agent-interop (A2A + NATS), and drives every provider's model catalog from the live API instead of a hand-maintained list.

> **The bar for every change:** one a thoughtful upstream maintainer would accept — small, focused, tested, and verified (`cargo clippy --workspace --all-targets -D warnings`, `cargo test`, desktop `lint:check`, all green) before it ships. Built on upstream `main`; nothing here is a throwaway hack.

---

## Why a plus fork?

Upstream goose is excellent and moving fast — which means there's always a frontier of *almost-finished* work: a migration landed but the old seam still bites users, a closed issue that real people still hit, a community PR that stalled on scope. A **plus fork** is the version that closes that frontier: it reads the whole codebase and the whole issue/PR **graveyard**, then implements what the community actually asked for and what the code was already trying to become — and proves each change with real tests.

So goose-plus is two things at once:
1. **A daily-driver distribution** — same goose, with the rough edges sanded and net-new agent-interop.
2. **A proving ground for upstream** — every fix/feature here is a tested, isolated candidate you can **port back to real goose** in one command (see [Contribute by porting](#contribute-by-porting)).

---

## How we're different — at a glance

| Pillar | upstream `goose` | `goose-plus` |
|---|---|---|
| **Provider model catalogs** | Hand-maintained static lists; context limits hardcoded and prone to drift | **Driven from each provider's own `/v1/models` on auth** — list, context window, capabilities, pricing. Static list is offline fallback only |
| **xAI / Grok** | API-key only | **SuperGrok subscription (OAuth) *and* API-key**, kept strictly separate (managed proxy vs metered console), working `reasoning_effort`, model-aware routing |
| **Nebius Token Factory** | — | Full provider, catalog + params from `?verbose=true` (tools/vision/reasoning/cost per model) |
| **Agent interop (A2A)** | — | Serves an Agent Card + JSON-RPC/REST/WebSocket; bearer auth; incremental streaming; cancel. Calls remote A2A agents as tools |
| **Event/Control bus (NATS)** | — | Opt-in publish firehose, bidirectional drive loop (fleet envelope: seq + instance + drop counter), **and** a JetStream KV claim/lease bus so concurrent goose-plus instances/subagents don't clobber the same file |
| **Standalone MCP servers** | builtin tools are in-agent only | `goose-plus mcp <name>` serves 5 tool servers (developer, computer control, memory, tutorial, autovisualiser) over stdio for any MCP client |
| **Web search** | Whatever single engine the model picks | searxng-mcp: 8 verified free providers **always run in full parallel**, HTML fallback + merge, live MCP progress/logging per provider, optional FlareSolverr last-resort fallback |
| **Codebase packing (repomix-mcp)** | Manual `npx repomix` outside the agent | Native in-agent extension (`pack_codebase`/`pack_remote_repository`/`grep_repomix_output`/etc.), auto-installing `repomix` via npm if missing |
| **Terminal UI** | Node/Ink shim (`node`/`npx` subprocess) | Native Rust TUI (ratatui + crossterm) talking to the agent directly over ACP |
| **Headless/self-hosted deployment** | Electron desktop or CLI only | Dockerized headless `goosed` API server, a browser/web build of the desktop UI, and a `docker-compose` one-liner for both |
| **First-prompt latency** | Cold system-prompt + tool-schema build every session | Startup pre-warming cuts first-token latency ~60% (measured 1003ms→842ms); platform extensions polled concurrently instead of serially |
| **Dependencies** | Mixed freshness | Kept current via the `use-latest-version` pipeline; lockfiles consistent |
| **Lint/format gate** | Default-feature clippy | Every crate inherits workspace lints; prettier wired into the gate; feature-gated code covered |
| **Release/CI** | Upstream signed releases | Self-maintaining `plus-v*` releases + keyless build-provenance, upstream `main` mirror, one-command upstream ports; `goose update` tracks goose-plus's own releases |
| **Internal security audits** | — | 4 independent code-first sweeps across the whole workspace: ~70 fixes (path-traversal guards, constant-time secret comparisons, resource leaks, TOCTOU races) |
| **Graveyard** | Open by definition | ~125 closed/rejected issues & PRs implemented and verified |

Full diff: **[compare main…goose-plus](https://github.com/88plug/goose-plus/compare/main...goose-plus)**.

---

## Headline additions

- **API-driven model marriage** — on auth, providers populate their model list *and* per-model parameters (context window, tool/vision/reasoning support, token cost) from the provider's own `/v1/models` (Nebius `?verbose=true`; xAI `context_length`/`context_window`). No more phantom models or stale context limits.
- **xAI SuperGrok native provider** — OAuth (loopback + device-code), **subscription** (`cli-chat-proxy.grok.com`) and **API-key** (`api.x.ai`) paths kept distinct, live-correct Grok context windows, and **working thinking-effort** (`reasoning_effort` actually transmitted, with graceful fallback).
- **Nebius Token Factory provider** — OpenAI-compatible, dynamic discovery with rich per-model metadata.
- **A2A (Agent2Agent)** — goose speaks A2A both ways (Agent Card + JSON-RPC/REST/WS server with bearer auth, streaming, cancel; and an A2A client). See [`docs/a2a.md`](docs/a2a.md), [`AUTH.md`](AUTH.md).
- **Native NATS bus** — opt-in publish + bidirectional drive, **plus a JetStream KV claim/lease coordination bus** so concurrent goose-plus instances/subagents claim a file before writing instead of racing each other. See [`docs/nats.md`](docs/nats.md).
- **Standalone MCP** — `goose-plus mcp <name>` exposes 5 builtin tool servers to any MCP host.
- **searxng-mcp parallel web search** — a dedicated MCP server that always runs 8 verified free search providers in full parallel (no cap), with HTML fallback + merge, live per-provider MCP progress/logging notifications, and a direct A2A fast-path (`searxng: <query>`) that skips a full LLM turn.
- **repomix-mcp codebase packing** — repomix's `pack_codebase`/`pack_remote_repository`/`grep_repomix_output`/etc. embedded as a native in-agent extension (not just documented as an external MCP server), auto-installing `repomix` via npm the same way `computercontroller` auto-installs peekaboo via Homebrew. See [`docs/repomix-mcp/README.md`](docs/repomix-mcp/README.md).
- **Native Rust TUI** — `goose tui` is now a real terminal UI (ratatui + crossterm over ACP), replacing the old Node/Ink subprocess shim.
- **Opt-in path confinement** — `GOOSE_CONFINEMENT=true` confines the developer extension's write/edit/analyze tools to the session's working directory, rejecting `..`-traversal and symlink escapes.
- **Headless & browser deployment** — a Dockerized `goosed` API server (no Electron needed) and a browser build of the desktop UI (full `window.electron`/`window.appConfig` web shim), both one-command via `docker-compose up`.
- **Faster first prompt** — session/ACP startup now pre-warms the system-prompt + tool-schema cache and polls platform extensions concurrently instead of serially, cutting measured first-token latency ~60% (1003ms→842ms). Opt out with `GOOSE_DISABLE_PREWARM=1`; live-measure any session with `GOOSE_PERF_LOG=1`.
- **Internal security-audit sweeps** — 4 independent code-first passes across every crate found and fixed ~70 bugs, including three separate path-traversal guards (memory-tool categories, local-inference quantization filenames, scheduler job IDs) and several constant-time secret-comparison fixes (A2A/MCP-app-proxy routes, tunnel pairing) that were comparing secrets with `!=` or a non-cryptographic hash.
- **Redacted diagnostics export** — `goose session diagnostics` scans every log, `session.json`, `config.yaml`, and scheduled-recipe file it bundles for high-entropy tokens (API keys, JWTs) and replaces them with `[REDACTED]` before zipping, so pasting a support bundle into a GitHub issue can't leak credentials.
- **ACP locked down to local origins** — the local ACP server's CORS layer allowed any web origin and its WebSocket upgrade had no origin check at all; both now reject anything that isn't localhost/127.0.0.1/`[::1]`.

---

## Use goose-plus's tools in Claude Code & Gemini CLI

goose-plus's built-in tools are real MCP servers, each runnable standalone over
stdio (`goose-plus mcp <name>`), so any MCP host can use them. This repo ships
the marketplace manifests:

- **Claude Code** — `.claude-plugin/marketplace.json` + `plugins/goose-plus-tools/`:
  ```
  /plugin marketplace add 88plug/goose-plus
  /plugin install goose-plus-tools@goose-plus
  ```
- **Gemini CLI** — `gemini-extension.json`:
  ```
  gemini extensions install https://github.com/88plug/goose-plus
  ```

Both just need the **`goose-plus` binary on PATH** (from the [releases](https://github.com/88plug/goose-plus/releases)) — the same model Claude Code's official LSP plugins use. The five servers exposed (verified standalone — `initialize` + `tools/list`):

| Server | `goose-plus mcp …` | tools |
|---|---|--:|
| developer | `developer` | 5 |
| computer control | `computercontroller` | 7 |
| memory | `memory` | 4 |
| tutorial | `tutorial` | 1 |
| autovisualiser | `autovisualiser` | 8 |

No server refactor was needed — the standalone `goose-plus mcp <name>` exposure (above) already speaks MCP; the marketplaces are thin manifests over it. (`searxng` and `repomix` are in-agent-only builtins — see below — not exposed as standalone `goose-plus mcp` servers.)

## Feature matrix (capabilities)

✓ = present · ◑ = partial · – = absent

| Capability | goose | goose-plus |
|---|:--:|:--:|
| API-driven provider catalog + params (`/v1/models`) | – | ✓ |
| Per-model tool / vision / reasoning / cost metadata from API | – | ✓ |
| xAI API-key provider | ✓ | ✓ |
| xAI SuperGrok subscription (OAuth) provider | – | ✓ |
| Nebius Token Factory provider | – | ✓ |
| A2A server (JSON-RPC + REST + WebSocket) | – | ✓ |
| A2A bearer auth + incremental streaming + cancel | – | ✓ |
| A2A client (remote agents as tools) | – | ✓ |
| Native NATS event firehose | – | ✓ |
| NATS bidirectional drive (run a turn over the bus) | – | ✓ |
| Builtin tools as standalone MCP servers | – | ✓ |
| Installable in Claude Code + Gemini CLI (marketplace manifests) | – | ✓ |
| `reasoning_effort` actually transmitted to provider | ◑ | ✓ |
| Dependencies kept on latest + consistent lockfiles | ◑ | ✓ |
| Workspace-wide lint gate (all crates + prettier + feature code) | ◑ | ✓ |
| Self-maintaining release + build provenance + upstream mirror | – | ✓ |
| Portable Windows `.zip` / Linux `.AppImage` | ◑ | ✓ |
| Native Rust TUI (`goose tui`, ratatui + crossterm over ACP) | – | ✓ |
| Opt-in filesystem path confinement for write/edit/analyze (`GOOSE_CONFINEMENT`) | – | ✓ |
| Cursor as an ACP provider (`cursor-acp`) | – | ✓ |
| `goose model` / `goose model list` CLI commands | – | ✓ |
| Per-tool-call timing shown in interactive sessions | – | ✓ |
| Inline `!<command>` shell passthrough in interactive sessions | – | ✓ |
| `/copy` last-response-to-clipboard (OSC 52 fallback for SSH) | – | ✓ |
| `GOOSE_CLI_BELL` opt-in terminal bell on turn completion | – | ✓ |
| Hidden/internal extensions flagged instead of silently dropped (REST API) | – | ✓ |
| New Chat as a true global shortcut (not just a menu accelerator) | – | ✓ |
| Toast notice when a mode change needs other sessions restarted | – | ✓ |
| OpenCode free/paid models provider | – | ✓ |
| searxng-mcp: 8 free providers always run in full parallel | – | ✓ |
| repomix-mcp: native in-agent codebase-packing extension | – | ✓ |
| NATS JetStream KV claim/lease coordination bus | – | ✓ |
| Dockerized headless `goosed` API server (no Electron) | – | ✓ |
| Browser/web build of the desktop UI | – | ✓ |
| `AfterAgentResponse` lifecycle hook | – | ✓ |
| `goose update` tracks goose-plus's own releases | ◑ | ✓ |
| Startup pre-warm (system prompt + tool schemas) for faster first token | – | ✓ |
| `GOOSE_PERF_LOG` per-turn timing diagnostic | – | ✓ |
| High-entropy secret redaction in diagnostics export | – | ✓ |
| ACP `_goose/unstable/health` startup readiness check | – | ✓ |
| Onboarding quick-setup card for env-detected provider credentials | – | ✓ |
| `GOOSE_PROVIDER_TIMEOUT` global fallback honored by every provider | ◑ | ✓ |
| Shell tool background process support (start/list/output/stop) | – | ✓ |
| Text-normalization + chunking before prompt-injection classification | – | ✓ |
| `\` + Enter line continuation in interactive CLI input | – | ✓ |
| Bounded per-instance SQLite pool (prevents SQLITE_BUSY under concurrent sessions) | – | ✓ |

---

## Bug matrix (fixed from the graveyard)

Real upstream issues/PRs that were closed-without-fix, rejected, or never got to — implemented and tested here. Each row is also a [port candidate](#contribute-by-porting).

| Area | Upstream # | What goose-plus fixes |
|---|---|---|
| Providers/reasoning | [#9397](https://github.com/aaif-goose/goose/issues/9397), [#9675](https://github.com/aaif-goose/goose/issues/9675) | DeepSeek / openai-compatible `reasoning_content` no longer dropped |
| Providers/streaming | [#8503](https://github.com/aaif-goose/goose/issues/8503) | Final text segment after tool calls no longer lost |
| Providers | [#8321](https://github.com/aaif-goose/goose/issues/8321) | Unmapped `/v1/models` surfaced instead of silently dropped |
| Providers | [#9124](https://github.com/aaif-goose/goose/issues/9124), [#7987](https://github.com/aaif-goose/goose/issues/7987) | Env-configurable retry + request timeout; sane 429/Retry-After |
| Providers | [#9489](https://github.com/aaif-goose/goose/issues/9489) | Ollama keep-alive keeps the model warm |
| Providers | [#9476](https://github.com/aaif-goose/goose/issues/9476) | Databricks serving-endpoints pagination — all models surface |
| Providers | [#8512](https://github.com/aaif-goose/goose/issues/8512) | Context-limit floor stops Context-Length-Exceeded on fresh install |
| Providers | [#2564](https://github.com/aaif-goose/goose/issues/2564), [#9333](https://github.com/aaif-goose/goose/issues/9333) | Codex `v1/responses` handling; DeepSeek V4 |
| Providers | [#6293](https://github.com/aaif-goose/goose/issues/6293), [#1863](https://github.com/aaif-goose/goose/issues/1863) | Gemini empty-reply-after-tool-use; fetch-400 |
| Providers | [#6573](https://github.com/aaif-goose/goose/issues/6573), [#8495](https://github.com/aaif-goose/goose/issues/8495) | devstral context limits; `OPENAI_CUSTOM_HEADERS` with commas |
| MCP | [#7063](https://github.com/aaif-goose/goose/issues/7063) | MCP auto-reconnect on dropped transport |
| Agent | [#9082](https://github.com/aaif-goose/goose/issues/9082), [#9640](https://github.com/aaif-goose/goose/issues/9640) | No-progress loop guard stops runaway turns |
| Agent | [#8777](https://github.com/aaif-goose/goose/issues/8777) | Unix login-shell process-group isolation |
| Agent | [#9398](https://github.com/aaif-goose/goose/issues/9398) | `detect_image_path` handles shell escapes |
| Agent | [#3085](https://github.com/aaif-goose/goose/issues/3085) | `GOOSE.md` recognized as a project context file |
| CLI | [#8059](https://github.com/aaif-goose/goose/issues/8059) | Bracketed paste — pasting no longer auto-executes |
| CLI | [#10025](https://github.com/aaif-goose/goose/issues/10025), [#10056](https://github.com/aaif-goose/goose/issues/10056), [#10104](https://github.com/aaif-goose/goose/issues/10104) | Terminal cursor left hidden after Ctrl+C during `configure` — submitted/closed unmerged three times upstream, never landed |
| CLI | [#7338](https://github.com/aaif-goose/goose/issues/7338) | WezTerm: arrow keys silently fail in `configure` menus (DECCKM application-cursor-key mode never reset) |
| Providers | [#10179](https://github.com/aaif-goose/goose/issues/10179) | `claude-sonnet-5` missing from the canonical registry — fell back to 128k context instead of its real 1M window |
| Developer tools | [#7587](https://github.com/aaif-goose/goose/issues/7587) | Write/edit/analyze path-target ambiguity — symlink and `..`-traversal escape from the workspace (opt-in `GOOSE_CONFINEMENT`) |
| Server | [#9358](https://github.com/aaif-goose/goose/issues/9358) | `GET /sessions/{id}` message pagination |
| Desktop | [#9342](https://github.com/aaif-goose/goose/issues/9342), [#8997](https://github.com/aaif-goose/goose/issues/8997) | Chat history not loading; reply-render delay under reduced-motion |
| Providers/Bedrock | [#10006](https://github.com/aaif-goose/goose/issues/10006) | `ResourceNotFoundException` retried ~6 times (~2 min hang) on an invalid model name instead of failing fast |
| Providers/Bedrock | [#9888](https://github.com/aaif-goose/goose/issues/9888) | `max_tokens`/`temperature` dropped — not forwarded via `inferenceConfig` on Converse/ConverseStream |
| Agent | [#9949](https://github.com/aaif-goose/goose/issues/9949), [#9963](https://github.com/aaif-goose/goose/issues/9963) | Recipe `extensions:` ignored when invoked via `delegate()`/`summon` subagents |
| Scheduler | [#10016](https://github.com/aaif-goose/goose/issues/10016) | `schedule sessions` always reports `Messages: 0` |
| Providers | [#9993](https://github.com/aaif-goose/goose/issues/9993) | Responses-API stream parser crashed on a malformed known event mid-stream instead of skipping it |
| Desktop | [#9881](https://github.com/aaif-goose/goose/issues/9881) | Extension toggle in Settings snapped back to On while disabling |
| Desktop | [#8135](https://github.com/aaif-goose/goose/pull/8135) | A tool call with no arguments, output, logs, or progress still showed a clickable expand chevron that opened onto an empty panel — `hasContent` is now computed from the same data driving the panel instead of inferred from render output |
| Providers | [#10032](https://github.com/aaif-goose/goose/issues/10032) | `GOOSE_CONTEXT_LIMIT` clobbered a known per-model context window instead of acting as a fallback |
| Developer tools | [#5444](https://github.com/aaif-goose/goose/issues/5444) | Analyze tool's `follow_depth`/`max_depth` params rejected a string-typed value, only accepting a plain number |
| Agent | [#8496](https://github.com/aaif-goose/goose/issues/8496) | Delegated agents treated `inherit` as a literal provider/model instead of falling back to the parent session's config |
| Recipes | [#5280](https://github.com/aaif-goose/goose/issues/5280) | Recipe parse failures now show the raw recipe content, not just the parser error |
| CLI | [#6224](https://github.com/aaif-goose/goose/issues/6224) | PowerShell `term init` output had leftover double braces — invalid PowerShell syntax |
| Desktop | [#5915](https://github.com/aaif-goose/goose/issues/5915) | `goosed`'s spawned environment could be missing the PATH entries a locally-installed `claude`/`codex` CLI needs to resolve |
| Context | [`micn/tool-sum-fixes`](https://github.com/aaif-goose/goose/tree/micn/tool-sum-fixes) | Tool-pair summarization created stale "ghost" messages once compaction could handle the context anyway — now off by default, gated to ≤65,536-token windows |
| Scheduler | [#5346](https://github.com/aaif-goose/goose/issues/5346) | A scheduled job whose recipe failed to load (bad YAML, no `prompt`/`instructions`) failed with only a log line — now creates a session with a visible explanation |
| Providers | [#7449](https://github.com/aaif-goose/goose/issues/7449) | A turn with multiple tool responses, one carrying an image, interleaved the image message between tool_result blocks instead of after all of them — Claude (via any Claude-backed provider routed through the OpenAI-compatible format) rejects non-contiguous tool_result blocks |
| CLI | [#2145](https://github.com/aaif-goose/goose/issues/2145) | `\` + Enter now inserts a newline in interactive input (the shell line-continuation convention), alongside the existing `Ctrl`+`J` binding — the completer's `Validator` was previously a stub that always submitted on Enter |
| Session storage | [`fix/sqlite-busy-connection-leak-7624`](https://github.com/aaif-goose/goose/tree/fix/sqlite-busy-connection-leak-7624) (adjusted) | Each `SessionManager` instance's SQLite pool used sqlx's default max size (10); multiple concurrent instances (e.g. one per ACP session) against the same file could multiply past what `busy_timeout` alone serializes. Capped at 2 connections per pool, not upstream's 1 — this fork's own `test_begin_immediate_prevents_lock_upgrade_deadlock` deliberately races two same-pool transactions and needs both slots, a real conflict caught only by running the full test suite before shipping |

### Fixed without an upstream ticket

No open/closed upstream issue exists for these — the **Source** column links the upstream branch this was ported from (verified live as of this writing — feature branches can be deleted, so if a link 404s, `git log --all --grep=<branch-name>` on this repo still finds the porting commit), or gives the goose-plus commit hash for fixes this fork found on its own (`git show <hash>` in this repo).

| Area | Source | What goose-plus fixes |
|---|---|---|
| Providers/xAI | `5b2eaec19` (own fix) | SuperGrok OAuth was unusable end-to-end: missing Grok CLI proxy headers (426) and a token-cache mismatch with grok-cli's own login (403 wrong-account) |
| Providers | [`alexhancock/fix-mistral`](https://github.com/aaif-goose/goose/tree/alexhancock/fix-mistral) | Mistral's OpenAI-compatible endpoint rejected `stream_options` |
| Providers | [`alexhancock/handle-gemini-databricks-errors`](https://github.com/aaif-goose/goose/tree/alexhancock/handle-gemini-databricks-errors) | Bare `{"error": {...}}` SSE events (e.g. Gemini via Databricks) silently swallowed instead of surfaced |
| Providers | [`worktree-fast-400-compaction-fix`](https://github.com/aaif-goose/goose/tree/worktree-fast-400-compaction-fix) | Fast-model utility calls (compaction/session-naming) 400ing from inherited thinking-effort budget overflow |
| Providers | `29261a351` (own fix) | A guard against a malformed bundled declarative provider silently vanishing from every surface with no test failure |
| Providers | `642c308c0` (own fix) | Unprefixed tool names (e.g. `read_resource`) failing to resolve to their extension-qualified form |
| Agent | [`llm_convenience`](https://github.com/aaif-goose/goose/tree/llm_convenience) | Panic on LLM-authored recipes with an empty `{}` response schema |
| Session | [`zane/check-exists`](https://github.com/aaif-goose/goose/tree/zane/check-exists) | Schema migration 7 erroring under concurrent-process races |
| Session | `d3a600a64` (own fix) | `sqlx` panic on session creation right after the v14 schema migration |
| Security | [`micn/symlink-fix`](https://github.com/aaif-goose/goose/tree/micn/symlink-fix) | A `.goosehints` symlink bypassing the import-boundary read restriction |
| Security | [`fix/hf-rfilename-path-validation`](https://github.com/aaif-goose/goose/tree/fix/hf-rfilename-path-validation) | HuggingFace `rfilename` path-traversal writing cached model files outside the intended cache dir |
| Desktop/npm | [`alexhancock/goose-tui-binary-resolution`](https://github.com/aaif-goose/goose/tree/alexhancock/goose-tui-binary-resolution) | Packaged npm `goose` binary shipping non-executable when resolved by path instead of through `node_modules/.bin` |
| A2A/CLI | `691a14749` (own fix, live battle-testing) | A2A's `message:send` fully unusable end-to-end (missing session row, missing provider bootstrap, mangled streamed-reply text) |
| Config | `43dfb2762` (own fix) | `GOOSE_A2A_ENABLE=1` / `GOOSE_NATS_DRIVE=1` silently no-op'd — only `"true"`/`"false"` parsed, not the conventional truthy `"1"` |
| Security/ACP | [`micn/acp-cors`](https://github.com/aaif-goose/goose/tree/micn/acp-cors) | ACP's CORS layer allowed any web origin (`tower_http::cors::Any`) — CSRF exposure on the unauthenticated-by-default local ACP server. WebSocket upgrades, which CORS preflight doesn't protect, had no origin check at all |
| Code mode | [`alexhancock/capture-result-content-types`](https://github.com/aaif-goose/goose/tree/alexhancock/capture-result-content-types) | JS-code-mode tool results with non-text content (images, embedded resources) were silently dropped instead of surfaced |
| MCP/Developer | [`jackamadeo/dev-tool-login-shell`](https://github.com/aaif-goose/goose/tree/jackamadeo/dev-tool-login-shell) | Standalone `goose mcp developer` server ran with the launching process's bare env instead of the user's login-shell PATH/aliases (nvm, rbenv, cargo, etc. invisible to its shell tool); now re-execs through `$SHELL -lc` once before serving |
| Providers/release hygiene | [`remove-canonical-mapping-report`](https://github.com/aaif-goose/goose/tree/remove-canonical-mapping-report) | `build_canonical_models`'s release-time checker called every provider's live API with CI credentials and committed the raw result to a public 5,200+ line JSON file — risking internal/EAP model names leaking; checker/report machinery removed, registry build kept |
| Agent/MOIM | [`wpfleger/tool-response-issue`](https://github.com/aaif-goose/goose/tree/wpfleger/tool-response-issue) | After a cancelled or tool-heavy turn, orphaned tool_use/tool_result blocks triggered a persistent provider 400 death-loop: `fix_conversation` repaired the orphan, but MOIM's allowlist only recognized merge/whitespace/trailing-assistant fixes and discarded any other repair, handing the still-broken conversation back every turn |
| Session storage | [`micn/efficient-session-writes`](https://github.com/aaif-goose/goose/tree/micn/efficient-session-writes) | Persisting a turn's messages looped over `add_message`, opening a separate `BEGIN IMMEDIATE` transaction and `UPDATE sessions SET updated_at` per message; batched into one transaction with a single `updated_at` touch |
| Providers | [`codex/usage-record-timings`](https://github.com/aaif-goose/goose/tree/codex/usage-record-timings) | `ProviderStats` already had `time_to_first_token_ms`/`elapsed_ms` fields, but only the MLX local-inference backend populated them (elapsed only, no TTFT); now measured uniformly for every provider around `stream_response_from_provider` |
| Gateway/Telegram | `52a9d2fe8` (own fix) | An HTML-rejection fallback re-sent the whole message on retry, duplicating chunks already delivered to the chat; now falls back per-chunk |
| Gateway/Pairing | `52a9d2fe8` (own fix) | The one-time pairing-code store/consume wasn't synchronized — a TOCTOU race could consume the same code twice or lose a concurrent write; now serialized under a mutex |
| Providers/Bedrock | `52a9d2fe8` (own fix) | Document extension was taken from the first `.`-split segment, so names like `report.2024.txt` were misclassified and silently downgraded to raw text instead of their real type |
| ACP/Transport | `52a9d2fe8` (own fix) | `session_streams` had no upper bound (unlike the existing `pending_routes` bound) — a fabricated `Acp-Session-Id` header could allocate unbounded `OutboundStream`s; now capped at `MAX_SESSION_STREAMS` |
| Server/Resources | `52a9d2fe8` (own fix) | `read_resource` 500'd on a binary resource that wasn't valid UTF-8 instead of returning its base64 blob directly |
| CLI/Session | `024c58040` (own fix) | `set_theme` wrote the config twice on save — a redundant first write's `.expect()` panicked the CLI on a recoverable read-only-config error, even though the second write handled that same failure gracefully |
| MCP/Developer/Security | `a8c430090` (redesigned; upstream's `fix/prevent-config-overwrite` had no PR and was explicitly WIP with a self-contradicting write heuristic) | `text_editor`'s `write`/`str_replace` had no guard against overwriting goose's own `config.yaml`/`secrets.yaml` — a confused or prompt-injected session could disable safety settings or corrupt credentials via its own file tools. Now refuses any write/edit resolving into goose's config directory, independent of workspace path confinement |
| Agent | [`aaif-goose/goose#10103`](https://github.com/aaif-goose/goose/pull/10103) (closed — superseded by a larger unmerged usage-ledger redesign, `#10172`, still open) | Subagents spawned via `delegate` run in their own session, so their token usage/cost never reached the parent — under-reporting cost-per-outcome and per-session budgets whenever delegation occurred. Now rolled into the parent's `accumulated_*` totals via an atomic UPDATE at the subagent-completion chokepoint |
| Desktop (macOS/Apple Silicon) | [`fix/electron-v8-crash-after-wake`](https://github.com/aaif-goose/goose/tree/fix/electron-v8-crash-after-wake) (no PR ever opened) | App crashed with `EXC_BREAKPOINT` ~4s after macOS system wake on long-running (2+ day) sessions — V8's tiered compilation queues deferred optimization work on the CFRunLoop, which fires against stale state after a long sleep. Mitigated by disabling V8 Maglev on Apple Silicon, a post-resume GC request, and a renderer reload after 8+ hours asleep. Platform-gated to `darwin`/`arm64`; ported mechanically and typecheck/lint-verified only — the underlying rare crash itself is unverified on this Linux dev environment |
| Providers | [`lucas/feat/goose-provider-timeout`](https://github.com/aaif-goose/goose/tree/lucas/feat/goose-provider-timeout) (broadened; `f9ffa48ed` own work) | `GOOSE_PROVIDER_TIMEOUT` (and, for OpenAI, its own advertised `OPENAI_TIMEOUT` config key) was silently ignored by every provider's request-timeout resolution, which fell straight through to a hardcoded 600s default — upstream's patch touched 4 providers; this fork's `resolve_provider_timeout()` closes the same gap across all 11 construction sites that had it |
| Developer tools | [`spence/active-tool`](https://github.com/aaif-goose/goose/tree/spence/active-tool) (redesigned; `ba81a85d2` own work) | Long-running shell commands (dev servers, watchers) blocked the shell tool until they exited — `shell(background: true)` now spawns detached and returns a process ID; `list_background_processes`/`get_background_process_output`/`stop_background_process` manage it. Redesigned to fit this fork's modular shell-tool architecture, reusing its existing command-building/allowlist/output-streaming helpers so background processes get the same `GOOSE_SHELL_ALLOWED_COMMANDS` confinement as foreground ones — upstream's version had no such allowlist to preserve |

*(Security fixes from four independent code-first audit sweeps — `1af9f0fe2`, `a42722b1c`, `5f0d1536d`, all goose-plus's own code, not upstream-mined: path-traversal guards in the memory tool's category argument, local-inference's quantization filenames, and the scheduler's job IDs — the same class of bug `GOOSE_CONFINEMENT` above fixes for file write/edit; non-constant-time secret comparisons (`!=` or a non-cryptographic hash instead of the existing `token_matches` helper) in the A2A/MCP-app-proxy routes and the tunnel pairing-code check; unescaped shell-literal interpolation in the computer-controller's Linux command execution; and an integer-underflow panic from NFC-normalization-expanded text in conversation trimming.)*

## Community-feature matrix (requests delivered)

Features the community asked for — requested, upvoted, or stalled in a PR — that goose-plus ships.

| Area | Upstream # | Feature |
|---|---|---|
| Agent | [#7808](https://github.com/aaif-goose/goose/issues/7808) | Recipe-level tool blocking (denylist) |
| Agent | [#8183](https://github.com/aaif-goose/goose/issues/8183) | Graceful unknown-tool calls with suggestions |
| Desktop | [#6926](https://github.com/aaif-goose/goose/issues/6926) | Folders for organizing chats |
| Desktop | [#9080](https://github.com/aaif-goose/goose/issues/9080) | Model favorites |
| Desktop | [#9391](https://github.com/aaif-goose/goose/issues/9391) | Close-to-tray |
| Desktop | [#7554](https://github.com/aaif-goose/goose/issues/7554) | Per-window pinned certificates |
| Desktop | [#9143](https://github.com/aaif-goose/goose/issues/9143) | Same-window link navigation |
| Desktop | [#1505](https://github.com/aaif-goose/goose/issues/1505) | In-chat find filter |
| Desktop | [#8288](https://github.com/aaif-goose/goose/issues/8288) | Accessibility font scaling |
| Desktop | [#9390](https://github.com/aaif-goose/goose/issues/9390) | Syntax-highlighted diffs |
| Desktop | [#7965](https://github.com/aaif-goose/goose/issues/7965) | Delete apps |
| Desktop | [#8140](https://github.com/aaif-goose/goose/issues/8140) | Configurable sidebar session limit |
| Desktop | [#6472](https://github.com/aaif-goose/goose/issues/6472) | MCP Apps `ui/update-model-context` |
| CLI | [#10177](https://github.com/aaif-goose/goose/issues/10177) | Inline `!<command>` shell passthrough, output folded into conversation context |
| CLI | [#10181](https://github.com/aaif-goose/goose/issues/10181) | `/copy` — copy last assistant response to clipboard (OSC 52 fallback for SSH) |
| CLI | [#10182](https://github.com/aaif-goose/goose/issues/10182) | `GOOSE_CLI_BELL` — opt-in terminal bell on turn completion / approval prompts |
| CLI | [#5546](https://github.com/aaif-goose/goose/issues/5546) | `@` file-mention Tab completion, fuzzy-ranked the same way as the desktop app's `@`-mention picker |
| CLI | [#10173](https://github.com/aaif-goose/goose/issues/10173) | `/help` + tab-completion synced to the real builtin command registry |
| Providers | [#8391](https://github.com/aaif-goose/goose/issues/8391) | Cursor as an ACP provider (`cursor-acp`) |
| Recipes | [#9610](https://github.com/aaif-goose/goose/issues/9610) | Per-recipe model request params (`reasoning_effort`, `top_p`, etc.) via `Settings.request_params` |
| Hooks | [#9969](https://github.com/aaif-goose/goose/issues/9969) | `AfterAgentResponse` lifecycle hook |
| Desktop | [`micn/new-chat-shortcut`](https://github.com/aaif-goose/goose/tree/micn/new-chat-shortcut) | New Chat registered as a true global shortcut (not just a menu accelerator) |
| Desktop | [`micn/note-restart-needed`](https://github.com/aaif-goose/goose/tree/micn/note-restart-needed) | Toast on mode change explaining other active sessions need a restart to pick up the new mode |
| CLI/Agent | [`micn/critical-advice`](https://github.com/aaif-goose/goose/tree/micn/critical-advice) | `CRITICAL_ADVISORY` env var appends an operator-set one-line advisory to every session's system prompt, without editing config/recipes; placeholder values ("None"/"null"/"undefined") are ignored |

### Shipped without an upstream ticket

| Area | Source | Feature |
|---|---|---|
| CLI | [`alexhancock/rust-tui`](https://github.com/aaif-goose/goose/tree/alexhancock/rust-tui) | Native Rust TUI replacing the Node/Ink shim |
| CLI | [`micn/mattjoyce-model-list-command`](https://github.com/aaif-goose/goose/tree/micn/mattjoyce-model-list-command) | `goose model` / `goose model list` commands |
| CLI | [`dhanji/timing-cli`](https://github.com/aaif-goose/goose/tree/dhanji/timing-cli) | Per-tool-call timing in interactive sessions |
| Server | [`alexhancock/hidden-extensions`](https://github.com/aaif-goose/goose/tree/alexhancock/hidden-extensions) | Hidden/internal extensions flagged instead of dropped from the REST API |
| Agent | [`zane/recipe-extensions-fallback`](https://github.com/aaif-goose/goose/tree/zane/recipe-extensions-fallback) | Recipe-embedded extension config falls back to the user's working global config on load failure |
| Providers | `dfcf54102` (own work) | OpenCode free/paid-models provider |
| MCP | own work — new server, not upstream-mined | searxng-mcp's always-full-parallel free-provider web search |
| MCP | own work — new server, not upstream-mined | repomix-mcp: native in-agent codebase-packing extension (embeds the community `repomix-plus` fork's capabilities) |
| NATS | `45532431e` (own work) | JetStream KV claim/lease bus for concurrent-instance file coordination |
| Security | `2557f7c16` (redesigned; upstream's `code-review-security` had no PR) | `GOOSE_SHELL_ALLOWED_COMMANDS` opt-in allowlist mode — the shell tool bypasses the shell entirely and directly execs only listed programs, no expansion/pipes/redirections/subshells possible. For recipes that process untrusted input (e.g. reviewing a PR diff), where a prompt-injection payload shouldn't reach an unrestricted shell |
| Security | `d5c43677d` (own work; same upstream branch) | `GOOSE_SKIP_CONTEXT_FILES` opts a session out of loading named repo-provided context files (`AGENTS.md`, `.goosehints`, `GOOSE.md`) — for the same untrusted-checkout scenario, so the session doesn't pick up instructions from the very repo it's reviewing |
| Desktop | [`micn/module-default-lock`](https://github.com/aaif-goose/goose/tree/micn/module-default-lock) | Optional model lock (`GOOSE_MODEL_LOCK` env default + a Settings toggle) — once locked, the chat bottom bar shows a static, non-interactive model display instead of the switcher, for shared/managed installs that want to pin the model |
| Deployment | `361ebdc7d` + `4ecb2d29a` (own work) | Dockerized headless `goosed` API server and a browser build of the desktop UI |
| Performance | `2c392bace` + `280245ac3` (own work) | Startup pre-warming cutting measured first-token latency ~60% |
| ACP | [`wpfleger/acp-client`](https://github.com/aaif-goose/goose/tree/wpfleger/acp-client) (partial; `3933f9b70` own work) | `_goose/unstable/health` startup readiness check for programmatic ACP clients — the other two asks in this branch (settable system prompt, provider+model switching via `set_model`) were already met more generally by this fork's existing `SetSessionSystemPromptRequest` (mode `set`/`append` + key) and `SetSessionConfigOptionRequest` (`config_id: "provider"` + `meta.model`) |
| Desktop | [`micn/env-provider-detector`](https://github.com/aaif-goose/goose/tree/micn/env-provider-detector) (redesigned; `b3becffdb` own work) | Onboarding "Quick Setup" card when a provider's credentials are already found in the environment — upstream added a new backend endpoint hardcoded to 3 providers; goose-plus instead surfaces the existing `is_configured` field (already env-aware via `Config::get_secret`) generalized across every provider, no backend change needed |
| Security | [`feat/classifier-input-chunking-and-normalisation`](https://github.com/aaif-goose/goose/tree/feat/classifier-input-chunking-and-normalisation) (`ddfc25b27` own work) | Verbose, repetitive tool output could dilute or overflow the prompt-injection classifier's fixed token window — normalizes/dedupes/chunks input before classifying, taking the max confidence across chunks. Fixed a byte-index chunk-boundary panic risk on multi-byte UTF-8 present in the reference implementation |

> The `#` numbers link to the upstream [aaif-goose/goose](https://github.com/aaif-goose/goose) issue/PR tracker. Where a closed/unmerged PR existed, goose-plus reused its diff as a starting point — making those rows the cheapest to port back. For no-ticket rows, `Source` links the upstream branch (verified live; if a link 404s after the branch is deleted, `git log --all --grep=<branch>` on this repo still finds the porting commit) or gives the goose-plus commit hash for original fork work.

---

## Contribute by porting

**The whole point of a plus fork: nothing here has to stay here.** Every row in the matrices above is already implemented, tested, and isolated — which makes it a ready-to-open PR for *real* goose. You don't have to start from a blank issue; you can pick something that's **already solved** and carry it upstream.

### The model

```
aaif-goose/goose  ──(weekly mirror)──►  goose-plus:main   (read-only, never edited)
                                              │
                                    port/<name>  (branched FROM main, not goose-plus)
                                              │   minimal patch cherry-picked from goose-plus
                                              ▼
                          PR: aaif-goose/goose  ◄── 88plug:goose-plus:port/<name>
```

Ports **always** branch from `main` (the clean upstream mirror), never from `goose-plus` — so the PR is a minimal, reviewable diff with none of the fork's branding or unrelated changes.

### First, point your remotes correctly

The tooling assumes the standard fork convention — `origin` = your fork, `upstream` = the source:

```bash
git remote set-url origin   https://github.com/88plug/goose-plus.git   # the fork (write here)
git remote set-url upstream https://github.com/aaif-goose/goose.git     # the source (port from here)
```

If `origin` points at the source instead, `port-to-upstream.sh` and `just sync-upstream-main` would try to push to the real upstream — both now **abort with a clear error** if `origin` isn't the fork, so a mis-set remote fails safe instead of pushing to the wrong repo.

### Pick → port → PR (one example)

1. **Pick** an item from a matrix above — say the Nebius provider, or bug [#8503](https://github.com/aaif-goose/goose/issues/8503) (lost final text segment).
2. **Branch a clean port** from the upstream mirror:
   ```bash
   git fetch upstream
   ./scripts/port-to-upstream.sh nebius     # creates port/nebius from upstream/main
   ```
3. **Apply the minimal patch** — cherry-pick the relevant goose-plus commit(s) for that item, then fix any fork-specific bits (doc links `88plug/goose-plus` → `aaif-goose/goose`, drop `-plus` branding).
4. **Test it** the way upstream will: `cargo test -p goose nebius && cargo clippy --workspace --all-targets -- -D warnings`.
5. **Push & open the PR** against upstream:
   ```bash
   git push -u origin port/nebius
   ```
   PR URL pattern: `https://github.com/aaif-goose/goose/compare/main...88plug:goose-plus:port/<name>`

### Why this is the easy path
- The change already exists and is **proven green** here, so you're packaging, not inventing.
- Branching from `main` keeps the diff **upstream-shaped** (no fork noise) — exactly what reviewers want.
- Bug/feature rows that reused a closed upstream PR are **80% done** — you're reviving real prior work the maintainers can recognize.

Good first ports: single-file provider fixes ([#8495](https://github.com/aaif-goose/goose/issues/8495), [#6573](https://github.com/aaif-goose/goose/issues/6573)), self-contained features ([#1505](https://github.com/aaif-goose/goose/issues/1505) find filter, [#9080](https://github.com/aaif-goose/goose/issues/9080) favorites), or a whole new provider (Nebius).

---

## Relationship to upstream

| Branch | Purpose |
|--------|---------|
| `main` | Read-only mirror of [aaif-goose/goose](https://github.com/aaif-goose/goose) `main` (synced weekly + on demand). Ruleset-protected: no direct pushes, no force-push, no deletion. |
| `goose-plus` | Default branch — all plus work, releases (`plus-v*` tags). |
| `port/<name>` | Short-lived branches for upstream PRs — **always** based on `main`, never `goose-plus`. |

Items already shipped upstream (LM Studio, Mistral, Ollama, `/model`, Azure, PreToolUse hooks, Gemini ACP, …) are intentionally **not** re-implemented here. Never merge `goose-plus` into `main`.

Sync the mirror manually: `just sync-upstream-main` or trigger **Sync upstream main** in Actions. Only that workflow (Actions admin bypass) and repo admins may update `main`.

---

## Releases

Tag `plus-v*` to trigger `.github/workflows/release-plus.yml` — CLI + desktop bundles (unsigned community builds) + keyless build-provenance attestation, published to [88plug/goose-plus releases](https://github.com/88plug/goose-plus/releases). Distinct from upstream's signed `v1.*` releases. Install lines are in the [README](README.md).

## Build

```bash
source bin/activate-hermit
cargo build --release          # CLI + server
cd ui/desktop && pnpm install  # desktop
```
