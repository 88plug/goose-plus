# WISHLIST.md — Autopoietic Relevance Ecology (ARE) PRD

**Product:** goose-plus session cognition evolution  
**Status:** Draft PRD  
**Baseline:** `9fd61b3ab` (`plus-v1.39.22`) · wishlist committed at `4de94604c`  
**Synthesis:** SearXNG research · Edgar Morin complex thought · local repomix forensics (goose-plus, Crush, OpenCode)

---

## Executive Summary

Coding-agent sessions degrade at **40–60% context utilization** while every major TUI waits until **80–95%** to act. The industry treats the problem as **buffer management** — compact, truncate, summarize. That is the wrong ontology.

**Autopoietic Relevance Ecology (ARE)** reframes the harness + repo + tools + user as a **coupled cognitive system**. The context window is not memory. It is a **broadcast channel** for beliefs and surprises that demand action *this turn*. Everything else lives in a graded belief ecology at tunable confidence until task perturbations pull it back.

goose-plus is the seed codebase: dual visibility metadata, incremental tool-pair eviction, MOIM early warning. ARE extends those primitives into belief atoms, per-turn selection-broadcast, ecological eviction, offline metagraph consolidation, and deuterolearning — with narrative compaction demoted to rare emergency surgery.

**Default operation:** belief revision + attention selection.  
**Rare operation:** L3 narrative compact (manual or irrecoverable prose only).

---

## The Level-Shift

### What engineering asks

> How do we replace messages smarter than compaction?

### What Morin / Bateson / Goertzel / Friston ask

> What is the cognitive system, and what role does a context window play in it?

| Framework | Core insight | ARE implication |
|---|---|---|
| **Morin** — pensée complexe | Systems are dialogic; observer is inside; preserve contradictions ([Codex Numeris](https://codexnumeris.org/46-cultiver-la-pensee-complexe/)) | Eviction preserves tension — abandoned approaches become low-confidence ghost constraints, not deletions |
| **Bateson** — ecology of mind | Information is difference; **deuterolearning** = learning about learning | Session learns meta-policies for its own memory management |
| **Goertzel** — cognitive synergy | Memory processes unstick each other via shared [Atomspace metagraph](https://hyperon.opencog.org/) ([paper](https://arxiv.org/pdf/1703.04361)) | Background consolidation merges belief atoms; compaction = graph chunking when stuck, not prose |
| **Friston** — active inference | Minimize surprise via generative world-model ([free energy principle](https://en.wikipedia.org/wiki/Free_energy_principle)) | Context carries **beliefs + prediction errors**, not transcripts |
| **Pei Wang** — NARS | [Non-axiomatic reasoning](https://cis.temple.edu/~pwang/NARS-Intro.html): all knowledge provisional; bag-memory under resource limits | Eviction lowers confidence and archives — never binary delete |
| **LIDA / GWT** | [Selection-broadcast cognitive cycle](https://aaai.org/papers/0011-fs07-01-011-%EF%80%A0lida-a-computational-model-of-global-workspace-theory-and-developmental-learning/) | Per-turn competition for workspace slots — not static hot/warm/cold zones |
| **Vervaeke** — relevance realization | Salience is participatory, telos-dependent ([Frontiers 2024](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2024.1362658/full)) | Broadcast scoring weights user intent, surprises, coupling sensors |
| **Clark / Hutchins** | [Extended mind](https://link.springer.com/article/10.1007/s11229-025-05046-y) + [distributed cognition](https://en.wikipedia.org/wiki/Distributed_cognition) | Unit of analysis = agent + repo + git + tools + user |
| **Maturana** — structural coupling | Cognition = effective action under perturbation ([autopoiesis paper](https://reflexus.org/wp-content/uploads/Autopoiesis-structural-coupling-and-cognition.pdf)) | Belief decay driven by git/test perturbations, not timers alone |

**Dialogic tension (Morin):** Friston says don't store history. Goertzel says store everything in a metagraph. JetBrains says mask and done. **All three are right at different timescales** — light ecology per-turn, heavy consolidation offline.

---

## Problem Statement

### Symptoms

- Causal chains break after compaction ("you told me X" when X was summarized away).
- Quality collapses at 40–60% utilization; harnesses act at 80–95% ([Chroma context rot](https://research.trychroma.com/context-rot), [OpenCode #10016](https://github.com/anomalyco/opencode/issues/10016)).
- Tool outputs dominate token budget; truncation destroys recoverability.
- Middle-edits break prompt-cache prefix ([Don't Break the Cache](https://arxiv.org/html/2601.06007v2)).

### Root cause

Harnesses optimize `messages[]` length. The real object is a **coupled belief ecology** distributed across chat, git, filesystem, tests, and user intent. Full compaction **flattens an ecology into prose**, destroying structure ([CWL](https://arxiv.org/html/2606.11213v1)).

### Why "Context Cortex" was insufficient

Context Cortex (prior draft of this doc) improved buffer management: typed surrogates, episode DAG, recoverability oracle, hot/warm/cold zones. That is a better garbage collector — still a **transcript manager**. ARE supersedes it by changing the unit of memory from **messages** to **belief atoms**.

---

## Vision: Autopoietic Relevance Ecology

### One sentence

> The repo is the body; beliefs are the nervous system; the context window is whatever currently demands action; everything else lives in the ecology at graded confidence until the task perturbs it back.

### Three invariants

| Invariant | Meaning |
|---|---|
| **CANONICALITY** | Truth lives in git + filesystem. Chat is disposable working surface. |
| **RECOVERABILITY** | Nothing removed without surrogate, re-fetch path, or explicit confidence archive. |
| **PREFIX STABILITY** | System prompt + pinned invariants = immutable broadcast prefix. Eviction happens in belief ecology, not by mutating the prefix. |

### Distributed cognitive unit

```
Agent ⟷ git repo ⟷ filesystem ⟷ tool outputs ⟷ user intent
         ↑________________ coupling sensors ________________↑
              (diff, mtime, test status, CI)
```

The context window is LIDA's **conscious workspace** — a narrow broadcast, not the whole mind.

### Four operating layers

```
L0  Observation Masking      — hide stale tool outputs (zero LLM cost)
    [JetBrains: halves cost, matches summarization]

L1  Belief Extraction         — tool outputs → typed belief atoms with confidence
    invariant | state | constraint | surprise | digest

L2  Selection-Broadcast       — per-turn codelet competition for workspace slots
    [LIDA/GWT: attention oscillates, not static zones]

L3  Ecological Consolidation   — background metagraph merge when processes stuck
    [Goertzel cognitive synergy: offline, off hot path]

L4  Narrative Compact         — LAST RESORT for irrecoverable prose only
    [current Codex/Claude/OpenCode — demoted, not deleted]
```

**Trigger L0–L2 at ~40% context.** Reserve L4 for ~85%+ or manual `/compact`.

---

## How ARE Works

### Belief atoms (replace transcripts)

After each tool call or user turn, extract beliefs — not store raw output:

```yaml
beliefs:
  - id: b-47
    type: invariant          # user decision — Morin: pin dialogic tension
    text: "auth uses JWT refresh, 7d TTL"
    confidence: 0.95
    source: user@turn-12
    pinned: true

  - id: b-48
    type: state              # recoverable world-state
    text: "middleware.ts exports withAuth, withRateLimit"
    confidence: 0.7
    source: read_file@turn-34
    recover: "read_file middleware.ts"
    last_verified_sha: a3f2c1

  - id: b-49
    type: surprise           # Friston: prediction error → must resolve
    text: "tests fail: rate limit not applied to /api/v2"
    confidence: 1.0
    source: test_run@turn-41
    action_required: true

  - id: b-50
    type: constraint         # Bateson ghost: abandoned approach
    text: "Redis session store broke tests — do not retry"
    confidence: 0.25
    source: failed_attempt@turn-8
    pinned: false            # kept at low confidence, not deleted
```

**What enters the LLM prompt each turn:**
- Pinned invariants and constraints
- Open surprises (prediction errors)
- Broadcast winners from salience competition
- **Not** 200 turns of tool logs

Raw messages stay in DB for user scroll. Agent sees beliefs.

### Selection-broadcast (per turn, not zones)

Codelets compete for workspace slots:

| Codelet | Salience drivers |
|---|---|
| Open test failure | surprise × urgency |
| User's latest instruction | recency × authority |
| File changed since last belief | structural coupling (git delta) |
| Constraint violation risk | pinned × task relevance |
| Stale state belief | confidence decay (NARS) |

Winners → context. Losers → archived beliefs (confidence ↓, recover path kept).

### Ecological eviction (not graph surgery)

Beliefs participate in an ecology:

| Role | Eviction priority |
|---|---|
| **Parasites** — redundant reads of same file | First to decay |
| **Symbionts** — mutually reinforcing beliefs | Decay together slowly |
| **Keystones** — architectural decisions | Never evict without user pin |
| **Ghosts** — failed approaches | Low confidence forever (Morin dialogic) |

Eviction = **succession**, not deletion. NARS: confidence approaches zero; belief archived, not erased.

### Structural coupling (Maturana)

Recoverability is not "can we re-run grep?" It is:

> Does evicting this belief break the agent's ability to compensate for repo perturbations?

Coupling sensors: `git diff`, file mtime, test status, CI signal. A `read_file` belief auto-decays when its file changes — perturbation-driven, not timer-only.

### Cognitive synergy offline (Goertzel)

Background task (extends goose's `maybe_summarize_tool_pairs`):

- Consolidate archived beliefs into metagraph chunks
- When episodic beliefs bottleneck procedural action, cross-feed (synergy unsticks processes)
- Runs off hot path — never blocks the turn loop

### Deuterolearning (Bateson) — the missing layer everywhere

Session learns meta-policies from its own eviction history:

```
observed: auth refactors → constraint beliefs needed 3× longer
learned:  task_class=refactor + domain=auth → pin_constraints_ttl=2h

observed: exploratory grep → 90% read beliefs never referenced again
learned:  task_class=explore → state_belief_ttl=5min
```

Second-order memory management. No current TUI has this.

### One turn under ARE

```
1. Coupling sensors: 3 files changed, 1 test red
2. Belief update: b-48 confidence 0.7 → 0.3 (stale sha)
3. New surprise belief from test failure
4. Codelet competition: user intent > test failure > stale middleware
5. Broadcast ~2k tokens to LLM
6. Agent acts (fix rate limit)
7. Background: consolidate 40 archived grep beliefs → 1 metagraph chunk
8. Deutero: "rate-limit bugs → keep test beliefs pinned until green"
```

No compact. No prose summary. No middle-out.

L4 fires only for irrecoverable prose (long design debates) or manual `/compact`.

---

## Current State: goose-plus Baseline

**HEAD:** `9fd61b3ab` (`plus-v1.39.22`)

### Key files

| File | Role |
|---|---|
| `crates/goose/src/context_mgmt/mod.rs` | Compaction, tool-pair summarization, middle-out filter |
| `crates/goose/src/agents/moim.rs` | `<turn-context>` injection, compaction countdown |
| `crates/goose/src/agents/agent.rs` | Orchestration, async eviction, `AfterAgentResponse` hook |
| `crates/goose/src/prompts/compaction.md` | L4 compaction prompt |
| `crates/goose/src/session/last_message_snippet.rs` | Session list UI only — not agent cognition |

### What exists today

| Tier | Mechanism | ARE mapping |
|---|---|---|
| T1 | `maybe_summarize_tool_pairs()` — batch 10, background async | Proto-L1: replace tool pairs, but lossy prose |
| T2 | Auto-compact at **80%**, middle-out tool removal, `compaction.md` | L4 today — should become rare |
| T3 | MOIM `<turn-context>` countdown after 50% of threshold | Proto-broadcast warning |
| — | Dual `MessageMetadata` visibility | Proto-belief hiding |
| — | `AfterAgentResponse` hook | Extension point for ARE middleware |
| — | Recovery compact on `ContextLengthExceeded` | Emergency L4 |

### Gap to ARE

| Have | Need |
|---|---|
| Message visibility layers | Belief atom store with confidence |
| Lossy tool-pair prose summaries | Typed `ToolDigest` / `FileAnchor` beliefs |
| Static 80% compact trigger | 40% L0–L2 + 85% L4 |
| Middle-out positional heuristic | Selection-broadcast + ecological succession |
| No meta-learning | Deuterolearning policies |
| No coupling sensors | Git/test-driven confidence decay |

---

## Competitive Landscape

| Capability | goose-plus | Crush | OpenCode core | DCP plugin |
|---|---|---|---|---|
| Incremental eviction | ✅ tool-pair | ❌ | ❌ | ✅ heuristic |
| Dual visibility | ✅ | ❌ | ❌ | partial |
| Belief / surrogate layer | ❌ (prose only) | ❌ | ❌ | ❌ |
| Selection-broadcast | ❌ | ❌ | ❌ | ❌ |
| Full compact fallback | ✅ 80% | ✅ ~80–90% | ✅ 90% | ✅ |
| Coupling sensors | ❌ | ❌ | ❌ | ❌ |
| Deuterolearning | ❌ | ❌ | ❌ | ❌ |
| Trigger timing | 80% | 80–90% | 90% | configurable |

**Crush:** summary-pointer handoff — `SummaryMessageID` slices history ([`agent.go`](../../crush/internal/agent/agent.go)). Nuclear, not layered.

**OpenCode:** 90% full summarization in `chat.ts` / `summarize.ts`. DCP plugin proves demand; core deferred the hard problem.

**Verdict:** goose-plus remains the only local codebase with incremental eviction + visibility primitives. ARE builds here first.

---

## Goals & Non-Goals

### Goals

| # | Goal |
|---|---|
| G1 | Coherence through 4+ hour sessions — no quality cliff at 40–50% utilization |
| G2 | Default: belief revision + broadcast — not compaction |
| G3 | Every eviction: confidence archive + recover path or pin |
| G4 | Prefix stability for prompt-cache economics |
| G5 | L0–L2 active at ~40%; L4 rare at ~85% or manual |
| G6 | Coupling sensors (git, tests) drive belief decay |
| G7 | Deuterolearning of session-specific eviction meta-policy |
| G8 | Context Inspector TUI: inspect beliefs, pin, rehydrate, preview broadcast |
| G9 | Design for TUI-agnostic extraction after goose-plus proves it |

### Non-Goals

- KV-cache eviction (H₂O, SAGE-KV, StreamingLLM) — wrong layer for API agents
- Full Hyperon/Atomspace rewrite — borrow synergy *pattern*, not full stack
- MemGPT virtual memory OS — filesystem + git is the archive
- Multi-agent context splitting — telephone problem ([Cognition](https://jxnl.co/writing/2025/09/11/why-cognition-does-not-use-multi-agent-systems/))
- Solving embodied relevance realization (Vervaeke) — approximate via salience under coupling
- Upstream Block goose diff in this doc

---

## Feature Wishlist

### F1 — Belief Atom Store

**Description:** Session-scoped belief DB parallel to message history. Messages become provenance; beliefs become agent-facing memory.

**Schema (illustrative):**

```rust
struct Belief {
    id: BeliefId,
    belief_type: Invariant | State | Constraint | Surprise | Digest,
    text: String,
    confidence: f32,          // 0.0–1.0, NARS-style
    source: Provenance,       // message_id, turn, tool_call_id
    recover: Option<RecoverCmd>,
    pinned: bool,
    last_verified_sha: Option<String>,
    coupling_tags: Vec<String>,  // files, test suites
}
```

**Acceptance criteria:**
- [ ] Beliefs persist in session DB alongside messages
- [ ] Every tool pair can emit ≥1 belief atom
- [ ] Agent prompt assembled from beliefs, not raw message scrollback
- [ ] User UI still shows full message history unchanged

---

### F2 — Belief Extraction (L1)

**Description:** Replace `summarize_tool_call()` prose with typed belief emission.

| Tool pattern | Belief type | Example |
|---|---|---|
| `read_file` | `State` | `middleware.ts:47-120 exports withAuth (sha:a3f2)` |
| `grep` / `bash` | `Digest` | `grep "auth" → 3 hits (re-run: …)` |
| `write_file` / commit | `State` + coupling tag | file + sha |
| test failure | `Surprise` | `action_required: true` |
| user decision | `Invariant` | auto-pinned |

**Acceptance criteria:**
- [ ] `GOOSE_BELIEF_EXTRACTION=true` (default on, replaces prose summarization)
- [ ] Typical belief <50 tokens
- [ ] `GOOSE_BELIEF_FORMAT=typed|prose` legacy fallback
- [ ] Unit tests per tool pattern

---

### F3 — Coupling Sensors

**Description:** Git diff, file mtime, test status update belief confidence each turn.

**Acceptance criteria:**
- [ ] `State` beliefs decay when `last_verified_sha` ≠ current HEAD blob
- [ ] `Surprise` beliefs auto-pin until test green or user dismiss
- [ ] Sensor run is LLM-free, <100ms typical
- [ ] MOIM reports coupling deltas in `<turn-context>`

---

### F4 — Selection-Broadcast Engine (L2)

**Description:** Per-turn codelet competition replaces static hot/warm/cold zones and middle-out heuristic.

**Salience formula (initial):**

```
score = w1·surprise + w2·user_recency + w3·pin + w4·coupling_delta
        - w5·confidence_decay - w6·redundancy
```

**Acceptance criteria:**
- [ ] Broadcast budget configurable (default: 40% of context limit)
- [ ] Winning beliefs injected as structured block, not message replay
- [ ] `protect_last_n` tool turns still honored
- [ ] Deterministic given same beliefs + sensors (testable without LLM)

---

### F5 — Ecological Eviction + NARS Confidence

**Description:** Eviction lowers confidence and archives. Ghost constraints stay at low confidence. Keystones require explicit unpin.

**Acceptance criteria:**
- [ ] No belief hard-deleted during session (archive only)
- [ ] `confidence < 0.1` → excluded from broadcast, recover path retained
- [ ] Redundant `State` beliefs on same file merge (parasite culling)
- [ ] User pin sets `confidence = 1.0, pinned = true`

---

### F6 — Observation Masking (L0)

**Description:** Zero-LLM stale output masking before belief extraction.

**Acceptance criteria:**
- [ ] Mask `read_file` content when file changed since read
- [ ] Mask duplicate identical tool calls within session
- [ ] Uses existing `agent_invisible` metadata
- [ ] Runs before F2 on every turn
- [ ] Token savings logged (target: ≥30% on read-heavy sessions)

---

### F7 — Threshold Rebalance

| Threshold | Today | Target |
|---|---|---|
| L0–L2 activation | 80% (compact) | **40%** (`GOOSE_AUTO_EVICTION_THRESHOLD`) |
| MOIM countdown start | 50% of 80% | **25%** of context |
| L4 compact | 80% | **85%** (`GOOSE_AUTO_COMPACT_THRESHOLD`) |

**Acceptance criteria:**
- [ ] Separate env vars for eviction vs compact
- [ ] L4 compact unchanged in behavior, just rarer
- [ ] E2E benchmark: task continuation score at 50% utilization vs baseline

---

### F8 — Cognitive Synergy Consolidator (L3)

**Description:** Background metagraph merge of archived beliefs. Fires when belief count exceeds budget or synergy heuristic detects stuckness (Goertzel).

**Acceptance criteria:**
- [ ] Runs in background `tokio` task (extends tool-pair pattern)
- [ ] Produces consolidated belief clusters, not prose
- [ ] Never mutates pinned/invariant beliefs
- [ ] Consolidation logged with before/after atom count

---

### F9 — Deuterolearning Meta-Policy

**Description:** Session-end or periodic update of eviction TTL weights from observed patterns.

**Acceptance criteria:**
- [ ] Per-project policy file (e.g. `.goose/belief-policy.toml`)
- [ ] At least 3 tunable params: `state_ttl`, `constraint_pin_duration`, `explore_vs_refactor bias`
- [ ] Policy influences F4 salience weights next session
- [ ] User can inspect and override in Context Inspector

---

### F10 — Context Inspector TUI

**Description:** Belief ecology dashboard — not just message list.

**Capabilities:**
- View beliefs by type, confidence, coupling tags
- Pin / unpin / rehydrate (run `recover_cmd`)
- Preview what broadcast would select this turn
- Show parasite/redundant clusters
- One-key force L4 compact (with structured handoff preview)

**Acceptance criteria:**
- [ ] Desktop UI panel
- [ ] Headless CLI: `goose-plus session beliefs <id>`
- [ ] Rehydrate temporarily promotes belief to full tool output in warm broadcast

---

### F11 — Structured Handoff (L4 upgrade)

**Description:** When L4 compact fires, emit machine-readable artifact alongside prose.

```yaml
invariants: [{id, text, turn}]
constraints: [{text, confidence, why}]
active_beliefs: [{id, type, confidence, recover}]
coupling_state: {sha, dirty_files, test_status}
open_surprises: [{text, action_required}]
archived_count: 1247
```

**Acceptance criteria:**
- [ ] Stored in session DB, loadable on resume
- [ ] Import compatible with existing Claude Code / Codex / Pi session import
- [ ] Prose summary optional when structured handoff present

---

### F12 — AfterAgentResponse Middleware Contract

**Description:** Document and expose hook for external ARE plugins.

**Acceptance criteria:**
- [ ] Hook receives: session_id, message_id, beliefs_broadcast, token_usage
- [ ] Example plugin: confidence calibration from outcome
- [ ] <5ms overhead

---

### F13 — TUI-Agnostic Extraction (future)

**Description:** `goose-are` crate — belief ecology engine with no UI deps.

**Acceptance criteria:**
- [ ] API: `ingest(turn) → beliefs' + broadcast`
- [ ] JSON Schema for belief atoms published
- [ ] Reference OpenCode/Crush adapter doc

---

## Implementation Phases

### Phase 0 — Belief Foundation

*Extend existing `context_mgmt/` + `MessageMetadata`*

| Item | Features |
|---|---|
| Belief atom store in session DB | F1 |
| Typed extraction replaces prose tool-pair summary | F2 |
| L0 observation masking | F6 |
| Threshold rebalance (40% / 85%) | F7 |
| Hook documentation | F12 |

**Exit:** Tool pairs emit beliefs; eviction starts at 40%; stale reads masked.

### Phase 1 — Attention & Coupling

| Item | Features |
|---|---|
| Coupling sensors (git sha, mtime) | F3 |
| Selection-broadcast engine | F4 |
| NARS confidence eviction | F5 |
| MOIM shows belief ecology summary | F3 |

**Exit:** Per-turn broadcast; no middle-out; git-driven decay.

### Phase 2 — Ecology & UX

| Item | Features |
|---|---|
| Background synergy consolidator | F8 |
| Context Inspector TUI | F10 |
| Structured L4 handoff | F11 |
| Test-surprise auto-pin | F3 |

**Exit:** Users inspect/pin/rehydrate beliefs; L4 produces structured artifact.

### Phase 3 — Learning & Extraction

| Item | Features |
|---|---|
| Deuterolearning meta-policy | F9 |
| `goose-are` crate | F13 |
| OpenCode/Crush adapter | F13 |

**Exit:** Sessions improve eviction policy over time; engine portable.

---

## Configuration

| Env / Config | Default | Purpose |
|---|---|---|
| `GOOSE_TOOL_PAIR_SUMMARIZATION` | `true` | Legacy; superseded by `GOOSE_BELIEF_EXTRACTION` |
| `GOOSE_BELIEF_EXTRACTION` | `true` | L1 belief emission |
| `GOOSE_BELIEF_FORMAT` | `typed` | `typed` \| `prose` |
| `GOOSE_AUTO_EVICTION_THRESHOLD` | `0.4` | L0–L2 start |
| `GOOSE_AUTO_COMPACT_THRESHOLD` | `0.85` | L4 compact (was 0.8) |
| `GOOSE_CONTEXT_L0_MASKING` | `true` | Observation masking |
| `GOOSE_BROADCAST_BUDGET` | `0.4` | Fraction of context for beliefs |
| `GOOSE_COUPLING_SENSORS` | `true` | Git/test-driven decay |
| `GOOSE_SYNERGY_CONSOLIDATE` | `true` | Background L3 |
| `GOOSE_DEUTERO_LEARN` | `true` | Meta-policy learning |
| `GOOSE_CONTEXT_INSPECTOR` | `true` | TUI panel |

---

## Success Metrics

| Metric | Baseline (today) | Target |
|---|---|---|
| Primary strategy | Compact at 80% | Broadcast at 40%; compact rare |
| Agent-facing unit | Messages | Belief atoms |
| Typical belief size | 100–500 tokens prose | 10–50 tokens typed |
| Decision retention at 50% util | Unmeasured; known failures | 100% pinned invariants |
| Agent recoverability | User scroll only | `recover_cmd` + rehydrate |
| LLM calls per eviction cycle | 1+ per tool-pair + compact | 0 for L0/L2; optional for L1 |
| 4hr session coherence (LLM-judge) | TBD | ≥80% task continuation |
| Token cost vs baseline | 1.0× | ≤0.6× read-heavy sessions |

---

## Open Questions

1. Belief schema: inline in `MessageMetadata` vs separate SQLite table vs both?
2. Broadcast block format: YAML in user message vs system injection vs MOIM extension?
3. Deuterolearning: per-repo `.goose/belief-policy.toml` vs global user profile?
4. Synergy consolidator: LLM-assisted chunk naming or purely structural merge?
5. NATS/A2A: publish belief ecology state for fleet observability?
6. Upstream `aaif-goose/goose` main: context work to port or supersede?

---

## References

### Thought Leaders & Frameworks

- [Morin — pensée complexe face à l'IA](https://codexnumeris.org/46-cultiver-la-pensee-complexe/)
- [Goertzel — Cognitive Synergy (arXiv:1703.04361)](https://arxiv.org/pdf/1703.04361)
- [OpenCog Hyperon](https://hyperon.opencog.org/)
- [Friston — Free Energy Principle](https://en.wikipedia.org/wiki/Free_energy_principle)
- [VERSES — Active Inference](https://www.verses.ai/blog/tag/active-inference)
- [Pei Wang — NARS Introduction](https://cis.temple.edu/~pwang/NARS-Intro.html)
- [LIDA — Global Workspace cognitive cycle](https://aaai.org/papers/0011-fs07-01-011-%EF%80%A0lida-a-computational-model-of-global-workspace-theory-and-developmental-learning/)
- [Vervaeke — Relevance Realization (Frontiers 2024)](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2024.1362658/full)
- [Clark — Extended Mind + LLMs (Synthese 2025)](https://link.springer.com/article/10.1007/s11229-025-05046-y)
- [Hutchins — Distributed Cognition](https://en.wikipedia.org/wiki/Distributed_cognition)
- [Maturana — Autopoiesis & Structural Coupling](https://reflexus.org/wp-content/uploads/Autopoiesis-structural-coupling-and-cognition.pdf)
- [Bateson — Ecology of Mind](https://en.wikipedia.org/wiki/Gregory_Bateson)

### Engineering & Context Rot

- [Chroma — Context Rot](https://research.trychroma.com/context-rot)
- [CWL — Context Window Layer](https://arxiv.org/html/2606.11213v1)
- [JetBrains — Complexity Trap / observation masking](https://arxiv.org/html/2508.21433v3)
- [Don't Break the Cache](https://arxiv.org/html/2601.06007v2)
- [Anthropic Context Editing](https://platform.claude.com/docs/en/build-with-claude/context-editing)
- [Justin3go — Compaction in Codex, Claude Code, OpenCode](https://justin3go.com/en/posts/2026/04/09-context-compaction-in-codex-claude-code-and-opencode)
- [OpenCode DCP plugin](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning)
- [OpenCode #10016](https://github.com/anomalyco/opencode/issues/10016) · [#14825](https://github.com/anomalyco/opencode/issues/14825)
- [Crush #2240](https://github.com/charmbracelet/crush/issues/2240)
- [Codex #6426](https://github.com/openai/codex/issues/6426)

### goose-plus Source (@ 9fd61b3ab)

- `crates/goose/src/context_mgmt/mod.rs`
- `crates/goose/src/agents/moim.rs`
- `crates/goose/src/agents/agent.rs`
- `crates/goose/src/prompts/compaction.md`
- `ui/desktop/tests/e2e/enhanced-context-management.spec.ts`
- `ui/desktop/tests/e2e/context-management.spec.ts`

---

## Appendix: Evolution from Context Cortex

Context Cortex (commit `4de94604c` first draft) proposed:
- Episode DAG eviction
- Typed surrogates (`DecisionCard`, `FileAnchor`, `ToolDigest`)
- Recoverability oracle
- Hot/warm/cold zones
- 40% trigger

ARE preserves those as **mechanisms inside a larger model**:

| Context Cortex concept | ARE upgrade |
|---|---|
| Typed surrogates | **Belief atoms** with confidence + provenance |
| Episode DAG | **Belief ecology** (parasite/symbiont/keystone) |
| Recoverability oracle | **Structural coupling** + `recover_cmd` |
| Hot/warm/cold zones | **Selection-broadcast** per turn |
| 40% trigger | Unchanged — but triggers belief ecology, not just replacement |
| Compaction as L3 | **L4 last resort** — beliefs are default |

Context Cortex was Phase 0 thinking. ARE is the destination.

---

*Replace first. Broadcast what matters. Compact only when the ecology cannot breathe. Chat is surface; git is body; beliefs are mind.*