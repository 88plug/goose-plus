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
| **Graveyard** | Open by definition | ~205-215 distinct improvements landed: 138 individually-cited closed/rejected issues & PRs & branches (mechanically counted via `grep -oE "issues/[0-9]+\|pull/[0-9]+\|tree/[A-Za-z0-9_./-]+\)" GOOSE_PLUS.md \| sort -u`, not hand-tallied — many rows cite both an issue and a PR number), ~70 fixes from the internal audit sweeps above, 15 headline own-work features |

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
| Disabling a builtin extension (Developer) actually prevents it loading at session start | – | ✓ |
| Configurable macOS seatbelt sandbox + egress proxy for `goosed` (opt-in) | – | ✓ |
| Developer `search` tool (Sourcegraph code / GitHub issues / Reddit, parallel) | – | ✓ |

---

## Bug matrix (fixed from the graveyard)

Real upstream issues/PRs that were closed-without-fix, rejected, or never got to — implemented and tested here. Each row is also a [port candidate](#contribute-by-porting).

| Area | Upstream # | What goose-plus fixes |
|---|---|---|
| Providers/reasoning | [#9397](https://github.com/aaif-goose/goose/issues/9397), [#9675](https://github.com/aaif-goose/goose/issues/9675) | DeepSeek / openai-compatible `reasoning_content` no longer dropped |
| Providers/reasoning | [#9434](https://github.com/aaif-goose/goose/issues/9434), [PR #10229](https://github.com/aaif-goose/goose/pull/10229) (ported) | A turn whose only thinking content was redacted (signature verification / content-filtering) produced an assistant tool-call message with no `reasoning_content` at all — DeepSeek rejects that outright. Emits a fingerprinted `[redacted:...]` placeholder instead, keeping the field present so split messages from the same turn still merge correctly while distinct turns don't |
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
| Server | [#10243](https://github.com/aaif-goose/goose/issues/10243), [PR #10245](https://github.com/aaif-goose/goose/pull/10245) (ported) | A reply request's completion handler disarmed the active-request guard before cleaning it up — if the task was cancelled in that window, the entry leaked in `active_requests` permanently, and every future request to that session failed with "Session already has an active request" until a manual cancel. Cleanup now runs before disarm |
| Desktop | [#9342](https://github.com/aaif-goose/goose/issues/9342), [#8997](https://github.com/aaif-goose/goose/issues/8997) | Chat history not loading; reply-render delay under reduced-motion |
| Providers/Bedrock | [#10006](https://github.com/aaif-goose/goose/issues/10006) | `ResourceNotFoundException` retried ~6 times (~2 min hang) on an invalid model name instead of failing fast |
| Providers/Bedrock | [#9888](https://github.com/aaif-goose/goose/issues/9888) | `max_tokens`/`temperature` dropped — not forwarded via `inferenceConfig` on Converse/ConverseStream |
| Providers/Bedrock | [PR #10046](https://github.com/aaif-goose/goose/pull/10046) (ported, relates to #10004) | `google.gemma-4-*` models (Mantle-only, not served by Converse) had their name mangled to `openai.google.gemma-4-*` by an unconditional strip-then-reprepend of the `openai.` prefix, so the Mantle-route check always missed and the request fell through to Converse with a `ValidationException` |
| Agent | [#9949](https://github.com/aaif-goose/goose/issues/9949), [#9963](https://github.com/aaif-goose/goose/issues/9963) | Recipe `extensions:` ignored when invoked via `delegate()`/`summon` subagents |
| Scheduler | [#10016](https://github.com/aaif-goose/goose/issues/10016) | `schedule sessions` always reports `Messages: 0` |
| Providers | [#9993](https://github.com/aaif-goose/goose/issues/9993) | Responses-API stream parser crashed on a malformed known event mid-stream instead of skipping it |
| Desktop | [#9881](https://github.com/aaif-goose/goose/issues/9881) | Extension toggle in Settings snapped back to On while disabling |
| Desktop | [#8135](https://github.com/aaif-goose/goose/pull/8135) | A tool call with no arguments, output, logs, or progress still showed a clickable expand chevron that opened onto an empty panel — `hasContent` is now computed from the same data driving the panel instead of inferred from render output |
| Providers | [#10032](https://github.com/aaif-goose/goose/issues/10032) | `GOOSE_CONTEXT_LIMIT` clobbered a known per-model context window instead of acting as a fallback |
| Server | [#10163](https://github.com/aaif-goose/goose/issues/10163), [PR #10165](https://github.com/aaif-goose/goose/pull/10165) (ported) | `/model-info` — which drives the Desktop token indicator and auto-compaction threshold display — only applied `GOOSE_CONTEXT_LIMIT`/declarative-provider limits on its fallback path; the common success path returned a hardcoded 128k for any model without a canonical registry entry, regardless of an explicit override. Now resolves the effective limit through the same path session/agent creation uses on every path |
| Developer tools | [#5444](https://github.com/aaif-goose/goose/issues/5444) | Analyze tool's `follow_depth`/`max_depth` params rejected a string-typed value, only accepting a plain number |
| Context | [PR #10095](https://github.com/aaif-goose/goose/pull/10095) (ported) | `count_chat_tokens` counted every image (top-level or inside a tool result) as 0 tokens. This estimate gates auto-compaction when provider usage metadata is absent, so a screenshot-heavy conversation ran the estimate light in the dangerous direction — compaction fired late and context silently overflowed. Now estimates image cost from decoded pixel dimensions, clamped and never zero |
| Agent | [#8496](https://github.com/aaif-goose/goose/issues/8496) | Delegated agents treated `inherit` as a literal provider/model instead of falling back to the parent session's config |
| Agent | [#9644](https://github.com/aaif-goose/goose/issues/9644), [PR #9650](https://github.com/aaif-goose/goose/pull/9650) (ported) | `GOOSE_SUBAGENT_PROVIDER`/`GOOSE_SUBAGENT_MODEL` were checked last in `resolve_provider`/`resolve_model_config`'s fallback chain, so an operator's explicit override was silently bypassed whenever the orchestrator LLM injected its own `provider`/`model` into a `delegate()` call. The env vars now win over LLM-injected params |
| Recipes | [#5280](https://github.com/aaif-goose/goose/issues/5280) | Recipe parse failures now show the raw recipe content, not just the parser error |
| Agent/Code mode | [PR #10214](https://github.com/aaif-goose/goose/pull/10214) (ported) | `execute_typescript`/`execute_bash` had no timeout and ignored their cancellation token; since pctx serializes execution behind a process-wide V8 mutex, one hung script wedged code execution for every session in the process. Now races execution against the extension timeout and real cancellation, releasing the mutex either way, and propagates cancellation to nested tool calls (e.g. a script's `developer.shell` call) so they're killed too instead of orphaned |
| CLI | [#6224](https://github.com/aaif-goose/goose/issues/6224) | PowerShell `term init` output had leftover double braces — invalid PowerShell syntax |
| Desktop | [#5915](https://github.com/aaif-goose/goose/issues/5915) | `goosed`'s spawned environment could be missing the PATH entries a locally-installed `claude`/`codex` CLI needs to resolve |
| Context | [`micn/tool-sum-fixes`](https://github.com/aaif-goose/goose/tree/micn/tool-sum-fixes) | Tool-pair summarization created stale "ghost" messages once compaction could handle the context anyway — now off by default, gated to ≤65,536-token windows |
| Scheduler | [#5346](https://github.com/aaif-goose/goose/issues/5346) | A scheduled job whose recipe failed to load (bad YAML, no `prompt`/`instructions`) failed with only a log line — now creates a session with a visible explanation |
| Providers | [#7449](https://github.com/aaif-goose/goose/issues/7449) | A turn with multiple tool responses, one carrying an image, interleaved the image message between tool_result blocks instead of after all of them — Claude (via any Claude-backed provider routed through the OpenAI-compatible format) rejects non-contiguous tool_result blocks |
| Providers/Agent | [#10080](https://github.com/aaif-goose/goose/issues/10080), [PR #10083](https://github.com/aaif-goose/goose/pull/10083) (ported) | The reply loop persisted a turn's signed thinking twice — once standalone, once attached to each split tool-call message — which `merge_consecutive_messages` glued into two adjacent identical signed blocks. Anthropic permanently rejects any session that reaches this shape with a 400, until repaired or abandoned. Fixed at the source (only persist the standalone copy when nothing else would carry the reasoning) and defensively (a new `dedupe_signed_thinking` repair pass drops exact duplicate signed blocks, covering already-contaminated sessions across every Claude transport) |
| CLI | [#2145](https://github.com/aaif-goose/goose/issues/2145) | `\` + Enter now inserts a newline in interactive input (the shell line-continuation convention), alongside the existing `Ctrl`+`J` binding — the completer's `Validator` was previously a stub that always submitted on Enter |
| Session storage | [`fix/sqlite-busy-connection-leak-7624`](https://github.com/aaif-goose/goose/tree/fix/sqlite-busy-connection-leak-7624) (adjusted) | Each `SessionManager` instance's SQLite pool used sqlx's default max size (10); multiple concurrent instances (e.g. one per ACP session) against the same file could multiply past what `busy_timeout` alone serializes. Capped at 2 connections per pool, not upstream's 1 — this fork's own `test_begin_immediate_prevents_lock_upgrade_deadlock` deliberately races two same-pool transactions and needs both slots, a real conflict caught only by running the full test suite before shipping |
| Security/ACP | [#10221](https://github.com/aaif-goose/goose/issues/10221), [PR #10223](https://github.com/aaif-goose/goose/pull/10223) (ported) | Disabling the Developer extension (Settings toggle, `enabled: false` in `config.yaml`) had no effect — `initial_session_extensions()` loaded every configured builtin unconditionally, so every new chat still got `shell`/`edit`/`write`/`tree`/`read_image` and could execute commands regardless of the toggle. Builtin selection now checks `is_builtin_disabled_by_user()`, which only treats an explicit `enabled: false` as an opt-out for extensions that are on by default — so an explicit `builtins` request (e.g. code mode's `code_execution`) still loads a default-off extension like `chatrecall` |
| MCP/OAuth | [#8453](https://github.com/aaif-goose/goose/issues/8453) ([`jhugo/fix-issue-8453`](https://github.com/aaif-goose/goose/tree/jhugo/fix-issue-8453), unmerged) | Remote MCP servers whose base URL returns a non-JSON HTTP 200 (or whose authorization server lives on a different host, or is behind a path-based deployment) made OAuth resource-metadata discovery fail fatally — no browser window ever opened, so auth-required remote extensions (e.g. Attio) could never connect. Adds an RFC 9728 / RFC 8414 well-known discovery fallback that builds the session through rmcp's public APIs when the primary discovery call fails |
| Agent | [#7425](https://github.com/aaif-goose/goose/issues/7425), [PR #7459](https://github.com/aaif-goose/goose/pull/7459) (adapted) | MOIM injection found the latest user-effective message and spliced its context block in — but when a turn ends with a text-only assistant reply (no tool call), that message was left trailing, and the conversation-repair pass silently dropped it to satisfy the "must end with user" constraint, discarding the model's own final response text. Now detects this case and appends a new trailing user message instead, preserving the assistant's text |
| Session storage | [#5094](https://github.com/aaif-goose/goose/issues/5094) (adapted) | Automatic session naming propagated any error from the naming LLM call (rate limit, network error) as a hard failure instead of the session just keeping its default name. Now falls back to a timestamped `Unnamed Session (...)` name |
| Local inference | [#10073](https://github.com/aaif-goose/goose/issues/10073), [PR #10105](https://github.com/aaif-goose/goose/pull/10105) (ported) | `goosed` crashed with `SIGILL` on x86_64 CPUs predating Haswell (no FMA) — the bundled llama.cpp CPU backend ran an FMA instruction unconditionally on init, taking down every backend-dependent feature. Now precheck the CPU's instruction sets (FMA/AVX2/F16C/BMI2/SSE4.2) before initializing and fail cleanly with an actionable error instead |
| Local inference | [PR #9666](https://github.com/aaif-goose/goose/pull/9666) (ported) | Some GGUF models (observed with Qwen2.5-Coder-32B) sample a ChatML turn delimiter like `<|im_start|>` mid-generation instead of stopping — not an EOG token, so nothing caught it, and the model rolled into a fabricated new turn (dozens of extra tool calls in one runaway response) while the delimiter leaked into streamed content verbatim. Now stops on any non-EOG `Control`-attribute token (except tool-call boundary markers, which must still reach the streaming parser) |
| Agent/Summon | [#7288](https://github.com/aaif-goose/goose/issues/7288), [PR #7297](https://github.com/aaif-goose/goose/pull/7297) (adapted) | `build_subrecipe_description` returned a subrecipe's explicit `description` immediately, skipping the recipe-file parameter lookup entirely — so any subrecipe with an explicit description (the common case for well-documented recipes) never told the delegating LLM what parameters it accepts. Now always loads the recipe file and appends its parameter list regardless of whether an explicit description was set — found via the branch graveyard's spot-check pass below, first genuinely new fix out of 9 candidates deep-read |
| CLI/Scheduler | [#6405](https://github.com/aaif-goose/goose/issues/6405), [PR #6416](https://github.com/aaif-goose/goose/pull/6416) (adapted) | `build_session` (the CLI's headless/interactive session entry point) never wired a scheduler into the agent — `Agent::new()`'s bare constructor leaves `scheduler_service: None`, unlike the ACP/desktop path via `AgentManager::instance()`. Any recipe or session calling the schedule-management tool from the CLI failed with "Scheduler not available" on every platform, not just the AARCH64 case upstream reported. Extracted `build_cli_scheduler_service`, mirroring the working ACP/desktop wiring |
| Scheduler | [PR #6096](https://github.com/aaif-goose/goose/pull/6096) (adapted) | `execute_job`'s prompt-text resolution silently discarded a scheduled recipe's `instructions` field whenever `prompt` was also set (an `.or()` chain only ever picks one) — the same class of bug `subagent_handler.rs`'s `get_agent_messages` already avoids for delegated subagents by treating `instructions` as system guidance and `prompt` as the user task, separately. Scheduled jobs had no equivalent split; only `prompt` ever reached the model. Now applies `instructions` via `agent.extend_system_prompt` when both fields are set, matching the existing subagent semantics — found via the exhaustive branch-graveyard verification pass below |
| Docs/MCP | [PR #10204](https://github.com/aaif-goose/goose/pull/10204) (ported) | The Browserbase MCP install docs were stale: `@browserbasehq/mcp`'s default (Gemini-backed Stagehand) configuration requires `GEMINI_API_KEY` alongside `BROWSERBASE_PROJECT_ID`/`BROWSERBASE_API_KEY` (confirmed against current npm/Browserbase docs), and the install command needs `npx -y`, not a bare `npx`. `documentation/static/servers.json` also still referenced the renamed `mcp-server-browserbase` package and was missing the `BROWSERBASE_PROJECT_ID` entry |
| Session storage | [PR #7454](https://github.com/aaif-goose/goose/pull/7454) (adapted; one rough-edge fix out of a larger "add lmstudio declarative provider" branch, the rest out of scope here) | `strip_xml_tags` (used to build session titles) only stripped balanced `<tag>...</tag>` pairs — a local model whose chat template already consumes the opening `<think>` tag left its raw reasoning text (ending in a bare `</think>`) completely unstripped, contaminating generated session names. Now also strips an orphan closing tag and everything before it |
| Desktop | [PR #7992](https://github.com/aaif-goose/goose/pull/7992) (adapted; ported the session-switch race guard, not the full multi-part branch) | `reloadConversation` (triggered when the SSE replay buffer overflows) had no guard against the user switching sessions while its `getSession` call was in flight — a stale response for the previous session could overwrite the newly-active session's messages. Now captures the session ID at call time and discards the response if the current session has since changed |
| Security/Apps | [PR #8010](https://github.com/aaif-goose/goose/pull/8010) (adapted; ported the path-sanitization fix, not the branch's UI-ordering changes, already covered here separately) | `load_app`/`save_app`/`delete_app` joined an LLM-tool-supplied app name directly into a filesystem path under the apps data dir with zero sanitization — a name containing `../` or a path separator could read, overwrite, or delete files outside the apps directory. Now validates the name against an allowlist charset before building any path |
| Agent | [PR #7492](https://github.com/aaif-goose/goose/pull/7492) (adapted; found via the medium/large-tier branch-graveyard pass, shared by [`baxen/develop2-testing`](https://github.com/aaif-goose/goose/tree/baxen/develop2-testing), never opened as a PR) | `moim.rs` had an auto-skip token threshold and env-var content overrides but no full on/off switch, unlike the existing `GOOSE_DISABLE_SESSION_NAMING`/`GOOSE_DISABLE_TOOL_CALL_SUMMARY` pattern. Adds `GOOSE_DISABLE_MOIM` |
| Agent/Subagents | [`dkatz/visibility-fixes`](https://github.com/aaif-goose/goose/tree/dkatz/visibility-fixes), never opened as a PR | `extract_response_text`'s `return_last_only` branch took the literal last message in a subagent's conversation instead of the last agent-visible one, so a trailing internal/invisible message could shadow the actual final reply reported back to the delegating agent |
| Observability | [PR #5590](https://github.com/aaif-goose/goose/pull/5590) (adapted; ported only the input/output span-capture concept onto this fork's already-centralized instrumentation, not the branch's per-provider duplication removal, which doesn't apply here) | `Provider::complete()`'s tracing span recorded `session.id` and the model name but never the actual request/response content, so an opt-in observability layer (e.g. Langfuse) attached to that span had nothing to show beyond timing. Now records truncated input/output, but only when the span is actually enabled, so there's no cost when nothing is subscribed |
| ACP/Config | [PR #8893](https://github.com/aaif-goose/goose/pull/8893) (adapted to this fork's config internals, which have diverged substantially since the branch was written) | `GooseAcpAgent` stores a per-instance `config_dir` (correctly used by `PermissionManager`), but `self.config()` always returned `Config::global()`, so ACP preferences/dictation/onboarding reads and writes silently used the process-wide config instead of the session's own directory whenever `config_dir` differed from the default — a real bug for concurrent per-directory ACP sessions |
| ACP/Session | [PR #9483](https://github.com/aaif-goose/goose/pull/9483) (adapted) | `on_set_session_system_prompt` only mutated the in-memory `PromptManager`, so a client-set persona/system-prompt override was silently lost whenever the agent was rebuilt (daemon restart, LRU eviction, session resume). Persists `Set`-mode overrides and `client_`-prefixed `Append`-mode extras to the session record and re-applies them whenever a session's agent is (re)activated; server-managed append keys (`recipe`, `final_output`, `recipe_instructions`) are intentionally excluded since they're rebuilt from other session state |
| Desktop | [PR #7545](https://github.com/aaif-goose/goose/pull/7545) (adapted; author stress-tested it for days and precisely diagnosed the root cause, but the PR sat with zero reviews/comments and was closed unmerged) | `resultsCache` in `useChatStream.ts` grew without bound over a long-running session (only cleared on explicit session deletion, never on the existing tab-eviction path) — added a weight-aware LRU cache capped at 5 sessions and wired eviction into `App.tsx`'s tab-limit logic. Independently found the same investigation missed: `goosed.ts`'s `stopErrorLogCollection()` removed the stderr `data` listener after startup but never called `resume()`, unlike the symmetric stdout handling right next to it, leaving the stream paused so goosed's own stderr writes would eventually block on a full OS pipe buffer. Also replaced `read-file`'s non-Windows `cat` subprocess spawn with the same native `fs.readFile` already used on Windows |
| Providers | [#9688](https://github.com/aaif-goose/goose/issues/9688) | Canonical model registry listed a 393216 max output-token limit for `nvidia/deepseek-ai/deepseek-v4-pro`, exceeding NVIDIA's actual API cap — confirmed against the reporter's own quoted first-party NVIDIA error message ("This model supports at most 262144 completion tokens"). Corrected to 262144; the model's 1048576 context limit is unaffected |
| Developer tools | [PR #9748](https://github.com/aaif-goose/goose/pull/9748) (adapted; ported only the process-group-kill fix, not the PR's larger environment-capture/working-dir-requirement changes, which are already handled differently or out of scope here) | The developer shell tool's timeout/cancellation path only called `child.start_kill()`, signaling the tracked child alone — but `configure_subprocess` already places every shell command in its own process group, so a script backgrounding a long-lived job (e.g. `sleep 100 &`) left it running, orphaned, after the tool call returned. Now sends `SIGKILL` to the whole process group first, falling back to `start_kill()` |
| Providers | [PR #9832](https://github.com/aaif-goose/goose/pull/9832) (ported) | Weak/cheap models occasionally emit tool-call arguments that are valid JSON but not an object (a bare array, string, or number). Both the OpenAI/OpenRouter and Databricks response decoders (streaming and non-streaming) passed the parsed value straight to rmcp's `object()`, whose `debug_assert!(value.is_object())` panics the process in debug builds and silently discards arguments in release. Now guards both decoder sites and returns an `INVALID_PARAMS` tool error instead, so the model gets feedback and can retry |
| Desktop | [PR #5349](https://github.com/aaif-goose/goose/pull/5349) (ported) | `goosed`'s startup healthcheck poll budget had regressed from a merged 120s back down to 30s in a later refactor — not enough time to enter a keychain password when running `goose ui` from source, leaving the app stuck |
| Providers | [PR #10000](https://github.com/aaif-goose/goose/pull/10000) (adapted; the PR's own diagnosed root cause — an SSE 8KiB line-buffering limit — was refuted, but the same investigation surfaced a real, different gap) | A `response.completed` stream event whose `response` object was missing an `id` field (e.g. from a non-conformant proxy) failed `serde_json::from_value` outright and was silently dropped — including its final usage stats and any output only present in that event — instead of degrading gracefully. Made `id` optional (`#[serde(default)]`) on `ResponseMetadata` and `ResponseOutputItemInfo::Reasoning` |
| Providers/Bedrock | [#2150](https://github.com/aaif-goose/goose/issues/2150) | `to_bedrock_tool` never stripped top-level `oneOf`/`allOf`/`anyOf` keys from a tool's `input_schema`, which Bedrock's Converse API rejects outright ("input_schema does not support oneOf, allOf, or anyOf at the top level") — breaking any Claude-on-Bedrock session as soon as an MCP tool with a union-typed schema (common with Pydantic-generated schemas) was in play. Now strips them before sending, since the model can still respect the alternatives via property descriptions |
| Agent | [#2885](https://github.com/aaif-goose/goose/issues/2885) | Stdio extension `args` were passed straight to `Command::args()` with no shell involved, so a leading `~` in an arg (e.g. `--prefix ~/my-server`, a common pattern in copy-pasted MCP server setup commands) was never expanded to the user's home directory the way running the same command line in an actual shell would. Now expands tildes in each arg before launch |
| Desktop | [PR #4626](https://github.com/aaif-goose/goose/pull/4626) (adapted) | The desktop app's CSP blocked MCP-UI webviews from loading external stylesheets (`style-src`/`style-src-elem`) and non-`self` media (`media-src`/`media-src-elem`) — common needs for MCP-UI content (e.g. web fonts, blob/data URLs). Relaxed both directives to allow `https:`/`blob:`/`data:` sources, matching the merged upstream PR |
| Providers | [#4540](https://github.com/aaif-goose/goose/issues/4540) | The cursor-agent provider piped the CLI's stderr but never read it, so any command failure only reported a bare exit code with no indication of what actually went wrong (auth errors, invalid model names, etc. were all swallowed). Now drains stderr concurrently with stdout and surfaces it in the error message |

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
| Desktop/Security | [`shellz-n-stuff/sandbox-impl-python-ssh-proxy`](https://github.com/aaif-goose/goose/tree/shellz-n-stuff/sandbox-impl-python-ssh-proxy) (draft PR [#7206](https://github.com/aaif-goose/goose/pull/7206), closed for inactivity, never reviewed/merged; `736557d33` own work) | ⚠️ **UNVERIFIED ON MACOS — do not treat as a tested security boundary.** Opt-in (`GOOSE_SANDBOX=true`, macOS-only) `sandbox-exec` seatbelt profile intended to deny `goosed` direct outbound network except localhost, forcing egress through a local HTTP CONNECT proxy with a live-reloaded domain blocklist, IP/loopback/SSH-host restrictions, and an optional LaunchDarkly egress-allowlist flag. Ported and adapted on a Linux dev machine — verification was limited to `tsc --noEmit`, eslint, and unit tests of the pure blocklist/domain logic. **The actual `sandbox-exec` profile, the seatbelt syntax, and the egress proxy have never been exercised on real macOS hardware.** Do not rely on this for actual isolation until someone has run it on macOS and confirmed the seatbelt profile actually blocks what it claims to. Follow-up RCA confirmed the underlying proxy mechanism is at least architecturally sound: `goosed`'s actual HTTP client (`reqwest`, `system-proxy` feature enabled in both `crates/goose/Cargo.toml` and `crates/goose-server/Cargo.toml`) does honor `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY`, and no provider overrides that with an explicit `.proxy()`/`.no_proxy()` call — so egress should actually route through the local proxy as designed. That does **not** substitute for running the real `sandbox-exec` profile on macOS. Dropped the draft's vestigial static `sandbox.sb` template, made dead by its own later commit switching to programmatic profile generation. `b4e54cefb` (own work) later added a loud runtime warning logged the moment `GOOSE_SANDBOX=true` actually spawns `goosed` through the seatbelt — the unverified-on-real-hardware caveat above previously lived only in this doc, which nobody reading logs at 2am has open |
| Developer tools | [`developer_search`](https://github.com/aaif-goose/goose/tree/developer_search) (draft PR [#6683](https://github.com/aaif-goose/goose/pull/6683), closed silently with zero reviews/comments; `8b175708d` own work) | New `search` tool on the developer extension — queries Sourcegraph code search, the GitHub Search API (issues), and Reddit in parallel with per-source timeouts, for "how have others solved this" research without a full web-search round trip. The draft shipped with zero non-network unit tests; added real coverage for the pure formatting/filtering logic instead |
| Skills | [`air-gapped-docs-minimal`](https://github.com/aaif-goose/goose/tree/air-gapped-docs-minimal) (no PR ever opened) | `GOOSE_DOCS_ROOT` opts the builtin `goose-doc-guide` skill out of hardcoding `https://goose-docs.ai`, pointing it at a local path or private mirror for air-gapped/offline installs — matches this fork's existing pattern of narrow, opt-in env-var config knobs |

> The `#` numbers link to the upstream [aaif-goose/goose](https://github.com/aaif-goose/goose) issue/PR tracker. Where a closed/unmerged PR existed, goose-plus reused its diff as a starting point — making those rows the cheapest to port back. For no-ticket rows, `Source` links the upstream branch (verified live; if a link 404s after the branch is deleted, `git log --all --grep=<branch>` on this repo still finds the porting commit) or gives the goose-plus commit hash for original fork work.

---

## Graveyard status: what's actually done vs. still open

Be honest about scale here rather than implying the matrices above are exhaustive. Two corrections
worth stating plainly, found by going past this doc's own citations to the real git history:

- **This repo was accidentally a shallow git clone** for a stretch, which silently breaks
  `git merge-base`/ahead-behind comparisons against upstream. Fixed via
  `git fetch --unshallow upstream`. If a fresh clone or CI checkout of this repo ever needs
  accurate ahead/behind numbers against `aaif-goose/goose`, unshallow first
  (`git fetch --unshallow upstream`) or the numbers will be nonsense.
- **~222-232 distinct improvements have actually landed**, not the ~128 an earlier count implied.
  155 individually-cited issues/PRs/branches (117 issues/PRs + 38 branches) — this figure is
  mechanically regenerated from the doc itself
  (`grep -oE "issues/[0-9]+|pull/[0-9]+|tree/[A-Za-z0-9_./-]+\)" GOOSE_PLUS.md | sort -u`),
  never hand-incremented. An earlier version of this line hand-tallied "108 → 121" one row at a
  time while landing 13 fixes in one stretch, which undercounted: most of those rows cite *both*
  an issue number and a PR number, so 13 new rows added 21 new distinct citations, not 13. Lesson
  applied: re-run the extraction command, don't count by hand. ~70 fixes from the 4 internal audit
  sweeps (documented as one summary line, not per-item rows, which is why a citation-count
  undercounts them), and 15 headline own-work features. Treated as an evidenced estimate, not false
  precision — the three buckets likely have some overlap a full one-by-one reconciliation would
  resolve.

Real remaining candidate pool. **A previous count here (579 closed issues, 519 closed PRs,
141 open issues, 75 open PRs, 108 branches = 1422 total) was wrong by roughly 7x** — it came from
a broken earlier data-extraction pass (one of its intermediate cache files was found to contain
49.8 million lines of runaway/duplicated output, not real API data). Re-derived fresh via the
GitHub search API and cross-checked against the repo's actual highest issue/PR number (10282,
which lines up with the corrected total below — the earlier undercounted figures did not):

- **2,269 closed issues, 184 open issues, 7,408 closed PRs, 97 open PRs, 617 upstream branches**
  = **10,575 total** (re-measured; drifts by a handful day to day as upstream stays active), of
  which only 3 branches are mechanically already-merged into `upstream/main`. Of the corrected
  10,576/10,575, roughly 20 items have been individually, manually resolved across recent sessions
  (13 landed as real fixes with full build+test+clippy+mutation verification, the rest confirmed
  already-covered/superseded/architecture-mismatched/unreproducible — see
  `scratchpad/remaining.txt` for the itemized reasoning on each). At this scale, that is a rounding
  error, not meaningful progress against the total — be explicit about that rather than implying
  otherwise.
- **The zero-engagement filter has now been redone against the corrected totals** (previously
  flagged untrustworthy — it was computed against the corrupted 579/519-item pool). Fetched every
  closed issue and closed PR fresh via the GitHub REST issues-list endpoint (not the search API,
  which caps at 1,000 results — too small for this dataset), paginated with per-page retries;
  fetched counts matched the API's live `total_count` exactly (2,269 / 7,408) confirming no
  corruption this time. "Zero-engagement" = 0 comments AND 0 reactions, checked against the pool
  with already-cited numbers excluded:
  - **Closed issues**: 2,249 not-yet-cited candidates → **318 zero-engagement (14.1%)**, 1,931 with
    real engagement (comments or reactions) still sitting un-triaged.
  - **Closed PRs**: 7,387 not-yet-cited candidates → **2,786 zero-engagement (37.7%)**, 4,601 with
    real engagement still un-triaged.
  - Combined: **3,104 zero-engagement** (truly no signal — lowest priority to mine) vs.
    **6,532 with real engagement** (comments and/or reactions — the actual high-value triage pool
    for closed issues/PRs). Open issues (184) and open PRs (97) aren't zero-engagement-filtered —
    being open is itself a live signal.
  - **Branch classification, also now done** (was the last open item): 619 upstream branches, of
    which 2 are housekeeping refs (`HEAD`, `main` itself) and 1 (`micn/testing-live`) is already
    merged, leaving 616 real candidates. Bulk-joined against a single `gh pr list --state all
    --limit 8000` call (not per-branch lookups) by `headRefName`: **6 MERGED** (squash-merged, so
    git ancestry missed them but the content already landed — excluded), **396 CLOSED** (a PR
    existed and was explicitly not merged — classic graveyard), **42 OPEN** (already covered by
    normal PR mining, deprioritized for this sub-pass), **172 none** (pushed, no PR ever opened).
    Size-tiered the CLOSED+none pool (568) by files-changed vs. `upstream/main`: 56
    release/version-snapshot branches excluded, 0 zero-diff, **307 small (≤10 files)**, 135 medium
    (11–30), 70 large (30+) — medium/large deprioritized for this pass, named not dropped.
  - Fanned out 12 haiku agents (one per ~27-branch chunk) across the **entire** small tier — not a
    sample — classifying each from its `git log -3`/changed-file list alone (no full diffs at this
    stage): 210 raw PROMISING calls. A raw haiku count is not a citable number until it's checked
    against every way this doc already cites something, so three independent cross-checks were run
    before trusting it: **(1) branch name** — 33 were already cited by their exact `tree/<name>`
    link elsewhere in this doc. **(2) PR number** — joining every remaining branch to its actual PR
    (via the same bulk `gh pr list` data) found 7 more whose fix landed and got cited under a
    *different* branch name (e.g. `dkatz/session-name-errors` → PR #5094 → already cited as issue
    #5094). **(3) number embedded in the branch name itself** — 8 more where the branch name
    contains the issue number directly (`goose/issue-6293`, `wt-goose-8391`, `fix/inherit-sentinel-
    8496`) and that number is already cited, which neither of the first two checks catches. All
    three overlaps are also a validation signal for the method: the haiku agents independently
    flagged real, already-landed fixes as PROMISING with zero knowledge they were already done.
    Net of all three rounds: **164 genuinely new PROMISING**, 51 UNCLEAR, 43 SKIP-experiment,
    1 SKIP-superseded, out of 267 small-tier branches not already accounted for by name/PR-number.
    The same cross-checks applied to medium/large tiers found 1 more already-covered medium-tier
    branch (net: 134 medium, 70 large — unaffected) and 4 already-covered branches in the
    deprioritized OPEN-PR bucket. Every count here is regenerated from source (result files →
    `pr_history.json` join → branch-name regex), never trusted from a narrative claim or the
    agents' own self-reported sums (two of which arithmetic-drifted mid-response before
    self-correcting). All 164 of these branches have since been individually deep-read, verified,
    and given a final disposition — none left in an open "still needs triage" state. Final
    breakdown: **106 already covered, 28 not applicable, 7 ported, 22 deferred as scope/product
    decisions, 1 uncertain** (164 = 106+28+7+22+1). The medium-tier (134) and large-tier (70)
    branch buckets — 204 more — have since had this same exhaustive treatment too (see below), and
    so have the 51 small-tier UNCLEAR branches (flagged by the original log/file-list-only triage
    as needing "a second, deeper look" — not simply skipped). Across all four batches, **419
    branches (164 small-PROMISING + 51 small-UNCLEAR + 204 medium/large) have now been
    individually deep-read and given a terminal disposition** — none left open. The honest
    "genuinely worth a human/agent triage pass" pool, replacing the old untrustworthy "~1354"
    figure, is now down to just **6,532 closed issues/PRs with real engagement + 281 open
    issues/PRs ≈ 6,813**. Not counted at all: 3,104 zero-engagement issues/PRs, or the 43
    SKIP-experiment branches — those were classified by the original log/file-list-only pass as
    personal/broken/draft fragments and have NOT had the same deep-read verification as the
    UNCLEAR bucket; that classification is plausible on its face but hasn't been individually
    confirmed the way everything else in this section has.
  - **Full-pool verification, not a sample: all 164 small-tier PROMISING branches have now been
    individually deep-read** (full diff + current-codebase comparison, not just log/file-list) —
    15 by hand in an initial spot-check, then the remaining 149 exhaustively via 11 parallel
    verification passes covering every branch, none skipped, then every survivor pushed to a
    final disposition (ported, already-covered, not-applicable, or deferred) rather than left as
    an open "still needs triage" queue. Mechanically-tallied result from the 149-branch pass: 94
    ALREADY_COVERED (often by more sophisticated or architecturally-relocated fixes than the
    upstream branch proposed — e.g. OAuth token refresh already lives in the vendored rmcp SDK's
    `CredentialStore`, tool-name resolution already generic in `extension_manager.rs`'s
    `resolve_tool()`), 24 NOT_APPLICABLE (most commonly: the branch targets `ui/goose2/`, an
    experimental UI tree that doesn't exist in this fork — 9 of the 24 — or a shell
    script/workflow this fork never carried over), 1 UNCERTAIN (`jhugo/fix-hermit-cmake-linux-
    arm64` — the literal proposed fix isn't present, but confirming whether it's actually needed
    requires live access to hermit's package repo, out of scope for this pass), and 30
    GENUINELY_NEW. Several of the "genuinely new" raw agent verdicts didn't survive a second,
    deeper look and were corrected before being trusted:
    - `alexhancock/oauth-auth-server-uri` → ALREADY_COVERED: reading the vendored `rmcp` 1.8.0 SDK
      source directly showed `OAuthState::start_authorization_with_metadata_url` already calls
      `discover_metadata()`, doing full RFC 9728 resource-metadata discovery (a different-host
      authorization server) before falling back to same-host — upstream's manual pre-resolution
      was a workaround for an older rmcp version that no longer applies.
    - `micn/replay-bug` → ALREADY_COVERED: its target (`sseMaxRetryAttempts: 1`) already exists at
      `useSessionEvents.ts:61`, a more sophisticated reconnect implementation (exponential
      backoff, Last-Event-ID resumption, terminal-error broadcast) than upstream's target — the
      first-pass agent checked `useChatStream.ts`, where this fork's actual SSE logic no longer
      lives, and found nothing.
    - `zane/icon-tooltips` → NOT_APPLICABLE: the `SidebarTrigger`/"New window" button it adds
      tooltips to doesn't exist anywhere in the current desktop UI (repo-wide grep, zero matches)
      — removed in the same `NavigationPanel` refactor that made `zane/hide-new-chat` not
      applicable either.
    - `feat/task-completion-notification` → ALREADY_COVERED: `useChatStream.ts` already fires a
      task-completion notification, more sophisticated than the branch's version (a real
      `notificationsEnabled` setting, multi-window-aware `isAnyWindowFocused()`, i18n strings) —
      wiring the branch's simpler version into `onStreamFinish` too would double-fire it.

    Of the remaining 26, **1 was ported** (`air-gapped-docs-minimal` — no PR, small, matched this
    fork's established pattern of narrow opt-in env-var config knobs like
    `GOOSE_SKIP_CONTEXT_FILES`/`GOOSE_CONFINEMENT` — see the feature-matrix "Shipped without an
    upstream ticket" section), **5 were ported earlier** (`dkatz/subagent-instructions-fix` →
    PR #6096, `codex/browserbase-gemini-key` → PR #10204, `zane/lmstudio` → PR #7454,
    `jhugo/fix-sse-reconnect-followup` → PR #7992, all in the bug matrix above), and **the
    remaining 20 are DEFERRED as scope/product decisions, not bugs** — each proposes a genuinely
    new subsystem or capability (a fast-model-races-the-main-agent reply system, a new sandboxed
    reader extension, a cross-conversation user-memory module, a second mesh provider
    implementation, a second-opinion "oracle" tool, a context-dedup infrastructure layer, a
    bundled-personalities feature, a TUI model-switcher, a feedback-widget UI, provider
    auto-detection, a fuzzy-match/patch text-editing pipeline, model-preloading, an npm
    release-version-sync tool built around upstream's own publish workflow, and a full
    `ARCHITECTURE.md` write-up) — none of these clear the "would a thoughtful maintainer
    unambiguously accept this exact diff" bar the rest of this doc holds itself to. Each is a
    legitimate roadmap candidate needing explicit scoping, not a diff to silently land from an
    abandoned branch — the same judgment already applied to `alexhancock/developer-only` (deferred
    for the identical reason: a product/UX default-tools decision, not an unambiguous bug).

    Across the full 164-branch pool, final tally: **106 already covered, 28 not applicable,
    7 ported, 22 deferred as scope decisions, 1 uncertain** (164 = 106+28+7+22+1). All 20 deferred
    candidates remain catalogued with exact file:line evidence in
    `scratchpad/graveyard-data-v2/verify_chunks/all_verified.json` (filter `verdict==DEFERRED`)
    for whenever that scoping conversation happens — nothing was silently dropped, each has a
    named reason. None of the 106 already-covered branches' fixes were cited anywhere in this doc
    by number or name before this pass, so the mechanical citation-exclusion above correctly
    couldn't have caught them — they were absorbed by the 4 internal audit sweeps or an earlier
    untracked session, not by a citable graveyard port. Raw data + scripts are in
    `scratchpad/graveyard-data-v2/` for anyone who wants to re-run or extend this.
- **The medium-tier (134) and large-tier (70) branch buckets — 204 total — have now had the
  same exhaustive, per-branch deep-read treatment as the small tier**, closing the gap the small-tier
  pass explicitly left open. Fanned out 21 parallel verification agents (~10 branches each, sorted
  by files-changed ascending) with a five-way classification taxonomy refined from the small tier's
  three-way one specifically to avoid re-litigating "is this a bug or a new subsystem" after the
  fact: ALREADY_COVERED, NOT_APPLICABLE, BUG_FIX_CANDIDATE (a narrow, scoped correctness gap in
  existing functionality — port it), NEW_CAPABILITY (a genuinely new subsystem/feature — defer, never
  silently build), UNCERTAIN. Results were aggregated mechanically (read every `mlresult_NN.json`,
  `Counter()` the verdicts — never trusted a chunk agent's own self-reported tally) into
  `scratchpad/graveyard-data-v2/ml_all_verified.json`. Mechanical tally across all 204 branches: **84
  ALREADY_COVERED, 59 NEW_CAPABILITY, 48 NOT_APPLICABLE, 9 BUG_FIX_CANDIDATE, 4 UNCERTAIN**
  (84+59+48+9+4 = 204, no duplicates).
  - The 9 BUG_FIX_CANDIDATE findings collapse to **8 distinct fixes** — `baxen/develop2-testing` and
    `alexhancock/develop2-test-fixes` both surfaced the identical MOIM-disable gap (same 3-commit
    core; the second branch is a test-fixes-only follow-up on the first). Each of the 8 was
    independently re-verified against the current codebase (not just the chunk agent's one-line
    summary) before any porting decision, per this session's standing practice that a first-pass
    agent verdict on "is this a small fix or a new subsystem" is not trustworthy without a second,
    deeper look.
  - **6 ported** with the full rigor cycle (tests, mutation-testing, clippy, fmt, baseline-comparison,
    docs) — see the bug matrix above: the apps.rs path-traversal fix (PR #8010), `GOOSE_DISABLE_MOIM`
    (PR #7492 / `baxen/develop2-testing`), the subagent visibility-filter fix
    (`dkatz/visibility-fixes`), the `complete()` tracing input/output capture (PR #5590), the ACP
    per-directory config cache (PR #8893), and session system-prompt persistence (PR #9483).
  - **2 deferred**, both requiring explicit operator/maintainer judgment rather than a silent port:
    - `alexhancock+aharvard/app-csp-permissions` — the underlying data model
      (`CspMetadata`/`PermissionsMetadata`/`UiMetadata`) and the desktop renderer that consumes it
      already exist in this fork; what's missing is parsing an app's `_meta.ui` JSON-LD block on the
      `create_app`/`iterate_app` path. But completing that means letting **LLM-generated app content
      declare camera/microphone/geolocation/clipboard-write permissions with no human review step** —
      a real product/security decision about what an LLM-authored web app should be allowed to
      request, not a narrow correctness gap. Needs explicit scoping before implementation.
    - `fix/9113-restore-gemini-acp` — upstream's own maintainer (`DOsinga`) publicly confirmed the
      currently-shipping `gemini_oauth.rs` provider "reuses Gemini CLI's OAuth credentials to hit
      Google's Code Assist API directly, which violates Google's TOS for third-party tools and can
      lead to account suspension" (issue #9113), wrote a replacement (`gemini_acp.rs`, built on the
      same shared `AcpProvider` infrastructure this fork's `claude-acp`/`codex-acp`/etc. already use),
      kept the PR (#9309) rebased through ~110 commits of drift over six weeks — then closed it
      himself without merging, for reasons not stated in any visible comment (a community reply on
      the issue objected to the specific "spawn `gemini --acp` over stdio" approach). Upstream `main`
      still ships the flagged provider unchanged and issue #9113 remains open, unresolved. This is a
      real, live TOS/account-suspension risk in currently-shipping code — but silently replacing or
      removing a shipping provider that users depend on, over an approach the original maintainer
      himself walked back, is a product decision this fork should make deliberately, not one to
      silently inherit from an abandoned branch.
- **The 51 small-tier UNCLEAR branches have now had the "second, deeper look" they were always
  flagged as needing** — the original triage classified them from `git log -3`/file-list alone
  (no diff content), and each carries a specific noted reason it couldn't be resolved that way
  (vague commit messages, near-duplicate sibling branches, "might be incomplete WIP", "unclear if
  still relevant"). Applied the same 3-layer citation exclusion used elsewhere first (2 were
  already cited: `fix/8495-custom-headers-comma-in-quotes` → PR #8495, `goose/issue-7449` → #7449),
  leaving 49. Fanned out 5 parallel deep-read agents (~10 branches each) instructed to read the
  actual diff and resolve the SPECIFIC noted ambiguity, not just re-read the log. Mechanically
  aggregated (never trusted a chunk's own tally): **27 ALREADY_COVERED, 14 NOT_APPLICABLE,
  7 NEW_CAPABILITY, 1 BUG_FIX_CANDIDATE** (27+14+7+1=49).
  - The 1 BUG_FIX_CANDIDATE (`zane/evict-sessions-from-cache-new`) was independently re-verified
    before porting — see PR #7545 in the bug matrix above. Notably, verifying it surfaced a second,
    related bug the branch itself never mentioned (`goosed.ts`'s stderr listener removal missing a
    matching `resume()` call), found only by reading the current file closely enough to compare it
    against the adjacent, symmetric stdout handling — the same "a first pass can miss something a
    second, deeper look catches" lesson that recurred throughout this branch-mining effort.
  - The 27 ALREADY_COVERED branches confirm, not undermine, the original triage's honesty about its
    own limits: e.g. `micn/health-checks`/`micn/live-tests`/`micn/live2` are confirmed (via full git
    history on the path) to be direct lineage predecessors of the fork's current
    `pr-smoke-test.yml`; `pr-5607`/`pr-5610`/`pr-5614` are a near-duplicate/duplicate triplet (5610
    and 5614 are byte-identical) targeting a Hub/TopNavigation tile-grid architecture this fork
    replaced with an independently redesigned `Hub.tsx`/`NavigationPanel.tsx`.
  - The 7 NEW_CAPABILITY branches (a client-fingerprinting mechanism, a subrecipe-import subsystem,
    a whole customizable-navigation UI, a persistent-shell-session extension, etc.) are scope
    decisions, not fixes — not ported, consistent with every other NEW_CAPABILITY verdict in this
    doc.
  - All three branch-size tiers are now fully resolved to a terminal disposition. Combining the
    small-tier PROMISING batch (164) with this UNCLEAR batch (51): **135 already-covered
    (106+27+2 excluded-via-citation), 42 not-applicable (28+14), 8 ported (7+1), 22 deferred +
    7 new-capability (29 total scope decisions), 1 uncertain** — 135+42+8+29+1 = 215 small-tier
    branches. Combined with the 204 medium/large-tier branches: **419 branches across all three
    size tiers** now have a verified terminal disposition, none left in an open "still needs
    triage" state.
- **The 208 not-yet-cited open issues/PRs (141 open issues + 67 open PRs) have now had the same
  exhaustive per-item deep-read treatment as the branches** — the next-smallest bounded pool after
  the branch graveyard was fully closed, picked deliberately over diving straight into the much
  larger 6,532-item closed-with-engagement pool. Excluded already-cited numbers first (both pools
  checked against the full citation list mechanically), fanned out 12 parallel deep-read agents (8
  for issues, 4 for PRs — PR agents additionally pulled `gh pr diff` for the actual code change,
  not just the description) using the same five-way taxonomy. Mechanically aggregated (never
  trusted a chunk's own tally):
  - **Issues (141): 34 ALREADY_COVERED, 24 NOT_APPLICABLE, 28 BUG_FIX_CANDIDATE, 46
    NEW_CAPABILITY, 9 UNCERTAIN.**
  - **PRs (67): 9 ALREADY_COVERED, 12 NOT_APPLICABLE, 17 BUG_FIX_CANDIDATE, 29 NEW_CAPABILITY,
    0 UNCERTAIN.**
  - **Combined: 208 items, 43 already-covered, 36 not-applicable, 45 bug-fix-candidate,
    75 new-capability (deferred scope decisions), 9 uncertain.**
  - One issue-level BUG_FIX_CANDIDATE (#9113) duplicates the branch-level Gemini-ACP/TOS finding
    already covered above (deferred, needs operator sign-off) — not double-counted. Of the
    remaining **44 distinct BUG_FIX_CANDIDATE findings, 2 were independently re-verified and
    ported this stretch** (see the bug matrix above: #9688's stale NVIDIA output-token limit, and
    PR #9748's process-group-kill fix) — **42 remain identified but not yet independently
    re-verified or ported**, each already backed by concrete file/line evidence from a real
    deep-read (not a guess), catalogued here rather than silently dropped:
    - Issues (26): #3468 (OAuth token cache stored plaintext), #8993 (Code Mode `ToolKind::Execute`
      not set per maintainer's own follow-up), #9136 (no per-turn tool-call-burst cap, distinct
      from the existing no-progress-turn guard), #9332 (`PR_SET_PDEATHSIG` thread-affinity race),
      #9467 (`flake.nix` missing `cargoLock.outputHashes` for git-sourced deps), #9526 (slash-command
      recipes don't wire `sub_recipes` into Summon), #9534 (`cache_control` not gated for
      `SessionType::SubAgent` — real cost issue, but needs `SessionType` threaded through the
      `Provider` trait's call chain or a per-call session lookup, not a one-line fix), #9674 (OAuth
      fallback primitives exist in `extension_manager.rs` but aren't fully wired), #9758/#9766 (moim
      turn-context/turn-bounding edge cases), #9782 (code-analysis language registry gaps), #9822
      (`claude_code.rs` subprocess missing the PDEATHSIG hardening pattern used elsewhere), #9910
      (extension-count nudge threshold miscalculation), #9919 (`parse_env_value` doesn't strip unit
      suffixes like "600s" — real, but touches a shared low-level parser used by every config key,
      needs care), #9938 (cancellation doesn't promptly stop in-flight tool execution), #9978 (ACP
      NewSession error passthrough lacks auth-failure detection), #10035 (`catalog_provider_id`
      persisted but never consumed), #10055 (CI schema-check jobs run serialized, not parallel),
      #10063 (race in `main.ts`'s `releaseWindowGoosedLease`), #10068 (sidebar session row missing
      features the full History view has), #10075 (O(n²) `bat::PrettyPrinter` reconstruction in CLI
      output), #10193 (OpenRouter doesn't override `skip_canonical_filtering()`), #10228 (Azure
      `is_v1_endpoint` doesn't gate the completions-prefix), #10251 (sidebar missing an existing
      delete action), #10256 (`pctx_codegen` unbounded recursion risk).
    - PRs (16): #9289 (Electron deep-link handler ignores its own `window` argument, duplicate
      windows), #9767 (system prompt mutated every turn via `load_subdirectory_hints`, invalidating
      the KV-cache prefix), #9979 (no way to remove an image from a message before resubmitting),
      #10007 (`InferenceMetadata` built conditionally, `None` when `resolved_model` fetch fails),
      #10021 (accessibility gaps — plain `onClick` divs instead of buttons/radio inputs in provider
      picker components), #10028 (hermit activation breaks under fish shell), #10041 (no
      `canonical_provider_id()` method; custom-provider canonical resolution incomplete), #10089
      (compaction check is once-per-turn only, can be skipped mid-turn during long tool-call loops),
      #10096 (auto-compaction re-appends the latest user message unbounded after summarization),
      #10116 (`rcgen`'s `aws_lc_rs` feature enabled unconditionally instead of gated behind
      `rustls-tls`), #10127 (bundled default apps like clock/chat have no protection against
      `delete_app` — a real gap surfaced while re-checking this session's own `validate_app_name`
      fix), #10148 (CI has no matrix job validating both TLS backend feature configurations),
      #10209 (`compose_moim`'s `<current-time>` has no UTC offset), #10258 (empty-string
      `finish_reason` treated as terminal in the OpenAI-compatible streaming decoder), #10269 (stale
      NVIDIA fallback model list plus synchronous keyring reads blocking the model picker), #10270
      (Kimi Code OAuth login not recognized as "configured").
    - The 9 UNCERTAIN issues (#9153, #9411, #9598, #9702, #9750, #9921, #9926, #9958, #10110) and
      75 NEW_CAPABILITY items are not itemized here — UNCERTAIN genuinely needs live repro/access
      this pass didn't have, and NEW_CAPABILITY follows the same "defer, don't silently build" rule
      as every other new-capability verdict in this doc. Raw data for all 208 items (full detail
      text, not just the number) is in `scratchpad/graveyard-data-v2/unclear_pass/{issues,prs}_all_verified.json`
      for whoever picks up the next batch.
  - Remaining un-triaged pool, unchanged by this pass: **6,532 closed issues/PRs with real
    engagement** — the next, much larger step if this triage continues.
- **First cut into the 6,532-item closed-with-real-engagement pool, top-engagement tier resolved
  (this stretch)**: the full pool is too large for one exhaustive deep-read pass (~435 chunks at
  the ~20-items-per-chunk rate used elsewhere in this doc), so it was tiered by engagement rather
  than sampled arbitrarily. Re-derived the pool fresh from the cached
  `scratchpad/graveyard-data-v2/{closed_issues,closed_prs}.jsonl` (2,269 closed issues + 7,408
  closed PRs, each carrying `comments`/`reactions`), filtered to real engagement and not already
  cited → 6,520 items (matches the previously-reported 6,532 within normal re-derivation drift).
  Scored every item as `comments*2 + reactions` and cut at `score >= 20` — a mechanical,
  reproducible threshold, not a hand-picked sample — yielding **279 items (149 issues + 130 PRs)**,
  split into 14 chunks of 20 for parallel deep-read triage (same 5-way taxonomy, same discipline:
  read the actual issue/PR body + comments + current fork code before verdicting, never trust a
  chunk's self-reported tally, mechanically re-aggregate from the JSON files instead). One chunk's
  agent went off-script mid-run (inspected broader session/task state instead of doing its
  assigned triage, never wrote its output file) and was relaunched cleanly from scratch.
  - Mechanically aggregated (279/279, zero duplicates): **181 ALREADY_COVERED, 64 NOT_APPLICABLE,
    18 NEW_CAPABILITY, 12 BUG_FIX_CANDIDATE, 4 UNCERTAIN.**
  - Of the 12 candidates, 3 were overturned on independent re-verification before deciding what to
    port — the same "second, deeper look" discipline that caught false positives earlier in this
    doc's history:
    - **#3821** (Windows CLI-provider launch failure) — the triage agent's claim of "no
      PATHEXT/shim resolution" was wrong: `gemini_cli.rs`'s constructor already calls
      `SearchPaths::builder().with_npm().resolve(&command)?`, which resolves through the `which`
      crate (honors `PATHEXT` on Windows) — `self.command` is already a fully-resolved path by the
      time `Command::new` runs it. Corrected to ALREADY_COVERED.
    - **#2787** (GitHub Copilot device-auth failure) — the triage agent proposed adding
      backoff to the 3-attempt retry loop, but the issue thread's own root cause was a missing
      keyring, and `crates/goose/src/config/base.rs`'s `is_keyring_availability_error`/
      `handle_keyring_fallback_error` already auto-detects exactly that and falls back to file
      storage — the actual reported failure mode is already fixed elsewhere; a bare backoff
      wouldn't address what was reported. Corrected to ALREADY_COVERED.
    - **#6284** (anonymous CLI-added MCP extensions get opaque names) — the triage agent
      checked only `extension_manager.rs` in isolation; `crates/goose-cli/src/session/mod.rs`
      already derives a readable, deterministic name from the URL host/path (`mcp_kiwi_com`,
      `localhost_8080_api`, confirmed via its own `test_case`-driven unit tests) before
      `ExtensionConfig::key()` is ever called — `sanitized_name` is never actually empty in
      practice. A different, arguably more deterministic mechanism than upstream's
      server-info-plus-random-suffix approach already solves this. Corrected to ALREADY_COVERED.
  - **1 of the remaining 9 ported this stretch**: **PR #9832** — weak/cheap models occasionally
    emit tool-call arguments that are valid JSON but not an object (bare array/string/number); the
    fork's `openai.rs` (streaming + non-streaming) and `databricks.rs` decoders passed the value
    straight into rmcp's `object()`, whose `debug_assert!(value.is_object())` panics in debug
    builds and silently empties the arguments in release — reproduced the exact panic via mutation
    testing (revert-the-guard, confirm the identical `rmcp-1.8.0/src/model.rs:37` assertion fires,
    restore). Guarded both decoder sites with an `is_object()` check plus an `INVALID_PARAMS` tool
    error, added a shared `describe_json_value` helper, and added regression tests at each site.
    Also restored `goosed.ts`'s healthcheck startup budget from a regressed 30s back to the merged
    120s (**PR #5349**) — a one-line data fix confirmed against the merged upstream PR body.
  - **8 remaining candidates catalogued, not ported this stretch** (real, but each needs more than
    a rubber-stamp change): **#1267** (Bedrock's 5-document-per-request API cap — the fix requires
    threading a shared document counter through 4 function signatures and updating ~15 existing
    test call sites in `formats/bedrock.rs`, not a one-line change; the issue reporter's own
    suggested approach — always use plain text instead of Bedrock's document content type — is the
    right shape but needs dedicated scoping), **#5911** (Windows Gemini CLI multi-line prompt
    argument passed through a `.cmd`-wrapped npm shim, where cmd.exe's own line-based batch parsing
    could corrupt an embedded newline — plausible but unverifiable without a live Windows repro,
    and the safer fix (stdin instead of an argv element) needs confirming the actual `gemini` CLI
    supports it), **#7070** (`claude_code.rs`'s `OnceCell<Arc<Mutex<CliProcess>>>` permanently
    poisons after the CLI process dies; upstream's own PR for this was closed unmerged because
    reviewers rejected the naive blind-respawn fix — the underlying gap is real but needs the
    process-group/signal-isolation redesign reviewers actually asked for, not a resurrection of the
    rejected diff), **#4468** (Terminal.app markdown highlighting gated too strictly by
    `is_terminal()` — the issue's own closing comment says this was fixed on upstream `main`, but
    the actual upstream diff wasn't tracked down this stretch to port precisely), **#2825**
    (trailing-newline normalization in `edit.rs`'s `file_write_with_cwd` — real, but an existing
    test (`test_file_write_new`) explicitly asserts the current no-normalization behavior, so this
    needs a judgment call on whether verbatim-write is deliberate design before changing it, not
    just flipping a test's expectation), **#3697** (Windows backslash paths losing characters —
    the triage agent couldn't pin down a concrete code location, only the maintainer's own proposed
    remediation ideas; not implementation-ready), **#2821** (`computercontroller/mod.rs` still has
    the verbose misleading-parameter-error anti-pattern already fixed elsewhere via typed
    `Parameters<...>` structs — mechanical but spans 9+ call sites in one file, a bigger lift than
    the ones ported this stretch).
  - The 4 UNCERTAIN and 18 NEW_CAPABILITY verdicts from this tier, plus the full 279-item raw
    triage output, are in `scratchpad/graveyard-data-v2/closed_pass/{chunk,result}_*.json` and
    `all_merged.json` for whoever picks up the next tier.
  - **Citation count after this batch: 112 issues/PRs + 38 branches = 150** (mechanically
    regenerated, up from 148).
- **Second engagement tier of the closed pool resolved (this stretch)**: continued down the same
  score ladder, this time `score in [14, 20)` (the band immediately below the first tier, so no
  overlap) — **306 items (151 issues + 155 PRs)**, split into 16 chunks of ~20 for the same
  deep-read triage discipline. Two chunk agents hit context-thrashing failures mid-run trying to
  pull full diffs on unusually large PRs (one was an auto-generated release-train PR); one had
  actually already written its result file before erroring (used it as-is after confirming it was
  complete and well-formed), the other was relaunched with explicit guidance to check PR size
  before pulling a full diff, which resolved cleanly.
  - Mechanically aggregated (306/306, zero duplicates): **190 ALREADY_COVERED, 74 NOT_APPLICABLE,
    19 NEW_CAPABILITY, 14 BUG_FIX_CANDIDATE, 9 UNCERTAIN.**
  - Independently re-verified all 14 candidates before deciding what to port. One, **#5336**, is
    the same trailing-newline gap in `edit.rs` already catalogued as **#2825** from the first tier
    — same deferred rationale (an existing test asserts the current no-normalization behavior).
  - **5 ported this stretch**, each independently re-verified and mutation-tested:
    - **PR #10000** — the PR's own diagnosed root cause (an 8KiB SSE line-buffering limit) was
      refuted (the actual `LinesCodec` has no such limit), but the same investigation surfaced a
      real, different gap: a `response.completed` stream event missing an `id` field failed
      `serde_json::from_value` outright and was silently dropped (losing final usage/output)
      instead of degrading gracefully. Made `id` optional via `#[serde(default)]`.
    - **#2150** — `to_bedrock_tool` never stripped top-level `oneOf`/`allOf`/`anyOf` from a tool's
      `input_schema`, which Bedrock's Converse API rejects outright, breaking any Claude-on-Bedrock
      session using an MCP tool with a union-typed schema.
    - **#2885** — stdio extension `args` had no tilde-expansion before being passed to
      `Command::args()`, so `--prefix ~/my-server`-style args (common in copy-pasted MCP setup
      commands) failed silently. First independently traced the issue's exact reported symptom
      (literal embedded quote characters in an arg) to confirm it was already fixed elsewhere — the
      CLI's `split_command_args` already strips quotes correctly (verified via its own existing
      tests) — narrowing the real remaining gap to tilde-expansion alone before fixing it.
    - **PR #4626** — the desktop CSP blocked MCP-UI webviews from loading external stylesheets and
      non-`self` media; relaxed `style-src`/`media-src` (plus explicit `style-src-elem`/
      `media-src-elem`) to match the merged upstream PR.
    - **#4540** — cursor-agent's CLI stderr was piped but never read, so failures only reported a
      bare exit code; now drains stderr concurrently with stdout and surfaces it in the error.
  - 3 verdicts were corrected during independent re-verification for other candidates before
    settling on the port list — not overturned to ALREADY_COVERED this time, but narrowed in scope
    (e.g. #7620's fix target turned out to be `config::base::get_secrets`'s `?`-short-circuit
    behavior specifically, not a broader provider rewrite) during investigation; those remain
    catalogued below rather than ported, since the narrower fix still touches a shared function
    used by other config keys and deserves its own dedicated verification pass.
  - **9 remaining candidates catalogued, not ported this stretch**: **#9583** (image content isn't
    forwarded to delegated subagents — confirmed via zero `image`/`Image` matches in
    `subagent_handler.rs`/`summon.rs` — a real but feature-sized gap, not a one-line fix),
    **#8780** (`CustomProviderForm.tsx` has no UI field for overriding a custom provider's context
    limit, even though the underlying `Model` type already supports `context_limit` — needs a new
    form field and wiring, not just a backend change), **#5812** (the `summon` extension is always
    enabled regardless of whether any other extension exists for a delegate to usefully act on —
    the actual upstream fix landed in a separate PR, #5825, not fetched or verified this stretch),
    **#7427** (Azure AI Foundry's `/models`-less v1/deployment-scoped endpoints have no static-list
    fallback when the generic `fetch_models_json()` 404s — spans `openai_compatible.rs`,
    `azure.rs`, and a server route, more than a single-function fix), **#7620** (`OPENAI_CUSTOM_HEADERS`
    is silently dropped whenever `OPENAI_API_KEY` is unset, because `config::base::get_secrets`'s
    `?`-based primary-key lookup short-circuits before the secondary key is ever fetched — real, but
    `get_secrets` is a shared function and needs checking every other caller before changing its
    semantics), **#4843** (recipe YAML with multi-line values still requires manually applying a
    Jinja `indent` filter — a UI auto-indent feature, not a narrow fix), **#4600** (OAuth
    401/reconnect detection only fires on initial connection, not on a token expiring mid-session —
    a state-machine change in the MCP client's reconnect path, higher regression risk), **#1095**
    (429 rate-limit errors carry a `retry_delay` hint that nothing in the core reply loop actually
    consumes for automatic backoff — a real robustness gap, but changing the core reply loop's
    retry behavior needs its own careful design and testing), **#5336** (duplicate of #2825, see
    above).
  - Full 306-item raw output: `scratchpad/graveyard-data-v2/closed_pass_tier2/{chunk,result}_*.json`
    and `all_merged_tier2.json`.
  - **Citation count after this batch: 117 issues/PRs + 38 branches = 155** (mechanically
    regenerated, up from 150). **Remaining pool, still the dominant untouched figure: ~5,935
    closed issues/PRs with real engagement below the score-14 cutoff used across both tiers so
    far** — the next step, if continued, is either lowering the score cutoff further (e.g.
    `score >= 10` covers ~1,047 items total, meaning ~768 more beyond what's been triaged) or
    accepting a coarser, cheaper triage pass for the long tail, the way the branch graveyard's small
    tier was first triaged by log/file-list alone before any deep read.
- **Upstream `main` has moved ~125 commits past this fork's last sync point** (re-measured fresh;
  was ~119 at an earlier count). This is
  informational, not a backlog: goose-plus has diverged too far architecturally (native Rust TUI,
  A2A/NATS, standalone in-agent MCP servers, provider-catalog rewrites, and more) for a bulk merge
  of upstream into `goose-plus` to ever make sense, and **that is never done here**. If anything in
  those 125 commits is worth having, it goes through the same graveyard-mining triage as everything
  else — evaluated and ported individually, never bulk-imported.

This is a living, incremental effort, not a finished sweep — treat any specific "N items fixed"
count as a lower bound on what the graveyard actually contains, not a claim that the graveyard is
empty. Never treat the full backlog as a completable-in-one-session goal; triage continues at a
sustainable pace gated on verifiability in the working environment.

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

### Release engineering, CI hardening & dependency maintenance (own work, no upstream ticket)

Infrastructure/maintenance work that doesn't fit the user-facing bug/feature matrices above, but is real landed engineering — cited here for the same completeness the rest of this doc holds itself to.

| Area | Source | What it does |
|---|---|---|
| Release CI | `40492e5bc` | `release-plus-preflight.sh` (`just release-plus-preflight`) catches bundle path/productName mismatches before tagging; updater dialog text aligned to goose-plus naming |
| Release CI/Linux | `8373b7b18` | Split the desktop-linux bundle into two runner legs — `rpm` only builds on `ubuntu-22.04` (24.04+ hits an `rpmdb.sqlite` EPERM), `glslc`/shaderc for the Vulkan leg only exists on `ubuntu-24.04+` — so standard (22.04) ships deb+rpm+AppImage and Vulkan (24.04) ships deb+AppImage+zip and skips rpm |
| Release CI/Linux | `cdd58dc62` | Root-caused the same `rpmdb.sqlite` EPERM as an apparmor unprivileged-userns restriction present on 24.04+ too, not just newer previews — moved both legs to `ubuntu-22.04`; made the Flatpak runtime install best-effort (`\|\| echo note`) so a Flathub outage no longer fails deb/rpm/AppImage; added a `libasound2t64`→`libasound2` fallback for pre-t64-transition runners |
| Release CI/Linux | `31293c31b` | Reverted an `ubuntu-26.04` preview-runner experiment for the desktop bundle after it broke rpm the same way — back to the proven 22.04/24.04 LTS pair (CLI-only build stays on 26.04, which builds fine and makes no rpm) |
| Release CI/Windows | `cba96f624`, `7f6f404d7` | `validate_entry_path`/`etcetera::home_dir` are only called from the non-Windows tar-extraction path (Windows uses `enclosed_name()` for zip-slip protection); cfg-gated both to match their actual callers instead of leaving them as Windows dead code |
| CI/lint | `583ed6e1a` | Removed the Windows/macOS `-D warnings` escape hatches by properly `#[cfg]`-gating every remaining platform-conditional item (an unused import inside a Unix-only socket helper, a merge-paths helper only called off-Windows, a field only consumed on non-Windows) instead of relaxing the lint — `unused_imports`/`unused_must_use` stay `deny` and every OS preflight runs `-D warnings` with no exceptions |
| Supply chain/CI | `3c9dc7ccb` | `deny.toml` + `cargo-deny.yml` (yanked/unknown-registry/license-allowlist gates), `osv-scanner.yml` (cross-ecosystem CVE scan), `pnpm-audit.yml` (UI dep CVEs + frozen-lockfile integrity), `dependabot-auto-merge.yml` (patch auto-merge; minor only for non-production deps) — all guarded to `repository == 88plug/goose-plus` so they no-op on forks of this fork |
| Dependencies/Desktop | `ea660d970` | Held-major desktop upgrades taken to latest stable: `@mcp-ui/client` 6→7 (full rewrite onto the v7 `AppRenderer`/`AppBridge` API), TypeScript 6, `eslint-plugin-react-hooks` 7, Electron 42, Vite 8, plus a round of smaller majors (knip, uuid, lucide, `@types/node`) |
| Dependencies/Local inference | `ff8ef8484` | `whisper.rs` audio backend migrated from its prior decode/resample stack to `symphonia` 0.6 + `rubato` 3.0 |
| Dependencies/ask-ai-bot | `34d7b1102` | The bundled `services/ask-ai-bot` helper migrated to AI SDK v7 (`ai` 6→7, `@ai-sdk/anthropic` 3→4), applying the v7 renames (`stepCountIs`→`isStepCount`, `streamText` `system`→`instructions`, `result.fullStream`→`result.stream`) |

## Build

```bash
source bin/activate-hermit
cargo build --release          # CLI + server
cd ui/desktop && pnpm install  # desktop
```
