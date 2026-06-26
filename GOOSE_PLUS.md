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
| **Event/Control bus (NATS)** | — | Opt-in publish firehose **and** bidirectional drive loop (fleet envelope: seq + instance + drop counter) |
| **Standalone MCP servers** | builtin tools are in-agent only | `goose-plus mcp developer` serves the developer tools over stdio for any MCP client |
| **Dependencies** | Mixed freshness | Kept current via the `use-latest-version` pipeline; lockfiles consistent |
| **Lint/format gate** | Default-feature clippy | Every crate inherits workspace lints; prettier wired into the gate; feature-gated code covered |
| **Release/CI** | Upstream signed releases | Self-maintaining `plus-v*` releases + keyless build-provenance, upstream `main` mirror, one-command upstream ports |
| **Graveyard** | Open by definition | ~70 closed/rejected issues & PRs implemented and verified |

Full diff: **[compare main…goose-plus](https://github.com/88plug/goose-plus/compare/main...goose-plus)**.

---

## Headline additions

- **API-driven model marriage** — on auth, providers populate their model list *and* per-model parameters (context window, tool/vision/reasoning support, token cost) from the provider's own `/v1/models` (Nebius `?verbose=true`; xAI `context_length`/`context_window`). No more phantom models or stale context limits.
- **xAI SuperGrok native provider** — OAuth (loopback + device-code), **subscription** (`cli-chat-proxy.grok.com`) and **API-key** (`api.x.ai`) paths kept distinct, live-correct Grok context windows, and **working thinking-effort** (`reasoning_effort` actually transmitted, with graceful fallback).
- **Nebius Token Factory provider** — OpenAI-compatible, dynamic discovery with rich per-model metadata.
- **A2A (Agent2Agent)** — goose speaks A2A both ways (Agent Card + JSON-RPC/REST/WS server with bearer auth, streaming, cancel; and an A2A client). See [`docs/a2a.md`](docs/a2a.md), [`AUTH.md`](AUTH.md).
- **Native NATS bus** — opt-in publish + bidirectional drive. See [`docs/nats.md`](docs/nats.md).
- **Standalone MCP** — `goose-plus mcp developer` exposes the builtin developer tools to any MCP host.

---

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
| `reasoning_effort` actually transmitted to provider | ◑ | ✓ |
| Dependencies kept on latest + consistent lockfiles | ◑ | ✓ |
| Workspace-wide lint gate (all crates + prettier + feature code) | ◑ | ✓ |
| Self-maintaining release + build provenance + upstream mirror | – | ✓ |
| Portable Windows `.zip` / Linux `.AppImage` / Docker web UI | ◑ | ✓ |

---

## Bug matrix (fixed from the graveyard)

Real upstream issues/PRs that were closed-without-fix, rejected, or never got to — implemented and tested here. Each row is also a [port candidate](#contribute-by-porting).

| Area | Upstream # | What goose-plus fixes |
|---|---|---|
| Providers/reasoning | #9397, #9675 | DeepSeek / openai-compatible `reasoning_content` no longer dropped |
| Providers/streaming | #8503 | Final text segment after tool calls no longer lost |
| Providers | #8321 | Unmapped `/v1/models` surfaced instead of silently dropped |
| Providers | #9124, #7987 | Env-configurable retry + request timeout; sane 429/Retry-After |
| Providers | #9489 | Ollama keep-alive keeps the model warm |
| Providers | #9476 | Databricks serving-endpoints pagination — all models surface |
| Providers | #8512 | Context-limit floor stops Context-Length-Exceeded on fresh install |
| Providers | #2564, #9333 | Codex `v1/responses` handling; DeepSeek V4 |
| Providers | #6293, #1863 | Gemini empty-reply-after-tool-use; fetch-400 |
| Providers | #6573, #8495 | devstral context limits; `OPENAI_CUSTOM_HEADERS` with commas |
| MCP | #7063 | MCP auto-reconnect on dropped transport |
| Agent | #9082, #9640 | No-progress loop guard stops runaway turns |
| Agent | #8777 | Unix login-shell process-group isolation |
| Agent | #9398 | `detect_image_path` handles shell escapes |
| Agent | #3085 | `GOOSE.md` recognized as a project context file |
| CLI | #8059 | Bracketed paste — pasting no longer auto-executes |
| Server | #9358 | `GET /sessions/{id}` message pagination |
| Desktop | #9342, #8997 | Chat history not loading; reply-render delay under reduced-motion |

*(Plus fixes proven in this fork without an upstream ticket: `GOOSE_A2A_ENABLE=1` / `GOOSE_NATS_DRIVE=1` truthy env flags, a guard against silent declarative-provider drop, and the SuperGrok 426/403 endpoint + credential fixes.)*

## Community-feature matrix (requests delivered)

Features the community asked for — requested, upvoted, or stalled in a PR — that goose-plus ships.

| Area | Upstream # | Feature |
|---|---|---|
| Agent | #7808 | Recipe-level tool blocking (denylist) |
| Agent | #8183 | Graceful unknown-tool calls with suggestions |
| Desktop | #6926 | Folders for organizing chats |
| Desktop | #9080 | Model favorites |
| Desktop | #9391 | Close-to-tray |
| Desktop | #7554 | Per-window pinned certificates |
| Desktop | #9143 | Same-window link navigation |
| Desktop | #1505 | In-chat find filter |
| Desktop | #8288 | Accessibility font scaling |
| Desktop | #9390 | Syntax-highlighted diffs |
| Desktop | #7965 | Delete apps |
| Desktop | #8140 | Configurable sidebar session limit |
| Desktop | #6472 | MCP Apps `ui/update-model-context` |

> The `#` numbers reference the upstream [aaif-goose/goose](https://github.com/aaif-goose/goose) issue/PR tracker. Where a closed/unmerged PR existed, goose-plus reused its diff as a starting point — making those rows the cheapest to port back.

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

### Pick → port → PR (one example)

1. **Pick** an item from a matrix above — say the Nebius provider, or bug #8503 (lost final text segment).
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

Good first ports: single-file provider fixes (#8495, #6573), self-contained features (#1505 find filter, #9080 favorites), or a whole new provider (Nebius).

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
