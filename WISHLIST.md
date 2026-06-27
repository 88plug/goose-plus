# WISHLIST.md — Context Cortex PRD

**Product:** goose-plus context management evolution  
**Status:** Draft PRD (research + repo forensics, June 2026)  
**Baseline:** `9fd61b3ab` (`plus-v1.39.22`)  
**Authors:** Research synthesis (SearXNG + Edgar Morin + local repomix forensics)

---

## Executive Summary

Long-running coding-agent sessions degrade before they overflow. Every major TUI (OpenCode, Crush, Codex, Claude Code) optimizes **crash prevention at ~80–95% context**, not **coherence preservation at ~40%**. goose-plus is already the most advanced local implementation — dual visibility metadata, incremental tool-pair eviction, MOIM early warning — but still relies on lossy prose summaries and positional heuristics.

**Context Cortex** is the proposed evolution: TUI-agnostic middleware that treats chat as disposable L1 cache, git/filesystem as canonical memory, and evicts **typed episode surrogates** via a dependency graph — not narrative compaction. This document is the wishlist / PRD for building it on goose-plus's existing `context_mgmt/` foundation.

---

## Problem Statement

### Symptoms

- Agents lose causal chains after compaction ("you told me X three pages ago" when X was summarized away).
- Quality collapses at 40–60% context utilization while harnesses wait until 80–95% to act ([Chroma context rot](https://research.trychroma.com/context-rot), [OpenCode #10016](https://github.com/anomalyco/opencode/issues/10016)).
- Tool outputs dominate token budget; blunt truncation or middle-drop destroys recoverability.
- Prompt-cache economics punish arbitrary middle-edits ([Don't Break the Cache](https://arxiv.org/html/2601.06007v2)).

### Root Cause

Harnesses optimize `messages[]` length. The real object is an **episode DAG**: user decisions → tool calls → file mutations → git state. Full compaction **flattens a graph into prose**, destroying structure ([CWL paper](https://arxiv.org/html/2606.11213v1)).

### Opportunity

goose-plus already has the right primitive: **dual visibility** (`agent_visible` / `user_visible`) plus incremental tool-pair replacement. Extend that into typed surrogates, graph-aware eviction, and agent recoverability — then port as middleware for other harnesses.

---

## Goals

| # | Goal |
|---|------|
| G1 | Preserve causal coherence through 4+ hour coding sessions without quality cliff at 40–50% context |
| G2 | Evict context incrementally with **typed surrogates**, not lossy prose summaries |
| G3 | Guarantee **recoverability**: every eviction has a re-fetch path or pinned surrogate |
| G4 | Maintain **prefix stability** for prompt-cache economics |
| G5 | Trigger eviction at **~40%** context, not 80–95% |
| G6 | Ship a **Context Inspector** TUI panel for pin/drop/rehydrate |
| G7 | Keep goose-plus as seed codebase; design for eventual TUI-agnostic extraction |

## Non-Goals

- KV-cache eviction research (H₂O, SAGE-KV, StreamingLLM) — wrong layer for API-based agents
- Full MemGPT/Letta-style virtual memory OS — filesystem + git is the archive
- Multi-agent context splitting — telephone problem; single agent + cold-zone eviction
- Replacing goose-plus's provider/A2A/NATS work — context cortex is orthogonal
- Upstream Block goose parity tracking in this doc (optional follow-up)

---

## Research Background

### Selective replacement is real — at multiple layers

| Layer | Examples | Relevance to goose-plus |
|---|---|---|
| Inference KV eviction | H₂O, SAGE-KV, StreamingLLM, FoX | Low — API clients never touch KV |
| Application/API editing | Anthropic context editing, MemGPT, A-Mem | High — patterns for selective removal |
| Structural access | Landmark Attention, CacheBlend | Medium — informs surrogate/pointer design |
| Agent memory surveys | [Memory for Autonomous LLM Agents](https://arxiv.org/html/2603.07670v1) | High — formalizes write/manage/read loop |

### What prior attempts missed

| Attempt | Missed |
|---|---|
| Compaction (all TUIs) | Destroys causal structure; lossy prose |
| Crush middle-drop ([#2240](https://github.com/charmbracelet/crush/issues/2240)) | No semantics, no recovery |
| Codex 256-line truncation ([#6426](https://github.com/openai/codex/issues/6426)) | Arbitrary cuts |
| DCP plugin | Heuristics, no DAG, not core |
| pi-context-prune | Good recoverability (`context_tree_query`), no causal eviction policy |
| CWL / pi-cwl | Good episode graph, requires full protocol rewrite |
| KV eviction research | Wrong layer for API agents |
| MemGPT | Heavy; agent must actively cooperate |
| Observation masking alone ([JetBrains](https://arxiv.org/html/2508.21433v3)) | Insufficient for 4hr sessions; excellent L0 foundation |

### Why TUIs haven't "solved it"

1. Products optimize **"don't crash"** not **"stay coherent"**
2. Selective deletion is semantically dangerous without recoverability guarantees
3. Prompt caching breaks on middle-edits without zone architecture
4. Plugins (DCP, pi-context-prune) fill the gap instead of core integration
5. Research solved inference KV — harnesses are API clients

---

## Current State: goose-plus Baseline

**Repo:** `/home/andrew/goose-plus`  
**HEAD:** `9fd61b3ab` — `fix(release-plus): pin makeLatest (plus-v1.39.22)`  
**Context core unchanged** in `a4b566f5c → 9fd61b3ab` pull (168 files elsewhere).

### Key files

| File | Role |
|---|---|
| `crates/goose/src/context_mgmt/mod.rs` | Compaction, tool-pair summarization, middle-out filter (820 lines) |
| `crates/goose/src/agents/moim.rs` | `<turn-context>` injection, compaction countdown (352 lines) |
| `crates/goose/src/agents/agent.rs` | Orchestration, async eviction per turn |
| `crates/goose/src/prompts/compaction.md` | LLM compaction prompt (9-section structured summary) |
| `crates/goose/src/session/last_message_snippet.rs` | Session list UI only — **not** agent memory |

### Tier 1 — Incremental tool-pair replacement ✅

- `maybe_summarize_tool_pairs()` — background `tokio` task per reply loop
- Batch: 10 pairs (`TOOLCALL_SUMMARIZATION_BATCH_SIZE`)
- Cutoff: `compute_tool_call_cutoff(context_limit × threshold)` — clamp(10, 500)
- On completion: originals → `with_agent_invisible()`, summary → `agent_only`
- Protects current turn: `protect_last_n` — never summarize last N tool calls
- Toggle: `GOOSE_TOOL_PAIR_SUMMARIZATION` (default **on**)

### Tier 2 — Full auto-compaction ✅

- Trigger: `check_if_compaction_needed()` at **80%** (`DEFAULT_COMPACTION_THRESHOLD = 0.8`)
- Override: `GOOSE_AUTO_COMPACT_THRESHOLD`
- Pre-compact: `filter_tool_responses()` — **middle-out** removal at 0→10→20→50→100%
- `do_compact()` → `compaction.md` → LLM prose summary
- Visibility: originals `agent_invisible`, user still sees; summary + continuation assistant injected
- Preserves latest user text message on auto-compact
- Manual: `/compact` slash command, ACP `conversation/truncate`

### Tier 3 — MOIM turn-context ✅

- Injects `<turn-context>` into latest user message each turn
- Compaction countdown after **50%** of threshold (`total_tokens / compaction_at >= 0.5`)
- Skips context limits < 32K (`MIN_CONTEXT_FOR_MOIM`)
- Agent warned early; compaction still fires at 80%

### Tier 3b — Recovery compact ✅

- On `ContextLengthExceeded`: retry compact up to 2 attempts
- `did_recovery_compact_this_iteration` — continues from last user message

### New in latest pull (extension point, not eviction)

- `AfterAgentResponse` hook in `agent.rs` — fires once per final assistant text response
- Enables external middleware to observe/capture responses
- Deleted: `CONTEXT_RECOVERY_REPORT.md` (was internal doc)

### What goose-plus got right (closest to Context Cortex)

- Dual visibility metadata — selective agent removal without deleting user history
- Incremental replacement **before** full compact
- Protects current turn
- Recovery compact on context-length errors mid-loop

### What goose-plus still misses

- Summaries are **prose**, not typed surrogates
- No **dependency graph** — middle-out is positional, not causal
- No **recoverability oracle** — agent cannot re-fetch evicted tool output
- Trigger still **80%** — past context-rot danger zone
- Tool-pair summaries are **lossy** ("A call to github was made…")

---

## Competitive Landscape

### Crush (`/home/andrew/crush`)

**Mechanism:** Summary pointer handoff only.

- `StopWhen` checks remaining tokens each step
- Small windows (<200K): trigger at **20% remaining**
- Large windows: trigger at **20K tokens remaining**
- `Summarize()` → `session.SummaryMessageID` → `getSessionMessages()` slices from summary onward
- Everything before summary **gone** from agent view (not visibility-layered)
- `disable_auto_summarize` config flag
- `compact_mode` = TUI display density, **not** context management

**Verdict:** Simplest. Lossy nuclear. No selective replacement.

### OpenCode (`/home/andrew/opencode`)

**Mechanism:** Standard 90% full summarization.

- Auto-summarize at **90%** of `(context - output)` in `chat.ts`
- `summarize.ts` — filter since last `summary: true`, LLM prose, recursive `chat()`
- No prune/compact/visibility in core session module
- [DCP plugin](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning) (3.5k stars) — external, heuristic

**Verdict:** Same paradigm as Crush. Community wants prune-before-summarize ([#14825](https://github.com/anomalyco/opencode/issues/14825)) — not in core.

### Comparative Matrix

| Capability | goose-plus | Crush | OpenCode core | DCP plugin |
|---|---|---|---|---|
| Selective tool eviction | ✅ tool-pair | ❌ | ❌ | ✅ heuristic |
| Dual visibility (agent/user) | ✅ | ❌ | ❌ | partial |
| Incremental (before full compact) | ✅ | ❌ | ❌ | ✅ |
| Full compaction fallback | ✅ 80% | ✅ ~80–90% | ✅ 90% | ✅ |
| Typed surrogates | ❌ | ❌ | ❌ | ❌ |
| Dependency graph | ❌ | ❌ | ❌ | ❌ |
| Agent recoverability | ❌ | ❌ | ❌ | partial (TUI) |
| Trigger timing | 80% | 80–90% | 90% | configurable |
| Context Inspector TUI | ❌ | ❌ | ❌ | ✅ |

**Conclusion:** goose-plus on this machine is the **seed codebase**. Crush/OpenCode core do not have incremental eviction primitives.

---

## Vision: Context Cortex

Not one algorithm. A **TUI-agnostic middleware OS** between any harness (OpenCode/Crush/Codex/Pi) and any provider API.

### Three invariants (non-negotiable)

| Invariant | Meaning |
|---|---|
| **CANONICALITY** | Truth lives in git/filesystem. Chat is disposable working set. |
| **RECOVERABILITY** | Nothing removed without a surrogate OR re-fetch path (file:line, git sha, pointer URI, `context_tree_query`). |
| **PREFIX STABILITY** | System prompt + pinned decisions = immutable hot prefix. Eviction only in cold zone. Cache-safe by construction. |

### Four layers (tiered, not either/or)

```
L0  Observation Masking     — hide stale tool outputs (zero LLM cost)
    [JetBrains: halves cost, matches summarization]

L1  Typed Surrogates        — replace episodes with 10–50 token cards, not 500-token summaries
    DecisionCard | FileAnchor | ToolDigest | GitSnapshot | ConstraintCard

L2  Dependency Eviction     — LLM-free policy on episode DAG
    [CWL: shed persisted actions, keep active reasoning + user turns]

L3  Narrative Compaction    — last resort only for irrecoverable prose
    [current Codex/Claude/OpenCode approach — demoted, not deleted]
```

**Trigger at ~40% context**, not 95%. Quality preservation, not crash prevention.

### Request zone architecture

```
[HOT PREFIX — cached, never mutated]
  system + AGENTS.md + pinned DecisionCards

[WARM ZONE — append-only during session]
  recent turns + active file context

[COLD ZONE — eviction target]
  completed tool episodes with persisted effects
```

### One sentence

**Stop managing tokens. Start managing episodes with recoverability guarantees — chat is cache, git is memory, eviction is graph surgery not summarization, and you trigger at 40% not 95%.**

---

## Feature Wishlist

### F1 — Recoverability Oracle

**Description:** Before evicting any turn, answer deterministically: *Can this episode be reconstructed from environment state alone?*

| Episode type | Recoverable? | Action |
|---|---|---|
| `read_file` after file edited | Yes — re-read | Replace with `FileAnchor{path, mtime, sha}` |
| `grep` / search output | Yes — re-run | Replace with `ToolDigest{cmd, hit_count}` |
| User decision ("use JWT not sessions") | **No** | Pin as `DecisionCard` — never evict |
| Failed approach (tried X, broke Y) | **No** | Pin as `ConstraintCard` |
| `write_file` / `git commit` | Yes — diff exists | Replace with `GitSnapshot{sha, files}` |

**Acceptance criteria:**
- [ ] Every eviction path calls oracle before mutating visibility
- [ ] Non-recoverable episodes auto-pinned to hot prefix
- [ ] Recoverable episodes always produce typed surrogate with `recover_cmd`
- [ ] Agent can invoke recover_cmd via existing tool surface (read_file, bash, etc.)

---

### F2 — Typed Surrogates (L1)

**Description:** Replace evicted episodes with structured 10–50 token cards, not prose summaries.

**Surrogate types:**

```rust
// Illustrative — actual schema TBD
DecisionCard   { id, text, turn, pinned: bool }
FileAnchor     { path, line_range, sha, mtime }
ToolDigest     { tool, args_hash, hit_count, recover_cmd }
GitSnapshot    { sha, files[], message }
ConstraintCard { what_failed, why, turn }
```

**Example output in context:**

```
[DECISION] auth=JWT, refresh=7d, decided@turn-12
[ANCHOR] src/auth.ts:47-120 (sha:a3f2c1)
[DIGEST] grep "middleware" → 3 hits in 2 files (re-run: grep -r middleware src/)
```

**Acceptance criteria:**
- [ ] `summarize_tool_call()` emits `ToolDigest` surrogate, not free-form prose
- [ ] Surrogates serialize to compact single-line format (<50 tokens typical)
- [ ] Surrogates stored in `MessageMetadata` extension field (backward compatible)
- [ ] Unit tests: surrogate round-trip preserves recover_cmd
- [ ] Eliminates compression-induced hallucination vs prose rewrite

---

### F3 — Episode DAG (L2)

**Description:** Model each tool call + result as a typed node with edges.

**Edge types:**
- `depends_on` → prior reads/decisions
- `mutates` → files changed
- `supersedes` → obsolete approaches

**Eviction policy:** Drop leaf nodes whose `mutates` are reflected in current git tree. Keep active reasoning chains and unpinned user turns.

**Acceptance criteria:**
- [ ] DAG built incrementally per tool pair (no full-session replay)
- [ ] Eviction policy is **LLM-free** (deterministic graph walk)
- [ ] Middle-out heuristic replaced by graph-leaf shedding
- [ ] Pinned `DecisionCard` / `ConstraintCard` nodes never evicted
- [ ] Integration test: 4hr simulated session retains decision chain after 60% eviction

---

### F4 — Observation Masking (L0)

**Description:** Hide stale tool outputs from agent view with zero LLM cost before any summarization.

**Rules (initial):**
- Mask `read_file` output when file mtime > read timestamp
- Mask completed grep/search when newer edits touched matched files
- Mask duplicate identical tool calls within session

**Acceptance criteria:**
- [ ] L0 runs before L1 on every turn
- [ ] Masked messages use existing `agent_invisible` metadata
- [ ] Token savings logged per session (target: ≥30% on read-heavy sessions)
- [ ] No LLM calls in L0 path

---

### F5 — Cold-Zone Architecture + Prefix Stability

**Description:** Structural zones in conversation assembly; eviction never mutates hot prefix.

**Acceptance criteria:**
- [ ] Hot prefix = system + project instructions + pinned cards (immutable per session)
- [ ] `agent.rs` prompt assembly respects zone ordering
- [ ] Eviction operations only touch cold zone messages
- [ ] Anthropic/OpenAI cache breakpoints align with zone boundaries
- [ ] Config: `GOOSE_CONTEXT_HOT_PREFIX_PIN` for user-pinned turns

---

### F6 — 40% Trigger Threshold

**Description:** Begin L0→L1→L2 eviction at ~40% context utilization; reserve L3 compaction for >85% or manual `/compact`.

**Acceptance criteria:**
- [ ] New default: `DEFAULT_EVICTION_THRESHOLD = 0.4` (separate from L3 compact threshold)
- [ ] `GOOSE_AUTO_EVICTION_THRESHOLD` env override
- [ ] MOIM countdown starts at 25% (not 50% of 80%)
- [ ] L3 full compact remains at 80% as last resort
- [ ] E2E test: session quality benchmark at 50% utilization vs baseline

---

### F7 — GREP-don't-hoard Primitive

**Description:** Treat every `read_file` as cache entry with TTL, not permanent context.

**Default TTL:** Until next edit to that file (via git status / mtime watch).

**Acceptance criteria:**
- [ ] `read_file` episodes auto-eligible for L0 mask after TTL
- [ ] Surrogate `FileAnchor` always emitted on eviction
- [ ] Agent prompted (via MOIM) to re-read on demand, not hoard
- [ ] Configurable TTL override per session

---

### F8 — Context Inspector TUI Panel

**Description:** Ship DCP-style inspector as first-class goose-plus UI.

**Capabilities:**
- View hot / warm / cold zone contents
- Pin / unpin any turn
- Preview eviction impact before applying
- One-key "rehydrate" any surrogate (re-run recover_cmd)
- Show token budget per zone

**Acceptance criteria:**
- [ ] Desktop UI panel in session view
- [ ] CLI/TUI equivalent for headless mode
- [ ] Pin action promotes message to hot prefix
- [ ] Rehydrate restores full tool output to warm zone temporarily
- [ ] Works with existing `MessageMetadata` visibility layers

---

### F9 — Structured Session Handoff (L3 upgrade)

**Description:** When L3 compaction must fire, output machine-readable handoff (YAML/JSON), not prose-only.

```yaml
decisions: [{id, text, turn}]
constraints: [{what_failed, why}]
active_files: [{path, sha, role}]
open_tasks: [{id, status, blocked_by}]
evicted_surrogates: [{type, pointer, recover_cmd}]
```

**Acceptance criteria:**
- [ ] `compaction.md` produces parallel structured artifact alongside prose summary
- [ ] Handoff artifact stored in session DB
- [ ] Session resume loads structured handoff into hot prefix
- [ ] Import compatible with Claude Code / Codex / Pi session formats (extend existing import)

---

### F10 — AfterAgentResponse Middleware Hook

**Description:** Leverage new `AfterAgentResponse` hook as extension point for Context Cortex plugins.

**Acceptance criteria:**
- [ ] Document hook contract for context middleware
- [ ] Example plugin: log zone token counts post-response
- [ ] Hook receives session_id, message_id, token usage — sufficient for external cortex
- [ ] No regression in hook latency (<5ms overhead)

---

### F11 — TUI-Agnostic Middleware Extraction (future)

**Description:** Extract Context Cortex as standalone crate / MCP server usable by Crush, OpenCode, Pi.

**Acceptance criteria:**
- [ ] `goose-context-cortex` crate with no UI dependencies
- [ ] stdin/stdout or MCP interface: `ingest(messages) → emit(messages')`
- [ ] Reference integration doc for OpenCode plugin wrapper
- [ ] Shared surrogate schema (JSON Schema published)

---

## Implementation Phases

### Phase 0 — Foundation (extend existing)

**Build on:** `MessageMetadata`, `maybe_summarize_tool_pairs`, MOIM, `AfterAgentResponse`

| Item | Effort | Dependencies |
|---|---|---|
| F2 typed surrogates for tool-pair path | M | `context_mgmt/mod.rs` |
| F6 40% eviction threshold (config only) | S | `context_mgmt/mod.rs`, `moim.rs` |
| F4 L0 observation masking (read_file TTL) | M | `context_mgmt/`, git/mtime |
| F10 hook documentation + example | S | `agent.rs`, docs |

**Exit criteria:** Tool-pair summaries emit `ToolDigest`; eviction starts at 40%; L0 masks stale reads.

### Phase 1 — Graph + Oracle

| Item | Effort | Dependencies |
|---|---|---|
| F1 recoverability oracle | L | episode classifier |
| F3 episode DAG | L | tool pair tracking, git integration |
| F5 cold-zone assembly | M | `agent.rs` prompt builder |
| Replace middle-out with graph eviction | M | F3 |

**Exit criteria:** Eviction is graph-based; non-recoverable decisions pinned; middle-out deprecated.

### Phase 2 — UX + Handoff

| Item | Effort | Dependencies |
|---|---|---|
| F8 Context Inspector TUI | L | desktop UI, session API |
| F9 structured handoff | M | `compaction.md`, session DB |
| F7 GREP-don't-hoard polish | S | F4 + MOIM messaging |

**Exit criteria:** Users can inspect/pin/rehydrate; L3 produces structured artifact.

### Phase 3 — Extraction

| Item | Effort | Dependencies |
|---|---|---|
| F11 middleware crate | L | Phase 0–2 stable API |
| OpenCode/Crush adapter docs | M | F11 |

**Exit criteria:** Third-party harness can consume Context Cortex without forking goose.

---

## What to Take / Leave (per competitor)

| Source | Take | Leave |
|---|---|---|
| **Goose `context_mgmt`** | visibility metadata, tool-pair batching, MOIM early warning | prose summaries, middle-out heuristic |
| **Goose `agent.rs`** | async background eviction per turn | 80% as primary trigger |
| **DCP** | TUI inspector, user pin/drop | relevance heuristics without graph |
| **CWL / pi-cwl** | episode typing, LLM-free eviction policy | full protocol rewrite |
| **Crush** | summary pointer as L3 last resort only | as primary mechanism |
| **JetBrains** | observation masking as L0 | as entire solution |
| **pi-context-prune** | `context_tree_query` recoverability | lack of causal policy |
| **Anthropic context editing** | selective tool-result clearing pattern | provider lock-in |

---

## Success Metrics

| Metric | Baseline (today) | Target |
|---|---|---|
| Eviction trigger | 80% | 40% (L0–L2), 80% (L3 only) |
| Tool-pair surrogate size | ~100–500 tokens prose | 10–50 tokens typed |
| Decision retention after 50% eviction | Unmeasured; known failures | 100% pinned decisions present |
| Agent recoverability | User scroll only | Agent can re-fetch via recover_cmd |
| LLM calls per eviction cycle | 1 per tool-pair + 1 compact | 0 for L0/L2; 1 only for L1 digest (optional) |
| 4hr session coherence (LLM-judge) | TBD benchmark | ≥80% task continuation score |
| Token cost vs baseline | 1.0× | ≤0.6× on read-heavy sessions (L0) |

---

## Configuration Wishlist

| Env / Config | Default | Purpose |
|---|---|---|
| `GOOSE_TOOL_PAIR_SUMMARIZATION` | `true` | Tier 1 on/off (existing) |
| `GOOSE_AUTO_COMPACT_THRESHOLD` | `0.8` | L3 full compact (existing) |
| `GOOSE_AUTO_EVICTION_THRESHOLD` | `0.4` | **New** — L0–L2 start |
| `GOOSE_CONTEXT_HOT_PREFIX_PIN` | `[]` | **New** — turn IDs pinned to hot zone |
| `GOOSE_CONTEXT_L0_MASKING` | `true` | **New** — observation masking |
| `GOOSE_CONTEXT_SURROGATE_FORMAT` | `typed` | **New** — `typed` \| `prose` (legacy) |
| `GOOSE_CONTEXT_INSPECTOR` | `true` | **New** — TUI panel |

---

## Open Questions

1. **Upstream Block goose:** Has `aaif-goose/goose` main shipped context work not in goose-plus? (Diff follow-up.)
2. **Surrogate schema:** JSON in message text vs `MessageMetadata` extension vs separate session table?
3. **DAG persistence:** In-memory per session vs SQLite episode graph?
4. **LLM for L1:** Zero-LLM surrogates only, or optional LLM for `DecisionCard` extraction from user text?
5. **ACP/NATS:** Should Context Cortex state publish on NATS bus for fleet observability?
6. **Benchmark:** Which goose-plus e2e tests (`enhanced-context-management.spec.ts`) to extend?

---

## References

### Papers & Research

- [H₂O: Heavy-Hitter Oracle](https://arxiv.org/abs/2306.14048) — KV eviction (wrong layer, informative)
- [SAGE-KV](https://arxiv.org/html/2503.08879v1) — attention-driven KV eviction
- [StreamingLLM](https://openreview.net/forum?id=NG7sS51zVF) — attention sinks
- [MemGPT](https://arxiv.org/pdf/2310.08560) — virtual context paging
- [CWL — Context Window Layer](https://arxiv.org/html/2606.11213v1) — episode DAG eviction
- [JetBrains Complexity Trap](https://arxiv.org/html/2508.21433v3) — observation masking
- [Don't Break the Cache](https://arxiv.org/html/2601.06007v2) — prompt cache stability
- [Chroma Context Rot](https://research.trychroma.com/context-rot) — quality degradation curve
- [Memory for Autonomous LLM Agents](https://arxiv.org/html/2603.07670v1) — survey
- [CodeCompass](https://arxiv.org/html/2602.20048v1) — graph navigation vs hoarding

### Production Systems & Docs

- [Anthropic Context Editing](https://platform.claude.com/docs/en/build-with-claude/context-editing)
- [Justin3go — Context Compaction in Codex, Claude Code, OpenCode](https://justin3go.com/en/posts/2026/04/09-context-compaction-in-codex-claude-code-and-opencode)
- [badlogic compaction research gist](https://gist.github.com/badlogic/cd2ef65b0697c4dbe2d13fbecb0a0a5f)
- [OpenCode DCP plugin](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning)
- [OpenCode #10016 — early compaction](https://github.com/anomalyco/opencode/issues/10016)
- [OpenCode #14825 — prune before summarize](https://github.com/anomalyco/opencode/issues/14825)
- [Crush #2240 — middle drop](https://github.com/charmbracelet/crush/issues/2240)
- [Codex #6426 — 256-line truncation](https://github.com/openai/codex/issues/6426)
- [AWS — Why AI Agents Fail](https://dev.to/aws/why-ai-agents-fail-3-failure-modes-that-cost-you-tokens-and-time-1flb)
- [Hindsight — Agent Harness Needs Memory](https://hindsight.vectorize.io/blog/2026/05/04/agent-harness-needs-memory)
- [Cognition — Why Not Multi-Agent](https://jxnl.co/writing/2025/09/11/why-cognition-does-not-use-multi-agent-systems/)

### Local Source (goose-plus @ 9fd61b3ab)

- `crates/goose/src/context_mgmt/mod.rs`
- `crates/goose/src/agents/moim.rs`
- `crates/goose/src/agents/agent.rs`
- `crates/goose/src/prompts/compaction.md`
- `crates/goose/src/session/last_message_snippet.rs`
- `ui/desktop/tests/e2e/enhanced-context-management.spec.ts`
- `ui/desktop/tests/e2e/context-management.spec.ts`

### Local Comparators (forensics, June 2026)

- `/home/andrew/crush` — `internal/agent/agent.go` (summary pointer)
- `/home/andrew/opencode` — `packages/opencode/src/session/chat.ts`, `summarize.ts`

### Research Tools Used

- SearXNG MCP (`searxng-mcp` @ `http://192.168.1.211:8890`)
- Edgar Morin MCP (complex thought synthesis, transcendence 0.88–0.93)
- Repomix MCP (local codebase packs)

---

## Appendix: Edgar Morin Synthesis

**Dialogic tension resolved:** Goose's incremental visibility path and Crush/OpenCode's summary-pointer path are **different memory models**, not competing implementations of the same idea.

1. **Goose invented the right primitive** — visibility metadata + incremental eviction — but stopped at prose summaries and positional heuristics.
2. **Crush/OpenCode optimized ship speed** — summary pointer is one code path until it isn't.
3. **Plugins validate demand** but cannot fix cache economics or episode graphs without harness cooperation.
4. **`goose-plus` is the seed** — `context_mgmt/` has ~80% of Context Cortex L0+L1; competitors' cores do not.

**Outsmart move:** Extend `MessageMetadata` + `maybe_summarize_tool_pairs` into typed surrogates with git-backed `FileAnchor` recoverability, drop trigger to 40%, port as middleware — don't restart from Crush's summary-pointer or OpenCode's 90% compact.

---

*This document captures research and wishlist items from a June 2026 investigation into selective context replacement for coding agents. It is not a commitment to implement all features — priority order follows Implementation Phases above.*