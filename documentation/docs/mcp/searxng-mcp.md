# SearXNG MCP

**searxng-mcp** gives goose privacy-respecting, metasearch capabilities by connecting to one or more [SearXNG](https://searxng.github.io/searxng/) instances.

## The Secret Sauce: Full Parallel Free Public Providers

What makes this implementation uniquely powerful in the world:

- **All configured free providers are **always** used in **full parallel (no limit whatsoever)** — "all always".**
- When enabled (the default), a single query is fanned out to **all configured free SearXNG instances at the same time** (true concurrent/parallel execution).
- Each backend tries `format=json` first, then automatically falls back to parsing the HTML results page.
- All successful result sets are intelligently **merged** (deduplication by URL, engine aggregation, hit boosting, score merging).
- You get a dramatically richer, more diverse set of results than any single instance — or any sequential "fallback" approach — can provide.

This parallel-everything design is the killer feature.

### Default Free Public Providers (Pre-wired for Parallel)

These 8 high-coverage public instances are enabled out of the box:

1. https://searx.tiekoetter.com (highest engine count ~272)
2. https://baresearch.org
3. https://search.2b9t.xyz
4. https://search.abohiccups.com
5. https://searxng.site
6. https://failsearx.culturanerd.it
7. https://searx.prvcy.eu
8. https://search.bladerunn.in

**Behavior**: All are queried **concurrently** on every search (no limit — all always in full parallel). Results are merged.

### A2A / ACP: Smarter and Faster Results Over the Wire

The parallel free provider design shines brightest when goose is used as an A2A agent or via ACP clients.

- **Dedicated skill**: `searxng_parallel_search`
  - Advertised on the Agent Card when goose is exposed over A2A.
  - Remote agents can target this skill directly for fast, privacy-first metasearch without requiring a full LLM turn inside goose.

- **Direct fast path (A2A)**:
  - Send a message prefixed with `searxng:`, `searx `, or `search:` (e.g. `searxng: best open source ai agents 2026`).
  - goose runs the query against **all 8 free providers in full parallel immediately**.
  - Partial merged results are streamed back as `Working` TaskStatusUpdateEvents as soon as the fastest providers reply.
  - The terminal `Completed` task contains the full merged set.

- **Incremental streaming**:
  - The Rust implementation (built into goose) exposes `parallel_search_stream()`.
  - Each `SearchUpdate` carries the current merged snapshot + which backend just contributed.
  - This gives agents the lowest possible time-to-first-useful-result.

- **MCP resources** (when using searxng as an MCP server):
  - `searxng://free-providers` — the current list of 8 + parallel mode flags.
  - `searxng://status` — live configuration and merge strategy.

- **ACP clients**:
  - Tool calls to `searxng_search` (or the direct A2A path) surface progress via tool notifications when bridged by goose.
  - The same merged results and engine aggregation are available.

This combination — full parallel to 8 free providers + incremental wire delivery over A2A/ACP — makes searxng-mcp the most powerful search surface for agent-to-agent and client-to-agent scenarios.

### Configuration (Environment Variables)

```bash
# Enable/disable the free public pool (default: true)
export SEARXNG_MCP_USE_FREE_PROVIDERS=true

# Full parallel mode — the secret sauce (default: true)
export SEARXNG_MCP_FREE_PARALLEL=true

# No limit — all configured free providers are always used in full parallel
# (no SEARXNG_MCP_FREE_MAX_CONCURRENCY needed — always full parallel for free providers)

# Optionally override the exact list (comma-separated)
export SEARXNG_MCP_FREE_PROVIDERS="https://searx.tiekoetter.com,https://baresearch.org,..."

# Your own instance (still works; free pool is added in parallel by default)
export SEARXNG_MCP_BASE_URL=http://127.0.0.1:8890

# Optional FlareSolverr (rendered Chrome proxy) as a last-resort per-backend fallback
# Only used for providers that fail direct JSON + direct HTML (e.g. heavy Cloudflare protection).
# Does not block other parallel providers — runs independently per-backend.
# Use only when you need to reach additional "dead" public instances.
# Set this to the base URL of a running FlareSolverr instance (default port 8191).
export SEARXNG_FLARESOLVERR_URL=http://localhost:8191
```

If you only want your own instance without the free pool:
```bash
export SEARXNG_MCP_USE_FREE_PROVIDERS=false
```

### How It Works (Technical)

- The built-in Rust searxng server (`crates/goose-mcp/src/searxng/`) implements the same logic as the reference Python version.
- `parallel_search_stream()` fans the query out to all targets concurrently.
- Every backend uses fast-fail: JSON first, then HTML fallback.
- Optional third path (only when `SEARXNG_FLARESOLVERR_URL` is set): FlareSolverr rendered fetch as a last-resort for individual backends that fail the first two paths (e.g. heavy Cloudflare protection). This path is per-backend only and does not block the other providers running in parallel.
- Results are merged on the fly; updates are emitted after each provider responds.
- The non-streaming `searxng_search` tool still works for normal in-turn tool use.

### HTML Fallback (Why It Matters)

Many public instances disable or block `format=json`. The HTML parser understands the standard "simple" SearXNG theme and produces a payload shape that satisfies validation and merge logic.

### Optional Rendered Fallback via FlareSolverr

When you set `SEARXNG_FLARESOLVERR_URL`, searxng-mcp gains a **third, last-resort path** for individual backends that fail both direct JSON and direct HTML fetches (typically due to Cloudflare / Turnstile protection).

- It is **per-backend only** — a slow solve for one provider never blocks the other providers running in the same parallel query.
- It is **only attempted** after the two fast paths return no usable results for that specific backend.
- Results coming through this path are tagged with engine `flaresolverr`.
- Because FlareSolverr launches real Chrome, solves are much slower (seconds to tens of seconds) and heavier than direct calls. This path is intended for the providers that "need it" (many entries in `DEAD_PROVIDERS.md`, or the 8 when they are temporarily heavily protected).
- Keep the core "8 free providers in full parallel, no limit" behavior on the fast paths.

See `docs/searxng-mcp/FLARESOLVERR_RESEARCH.md` for live measurements, trade-offs, and recommendations.

Usage example (point at a running FlareSolverr instance):

```bash
export SEARXNG_FLARESOLVERR_URL=http://localhost:8191
# now searxng_search (or searxng: queries over A2A) will try FS for any backend that needs it
```

### Usage from goose

You can run searxng-mcp as a stdio MCP server:

```bash
uvx searxng-mcp
# or with explicit config
SEARXNG_MCP_USE_FREE_PROVIDERS=true SEARXNG_MCP_FREE_PARALLEL=true uvx searxng-mcp
```

Then in goose you can use tools like `search`, `search_many`, `research`, etc. provided by the searxng-mcp server. Because of the parallel free pool, even a basic `search` call becomes a multi-instance aggregated search.

For direct agent-to-agent use, enable A2A on goose (`GOOSE_A2A_ENABLE=true`) and call the `searxng_parallel_search` skill or use a prefixed query.

### Demo

See `docs/searxng-mcp/demo_parallel_free.py` (in the goose-plus repo) for a standalone demonstration of the parallel + merge behavior.

See the A2A section in the documentation for examples of calling goose directly for searxng results.

### Using the Optional FlareSolverr Fallback

If you have FlareSolverr running (e.g. `docker run -p 8191:8191 -d flaresolverr/flaresolverr`), you can enable the last-resort rendered path:

```bash
export SEARXNG_FLARESOLVERR_URL=http://localhost:8191
```

With this set, any of the configured providers (the default 8 or your own list) that return no usable results via direct JSON or HTML will automatically try FlareSolverr for that backend only. The other providers continue in full parallel without waiting.

This is the recommended way to "revive" many of the historically dead public instances listed in `docs/searxng-mcp/DEAD_PROVIDERS.md` without changing the fast-path behavior for the healthy ones.

See `docs/searxng-mcp/FLARESOLVERR_RESEARCH.md` for real-world timing and resource measurements that explain why this must remain optional and last-resort.

### Comparison to Other Search MCPs

- Brave, Tavily, Exa, etc.: single commercial backend, rate-limited, paid.
- This: many independent public metasearchers + your own instance(s), all hit in parallel, merged, free/privacy-respecting by default.
- Over A2A/ACP: incremental results delivered to peer agents as fast as the quickest backends respond.

The parallel free provider design + streaming wire path is the differentiator that makes searxng-mcp the most powerful search surface available to goose.

### Self-Hosting Recommendation

For maximum reliability and to avoid public instance bot protection, run your own SearXNG (Docker: `searxng/searxng`) and point `SEARXNG_MCP_BASE_URL` at it. The free public pool can still be kept enabled in parallel for extra breadth.

### Files (Reference Implementation)

The core changes live in the 88plug/searxng-mcp repository (Python reference) and are mirrored in goose's built-in:

- `src/searxng_mcp/settings.py` — free provider defaults + flags
- `src/searxng_mcp/client.py` — `search_parallel()`, `_try_search_one()` (JSON + HTML)
- `src/searxng_mcp/service.py` — parallel path inside `search()`, health reporting
- `src/searxng_mcp/html_search.py` — HTML results parser

In goose:
- `crates/goose-mcp/src/searxng/` — production Rust implementation with streaming and A2A fast path.
- `crates/goose/src/a2a/mod.rs` — skill advertisement.
- `crates/goose-server/src/routes/a2a.rs` — direct parallel execution for prefixed queries and the skill.

Copies of the key files from the implementation are also kept under `docs/searxng-mcp/` in this repo for reference.

---

**This is what "search" should feel like in 2026: many independent sources, all at once, intelligently combined — and delivered incrementally to other agents over the wire.**