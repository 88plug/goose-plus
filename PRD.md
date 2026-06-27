# PRD — Autopoietic Relevance Ecology (ARE)

**Product:** goose-plus cognitive substrate  
**Codename:** ARE  
**Status:** Approved direction · implementation pending  
**Baseline codebase:** `9fd61b3ab` (`plus-v1.39.22`)  
**Supersedes:** `WISHLIST.md` (removed)

---

## 1. Mission

Make goose-plus the coding agent whose **outputs and creations blow away every other tool in existence** — not by using a bigger model, but by **never committing epistemic violence against its own session**.

Every competitor — Claude Code, Codex, OpenCode, Crush, Cursor — eventually **compacts**. Compaction is amnesia dressed as efficiency. It destroys why decisions were made, what failed, what the user corrected, and which constraints still bind. Quality does not die at 100% context; it dies at 40–60% ([Chroma context rot](https://research.trychroma.com/context-rot)) while harnesses sleep until 80–95%.

**ARE eliminates compaction as default behavior.** goose-plus will:

1. Maintain a **belief ecology** — graded, recoverable, dialogically complete.
2. **Broadcast** only what demands action this turn into the LLM context window.
3. Use **NATS/JetStream** as a distributed nervous system so belief, coupling, and learning propagate across instances and sessions.
4. Reserve narrative compaction for **manual emergency** only.

The context window stops being memory. It becomes **conscious workspace** (LIDA/GWT). Memory lives in git, belief store, and the bus.

---

## 2. The Competitive Moat

### 2.1 What competitors cannot do

| Failure mode | Crush / OpenCode / Codex / Claude Code | goose-plus + ARE |
|---|---|---|
| Long-session coherence | Summary-pointer or prose compact at 80–95% | Belief broadcast; compact rare |
| Preserved decisions | Summarized away | `Invariant` beliefs pinned |
| Preserved failures | Lost | `Constraint` ghosts at low confidence |
| Repeated mistakes | Re-try abandoned approaches | Ghost constraints + deuterolearning |
| Multi-instance work | Silent file races | `goose_coord` + shared belief KV |
| Fleet learning | None | `goose_deutero` KV per repo |
| Recoverability | User scrollback only | `recover_cmd` + JetStream Object Store |
| Output quality at hour 4 | Degraded / hallucinated | Same invariant surface |

### 2.2 Why better code comes out

Compaction is **epistemic violence** (Morin: reduction of dialogic reality to monologue). An agent that forgets:

- *why* JWT was chosen over sessions,
- *that* Redis already failed,
- *what* the user said at turn 12,

will produce **architecturally inconsistent code** — wrong abstractions, repeated dead ends, subtle regressions.

ARE preserves the **dialogic structure** of a session: decisions, surprises, constraints, and state — each as typed beliefs with confidence. The LLM sees a **curated cognitive surface**, not a mutilated transcript. Code quality is a downstream effect of **never lying to the model about its own history**.

### 2.3 Why NATS makes this uncopyable

Single-process TUIs can mimic belief extraction locally. They **cannot** easily replicate:

- **Fleet-visible belief ecology** — instance A's `read_file` insight becomes instance B's `State` belief without re-fetch.
- **Cross-instance coupling** — git/test perturbations propagate via KV watchers.
- **Deuterolearning at repo scale** — one session teaches all instances how to manage auth-refactor memory.
- **Durable event sourcing** — JetStream replay rebuilds belief state after crash.
- **Coordination + cognition unified** — same bus that locks files also carries belief deltas.

goose-plus already ships NATS firehose, drive loop, and JetStream KV coordination ([`docs/nats.md`](docs/nats.md)). ARE turns that infrastructure from **telemetry** into **the agent's nervous system**.

---

## 3. Philosophical Foundation

Synthesized via pensée complexe (Morin transcendence ~0.92) from Bateson, Goertzel, Friston, Wang, LIDA, Vervaeke, Clark/Hutchins, Maturana.

### 3.1 Core reframing

| Old ontology | ARE ontology |
|---|---|
| Context = message buffer | Context = broadcast workspace |
| Memory = chat history | Memory = belief ecology + git |
| Eviction = delete/summarize | Eviction = confidence decay + archive |
| Agent = LLM + tools | Agent = coupled system (agent ⟷ repo ⟷ bus ⟷ user) |
| Success = don't overflow | Success = stay coherent at 40% utilization |

### 3.2 Morinian principles in ARE

| Principle | Implementation |
|---|---|
| **Dialogic** | Ghost constraints preserve abandoned paths; no single prose narrative |
| **Hologrammatic** | Each belief carries provenance linking to whole task ecology |
| **Recursive** | Deuterolearning: session learns policies about its own memory |
| **Subject reintroduction** | User pins and corrections enter belief layer as first-class |
| **Auto-eco-organization** | Ecological succession evicts parasites before keystones |

### 3.3 Borrowed mechanisms

| Source | Mechanism in ARE |
|---|---|
| **Friston** — active inference | `Surprise` beliefs = prediction errors requiring action |
| **Wang** — NARS | All beliefs provisional; confidence decay, never hard delete |
| **LIDA** — GWT | Per-turn selection-broadcast of codelets |
| **Goertzel** — cognitive synergy | Background metagraph consolidation when stuck |
| **Bateson** — deuterolearning | Fleet meta-policy in `goose_deutero` KV |
| **Maturana** — structural coupling | Git/test sensors drive belief decay |
| **Clark/Hutchins** — extended/distributed mind | Unit of cognition includes repo + NATS bus |

---

## 4. System Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         goose-plus agent turn                            │
├─────────────────────────────────────────────────────────────────────────┤
│  Coupling Sensors ──► Belief Store ──► Selection-Broadcast ──► LLM     │
│  (git, tests, mtime)     ▲                    │                        │
│                          │                    ▼                        │
│                    L0 Mask ──► L1 Extract ──► L3 Consolidate (async)    │
└──────────────────────────┬──────────────────────────────────────────────┘
                           │
           ┌───────────────┴───────────────┐
           │     NATS Dual-Plane Bus        │
           ├───────────────────────────────┤
           │  Plane A: Core NATS (hot)     │  belief.delta, broadcast.preview
           │           best-effort, <1ms    │  coord.claim, drive.cmd
           │  Plane B: JetStream (cold)    │  durable belief events, KV
           │           async append         │  Object Store recovery blobs
           └───────────────────────────────┘
                           │
           ┌───────────────┴───────────────┐
           │  JetStream KV Buckets           │
           │  goose_beliefs   session ecology│
           │  goose_coupling  git/test state │
           │  goose_deutero   meta-policies  │
           │  goose_coord     file leases    │
           │  goose_recovery  (Object Store) │
           └───────────────────────────────┘
```

### 4.1 Invariants

| # | Invariant | Enforcement |
|---|---|---|
| I1 | **CANONICALITY** — git/filesystem owns truth | Beliefs reference sha/mtime; chat is disposable |
| I2 | **RECOVERABILITY** — no silent loss | `recover_cmd` or Object Store blob for every archive |
| I3 | **PREFIX STABILITY** — cache-safe broadcast | Pinned invariants never mutated mid-session |
| I4 | **NON-BLOCKING** — bus never stalls turns | try_send + drop-on-full; bounded JetStream timeouts |
| I5 | **OPT-IN DURABILITY** — local-first | Full ARE works offline; NATS amplifies when present |

### 4.2 Cognitive layers

```
L0  Observation Masking       Zero-LLM hide stale tool outputs
L1  Belief Extraction          Tool/user → typed atoms + confidence
L2  Selection-Broadcast        Per-turn codelet competition → LLM prompt
L3  Ecological Consolidation   Background metagraph merge (Goertzel synergy)
L4  Narrative Compact          EMERGENCY ONLY — manual or irrecoverable prose
```

**Thresholds:** L0–L2 activate at **40%** context. L4 at **85%** or `/compact`.

---

## 5. Belief Ecology

### 5.1 Belief atom schema

```rust
struct Belief {
    id: BeliefId,
    belief_type: Invariant | State | Constraint | Surprise | Digest,
    text: String,                    // agent-facing, <50 tokens typical
    confidence: f32,                 // 0.0–1.0, NARS-style
    source: Provenance,              // message_id, turn, tool_call_id, instance
    recover: Option<RecoverCmd>,
    pinned: bool,
    coupling_tags: Vec<String>,      // files, tests, domains
    last_verified_sha: Option<String>,
    fleet_visible: bool,             // publish to goose_beliefs KV
}
```

### 5.2 Belief types

| Type | Source | Broadcast priority | Eviction |
|---|---|---|---|
| `Invariant` | User decision | Always if pinned | Never without unpin |
| `Constraint` | Failed approach | High when relevant | Decay to ghost (~0.2), never delete |
| `Surprise` | Test failure, anomaly | Highest | Until resolved |
| `State` | read_file, git | Medium | Decay on coupling perturbation |
| `Digest` | grep, bash, search | Low | Aggressive decay; recover_cmd kept |

### 5.3 Ecological roles

| Role | Example | Eviction order |
|---|---|---|
| Parasite | 5× redundant read of same file | First |
| Symbiont | Decision + its implementation beliefs | Slow joint decay |
| Keystone | Architecture invariant | Never without user |
| Ghost | Abandoned Redis approach @ 0.2 | Immortal at low confidence |

### 5.4 Selection-broadcast (replaces compaction)

Each turn, codelets compete for a **broadcast budget** (default 40% of context):

```
score = w₁·surprise + w₂·user_recency + w₃·pin + w₄·coupling_delta
      − w₅·confidence_decay − w₆·redundancy + w₇·deutero_bias
```

Winners → structured broadcast block in prompt. Losers → archived (confidence ↓).

**The LLM never sees 200 turns of tool logs.** It sees the cognitive surface required to act correctly *now*.

---

## 6. NATS / JetStream — Distributed Nervous System

### 6.1 Design constraint (Morin dialogic resolution)

**Plane A (hot) must never block turns.** Plane B (durable) must never be required for a turn to succeed.

This mirrors existing goose-plus NATS design: firehose drops on overflow; coord degrades to no-op. ARE extends the pattern — **cognition is local-first; the bus amplifies**.

### 6.2 Plane A — Core NATS (existing + extended)

| Subject | Purpose | Latency |
|---|---|---|
| `{prefix}.{session}.belief.delta` | Belief create/update/decay event | try_send, drop OK |
| `{prefix}.{session}.broadcast.preview` | What would enter workspace | observability |
| `{prefix}.{repo_hash}.belief.shared` | Fleet-visible repo beliefs | cross-instance |
| `{prefix}.coord.*` | File claims (existing) | existing |
| `{prefix}.cmd` / `{session}.reply` | Drive loop (existing) | existing |

Envelope (extends existing `v:1` schema):

```json
{
  "v": 2,
  "type": "belief.delta",
  "seq": 1842,
  "instance": "build-1:4421",
  "ts": "2026-06-27T12:00:00Z",
  "session_id": "019f…",
  "repo_hash": "a3f2c1…",
  "payload": {
    "belief_id": "b-49",
    "op": "create",
    "belief_type": "surprise",
    "confidence": 1.0,
    "text": "tests fail: rate limit missing on /api/v2",
    "coupling_tags": ["src/middleware.ts", "tests/api_test.rs"]
  }
}
```

### 6.3 Plane B — JetStream (new, opt-in)

Enable with `GOOSE_NATS_JS=true` (requires `nats-server -js`).

#### Stream: `GOOSE_BELIEFS`

- **Subjects:** `{prefix}.belief.>`  
- **Retention:** limits (configurable, default 7d) or interest  
- **Purpose:** Event sourcing — replay rebuilds belief ecology after crash  
- **Replicas:** 3 in production for HA  

Every `belief.delta` on Plane A is **async-appended** to JetStream via background task (same channel pattern as firehose).

#### JetStream KV buckets

| Bucket | Key | Value | TTL | Purpose |
|---|---|---|---|---|
| `goose_beliefs` | `{repo_hash}.{belief_id}` | Belief JSON | session + 24h | Fleet-shared belief index |
| `goose_coupling` | `{repo_hash}` | `{sha, dirty_files, test_status}` | 5m heartbeat | Coupling sensor state |
| `goose_deutero` | `{repo_hash}` | Meta-policy TOML | none | Fleet deuterolearning |
| `goose_coord` | `{resource}` | lease (existing) | 60s | File locks (existing) |

**KV Watchers:** Each goose-plus instance watches `goose_coupling` and `goose_beliefs` for its repo — instance B decays beliefs when instance A's write changes sha (Maturana structural coupling at fleet scale).

#### Object Store: `goose_recovery`

- Full tool outputs >8KiB (today truncated in NATS firehose)
- Key: `{session_id}/{tool_call_id}`
- Referenced by `Digest`/`State` beliefs via `recover_ref`
- Agent rehydrates without re-running tool

### 6.4 NATS-enabled workflows

#### Single developer (NATS off)

ARE runs entirely local: SQLite belief store, git coupling sensors. **Zero behavior regression** vs today.

#### Team / fleet (NATS on)

```
Developer A (goose-plus)          NATS / JetStream           Developer B (goose-plus)
        │                              │                              │
        ├─ belief: JWT invariant ─────► goose_beliefs KV ───────────► broadcast includes A's invariant
        ├─ claim src/auth.ts ─────────► goose_coord ─────────────────► B waits, no clobber
        ├─ test surprise ─────────────► goose_coupling ──────────────► B sees red tests, prioritizes
        └─ deutero: auth refactor ────► goose_deutero ───────────────► B inherits pin duration
```

#### Session crash recovery

```
1. New instance starts, session_id known
2. JetStream consumer replays GOOSE_BELIEFS stream from last ack
3. Belief store rebuilt in <1s
4. Broadcast resumes — no compact, no summary handoff
```

### 6.5 Configuration

| Variable | Default | Purpose |
|---|---|---|
| `GOOSE_NATS_URL` | unset | Master enable (existing) |
| `GOOSE_NATS_JS` | `false` | Enable JetStream Plane B |
| `GOOSE_NATS_BELIEFS` | `true` | Publish belief deltas |
| `GOOSE_NATS_BELIEFS_KV` | `true` | Replicate to goose_beliefs KV |
| `GOOSE_NATS_RECOVERY_OS` | `true` | Object Store for large outputs |
| `GOOSE_NATS_COORD` | `false` | File coordination (existing) |
| `GOOSE_NATS_DRIVE` | `false` | Inbound drive (existing) |

---

## 7. goose-plus Integration Points

### 7.1 Existing assets (build on, do not replace)

| Asset | Location | ARE role |
|---|---|---|
| Dual visibility metadata | `MessageMetadata` | Proto-belief hiding (L0) |
| Tool-pair summarization | `context_mgmt/mod.rs` | Upgrade to L1 belief extraction |
| MOIM turn-context | `agents/moim.rs` | Carry broadcast preview + coupling deltas |
| Auto-compact at 80% | `context_mgmt/mod.rs` | Demote to L4 emergency |
| `AfterAgentResponse` hook | `agents/agent.rs` | Belief calibration, deutero signals |
| NATS firehose | `nats/mod.rs` | Plane A belief.delta transport |
| JetStream coord | `nats/coord.rs` | File leases; pattern for belief KV |
| A2A / subagents | `docs/a2a.md` | Subagents publish to shared belief KV |

### 7.2 New modules (planned)

```
crates/goose/src/are/
  mod.rs              # ARE orchestrator
  belief.rs           # atom schema, store, confidence
  broadcast.rs        # selection-broadcast engine
  coupling.rs         # git/test/mtime sensors
  ecology.rs          # parasite/keystone succession
  consolidate.rs      # L3 background synergy
  deutero.rs          # meta-policy learn + apply
  nats_bridge.rs      # Plane A/B integration
```

---

## 8. Requirements

### R1 — Belief Store (P0)

Local SQLite belief table per session. Messages remain provenance; agent prompt assembled from beliefs.

- [ ] CRUD beliefs with confidence, provenance, coupling tags
- [ ] Prompt builder reads beliefs, not full message scrollback
- [ ] User message history UI unchanged

### R2 — Belief Extraction (P0)

Replace `summarize_tool_call()` prose with typed belief emission.

- [ ] `Invariant`, `State`, `Constraint`, `Surprise`, `Digest` per tool pattern
- [ ] `GOOSE_BELIEF_EXTRACTION=true` default on
- [ ] Typical belief <50 tokens

### R3 — Coupling Sensors (P0)

- [ ] Git sha/mtime decay for `State` beliefs
- [ ] Test failure → `Surprise` with `action_required`
- [ ] MOIM reports coupling summary
- [ ] LLM-free, <100ms

### R4 — Selection-Broadcast (P0)

- [ ] Replace middle-out heuristic and 80% auto-compact trigger
- [ ] `GOOSE_AUTO_EVICTION_THRESHOLD=0.4`
- [ ] Deterministic salience (unit-testable without LLM)
- [ ] `GOOSE_AUTO_COMPACT_THRESHOLD=0.85` for L4 only

### R5 — Observation Masking (P0)

- [ ] L0 before L1 every turn
- [ ] Stale read mask via mtime/sha
- [ ] ≥30% token reduction on read-heavy sessions

### R6 — NATS Belief Bridge (P1)

- [ ] `belief.delta` on Plane A (extends firehose)
- [ ] JetStream `GOOSE_BELIEFS` stream async append
- [ ] `goose_beliefs` KV with repo-scoped keys
- [ ] KV watcher decays cross-instance stale beliefs
- [ ] Non-blocking: identical safety guarantees as existing NATS

### R7 — Recovery Object Store (P1)

- [ ] `goose_recovery` Object Store for outputs >8KiB
- [ ] `recover_ref` on beliefs; rehydrate tool in broadcast
- [ ] Firehose truncation remains for telemetry; full blob in Object Store

### R8 — Ecological Consolidation (P1)

- [ ] Background L3 metagraph merge
- [ ] Never touches pinned/invariant beliefs
- [ ] Triggered by belief count or synergy stuckness

### R9 — Deuterolearning (P2)

- [ ] `goose_deutero` KV per repo_hash
- [ ] Learn salience weight biases from session outcomes
- [ ] `.goose/deutero.toml` local mirror for offline

### R10 — Belief Inspector UI (P2)

- [ ] Desktop panel: beliefs by type/confidence
- [ ] Pin/unpin/rehydrate/preview broadcast
- [ ] CLI: `goose-plus session beliefs <id>`
- [ ] NATS live view: `nats sub 'goose.*.belief.>'`

### R11 — Structured Emergency Handoff (P2)

- [ ] L4 compact emits YAML artifact + optional prose
- [ ] JetStream replay + handoff = full session resurrection

### R12 — Fleet Deutero + A2A (P3)

- [ ] Remote A2A agents publish beliefs to shared KV
- [ ] Subagent beliefs merge into parent broadcast with attribution

---

## 9. Implementation Roadmap

### Phase 0 — Local ARE (no NATS required)

**Goal:** Belief broadcast replaces compact as default. Any goose-plus user benefits immediately.

| Deliverable | Requirements |
|---|---|
| Belief store + extraction | R1, R2 |
| Coupling sensors | R3 |
| Selection-broadcast + thresholds | R4 |
| L0 masking | R5 |

**Exit criteria:** 4-hour session benchmark ≥80% task continuation at 50% utilization; zero auto-compact below 85%.

### Phase 1 — NATS Nervous System

**Goal:** Fleet-visible belief ecology. Uncopyable moat activates.

| Deliverable | Requirements |
|---|---|
| Plane A belief.delta | R6 |
| JetStream stream + KV | R6 |
| Object Store recovery | R7 |
| Cross-instance watcher | R6 |

**Exit criteria:** Two instances on same repo share invariants; crash recovery via JetStream replay.

### Phase 2 — Ecology + UX

| Deliverable | Requirements |
|---|---|
| L3 consolidator | R8 |
| Belief Inspector | R10 |
| Structured L4 handoff | R11 |

### Phase 3 — Learning + Fleet Intelligence

| Deliverable | Requirements |
|---|---|
| Deuterolearning | R9 |
| A2A belief merge | R12 |

---

## 10. Success Metrics

| Metric | Industry (compact-based) | ARE target |
|---|---|---|
| Coherence at 50% context | Degraded (context rot) | ≥80% task continuation (LLM-judge) |
| Decision retention after 2hr | ~60% (estimated) | 100% pinned invariants |
| Repeated dead-end approaches | Common | <10% vs baseline (ghost constraints) |
| Auto-compact frequency | Every long session | <5% of sessions |
| Token cost (read-heavy) | 1.0× | ≤0.6× |
| Multi-instance file clobber | Unprotected | Zero with coord + shared beliefs |
| Crash recovery | Summary handoff (lossy) | JetStream replay (lossless beliefs) |
| User corrections honored at hour 4 | Often lost | Pin survives entire session |

---

## 11. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Belief extraction hallucinates wrong beliefs | Provenance required; confidence starts low; coupling verifies `State` |
| NATS/JetStream latency stalls turns | Dual-plane; local-first; try_send; bounded timeouts |
| KV watcher storms on large fleets | Debounce per repo_hash; batch decay |
| Belief store grows unbounded | L3 consolidation; JetStream retention limits; parasite culling |
| Offline / no NATS feature gap | Full ARE local in SQLite; NATS is amplifier only |
| L4 still needed occasionally | Structured handoff; user-initiated `/compact` preserved |

---

## 12. Configuration Reference

| Variable | Default | Layer |
|---|---|---|
| `GOOSE_BELIEF_EXTRACTION` | `true` | L1 |
| `GOOSE_CONTEXT_L0_MASKING` | `true` | L0 |
| `GOOSE_AUTO_EVICTION_THRESHOLD` | `0.4` | L2 trigger |
| `GOOSE_AUTO_COMPACT_THRESHOLD` | `0.85` | L4 trigger |
| `GOOSE_BROADCAST_BUDGET` | `0.4` | L2 budget |
| `GOOSE_COUPLING_SENSORS` | `true` | Coupling |
| `GOOSE_SYNERGY_CONSOLIDATE` | `true` | L3 |
| `GOOSE_DEUTERO_LEARN` | `true` | Deutero |
| `GOOSE_NATS_JS` | `false` | Plane B |
| `GOOSE_NATS_BELIEFS` | `true` | Plane A |

---

## 13. References

### Philosophy & cognition

- [Morin — pensée complexe face à l'IA](https://codexnumeris.org/46-cultiver-la-pensee-complexe/)
- [Goertzel — Cognitive Synergy (arXiv:1703.04361)](https://arxiv.org/pdf/1703.04361)
- [Friston — Free Energy Principle](https://en.wikipedia.org/wiki/Free_energy_principle)
- [Pei Wang — NARS](https://cis.temple.edu/~pwang/NARS-Intro.html)
- [LIDA / Global Workspace](https://aaai.org/papers/0011-fs07-01-011-%EF%80%A0lida-a-computational-model-of-global-workspace-theory-and-developmental-learning/)
- [Vervaeke — Relevance Realization](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2024.1362658/full)
- [Maturana — Structural Coupling](https://reflexus.org/wp-content/uploads/Autopoiesis-structural-coupling-and-cognition.pdf)

### Context rot & competitors

- [Chroma — Context Rot](https://research.trychroma.com/context-rot)
- [OpenCode #10016](https://github.com/anomalyco/opencode/issues/10016)
- [Justin3go — Compaction comparison](https://justin3go.com/en/posts/2026/04/09-context-compaction-in-codex-claude-code-and-opencode)

### NATS

- [JetStream concepts](https://docs.nats.io/nats-concepts/jetstream)
- [JetStream KV walkthrough](https://docs.nats.io/nats-concepts/jetstream/key-value-store/kv_walkthrough)
- [goose-plus NATS docs](docs/nats.md)

### goose-plus source

- `crates/goose/src/context_mgmt/mod.rs`
- `crates/goose/src/agents/moim.rs`
- `crates/goose/src/agents/agent.rs`
- `crates/goose/src/nats/mod.rs`
- `crates/goose/src/nats/coord.rs`

---

## 14. Summary

**goose-plus + ARE** is not a better compact button. It is a different species of coding agent:

- **Beliefs**, not transcripts.
- **Broadcast**, not scrollback.
- **Coupling**, not timers.
- **Ecology**, not deletion.
- **NATS**, not isolation.
- **Deuterolearning**, not amnesia.

Competitors will keep compacting because it is easy to ship. goose-plus will keep **remembering why** — and the code it writes at hour four will still match the decisions made at minute four.

*The repo is the body. Beliefs are the nervous system. NATS is how the fleet shares a mind. The context window is only what demands action right now.*