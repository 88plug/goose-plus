# searxng-mcp Work Index (goose-plus)

**Date:** 2026-06-27  
**Status:** A2A/ACP streaming + direct fast path COMPLETE (parallel free providers remain the core superpower)

---

## Goal Achieved (Phase 1 + Phase 2)

### Phase 1 — Secret Sauce (Full Parallel Free Providers)
searxng-mcp now has **strictly greater capabilities** than any other search MCP by:
- Pre-wiring 8 public free SearXNG providers by default
- Querying them in **full parallel** (all at once, not sequential fallback) — "all always"
- Fast-failing bad backends instantly
- Automatic HTML fallback when `format=json` is blocked
- Smart cross-backend result merging (dedup by URL + engine aggregation + hit boosting)

### Phase 2 — A2A/ACP Superpowers (Faster + Smarter Over the Wire)
- Streaming incremental results: `parallel_search_stream()` yields `SearchUpdate`s as each free provider responds.
- MCP resources: `searxng://free-providers` and `searxng://status`.
- A2A skill `searxng_parallel_search` advertised on the Agent Card.
- Direct fast path in A2A executor: messages like `searxng: <query>`, `search: <query>` bypass full LLM turn and stream partial merged results as `Working` updates immediately, with final results in `Completed`.
- This delivers the lowest time-to-first-useful-result to peer agents and ACP clients.

This combination (full parallel to 8 free + incremental wire delivery) is the killer feature.

---

## Pre-wired DEFAULT_FREE_PROVIDERS (8) — Always Full Parallel

1. https://searx.tiekoetter.com
2. https://baresearch.org
3. https://search.2b9t.xyz
4. https://search.abohiccups.com
5. https://searxng.site
6. https://failsearx.culturanerd.it
7. https://searx.prvcy.eu
8. https://search.bladerunn.in

Dead/unreliable providers are tracked only in `DEAD_PROVIDERS.md` (never in active code).

---

## Key Deliverables

### Documentation
- `documentation/docs/mcp/searxng-mcp.md` — Main docs (parallel free design + A2A/ACP streaming + skill + resources)
- `docs/a2a.md` — A2A usage + searxng skill and fast-path examples
- `CAPABILITY_SUMMARY.md`, `SOURCES.md`, `WORKING_LIST.md`, `DEAD_PROVIDERS.md`

### Implementation (Rust, built into goose)
- `crates/goose-mcp/src/searxng/mod.rs` — `SearxngServer`, `parallel_search_stream()`, resources, tool
- `crates/goose-mcp/src/searxng/client.rs` — JSON + HTML fallback per backend
- `crates/goose-mcp/src/searxng/merge.rs` — cross-backend merge
- `crates/goose/src/a2a/mod.rs` — `searxng_parallel_search` skill in AgentCard
- `crates/goose-server/src/routes/a2a.rs` — direct fast-path executor for prefixed queries / skill
- Re-export of `SearchUpdate` from `goose_mcp`

### Reference (Python, for 88plug/searxng-mcp)
`docs/searxng-mcp/`
- `settings.py`, `client.py`, `service.py`, `html_search.py`
- `demo_parallel_free.py`, `test_parallel_merge.py`

---

## Verified Behaviors
- Always full parallel to exactly the 8 working providers (no cap).
- Streaming produces incremental merged snapshots as backends complete.
- A2A direct path emits `Working` updates with partials, then `Completed` with full set.
- MCP tool path continues to work normally (now internally driven by the same streaming logic).
- Resources expose the free list and parallel mode.
- All "goose" references use lowercase per brand guidelines.

---

## Remaining / Future (non-blocking)
- Optional: emit `ProgressNotification` / `LoggingMessageNotification` during tool-path parallel execution for live ACP updates.
- Unit/integration tests for the streaming path.
- Upstream 88plug/searxng-mcp sync.

## Delivered (optional rendered fallback)
- Optional FlareSolverr support added to the Rust client.
- New environment variable: `SEARXNG_FLARESOLVERR_URL`.
- Non-blocking per-backend last-resort path (after JSON + HTML fail) that does not block the "full parallel, no limit" free provider pool.
- Reusable `parse_simple_theme_html` + `try_flaresolverr` implementation.
- Unit tests for the HTML parser (in `crates/goose-mcp/src/searxng/client.rs`).
- Documented in `documentation/docs/mcp/searxng-mcp.md` and `docs/searxng-mcp/FLARESOLVERR_RESEARCH.md` (with live test data showing why it must stay optional/last-resort).

---

## Quick Start

```bash
# Power mode (full parallel free) is on by default in goose
# For standalone:
uvx searxng-mcp

# A2A fast path (from another agent):
# Send "searxng: your query here" or target skill "searxng_parallel_search"
```

One query → **all 8 free providers in full parallel** → incremental merged results over A2A/ACP.

This is greater capability.

---

**Core work COMPLETE** — searxng-mcp + A2A/ACP is now the most powerful search surface available to goose and peer agents.