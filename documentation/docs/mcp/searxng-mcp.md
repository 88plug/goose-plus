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
```

If you only want your own instance without the free pool:
```bash
export SEARXNG_MCP_USE_FREE_PROVIDERS=false
```

### How It Works (Technical)

- `client.py` maintains clients for both your configured backend(s) **and** the entire free pool.
- `search_parallel()` fires the identical query to all of them concurrently using `asyncio` + semaphore.
- Every backend uses `_try_search_one()`:
  1. Attempt `format=json`
  2. On 403/429/non-JSON/etc → automatically fetch raw HTML and parse it with the built-in HTML results parser.
- Successful payloads are turned into `QueryOutcome`s.
- `service.py` calls the existing powerful `_merge_query_outcomes()` (same logic used by `search_many` / `research`).
- Health and structured output report how many free backends were used.

### HTML Fallback (Why It Matters)

Many public instances disable or block `format=json`. The HTML parser (`html_search.py`) understands the standard "simple" SearXNG theme and produces a payload shape that satisfies the MCP client's validation.

### Usage from goose

You can run searxng-mcp as a stdio MCP server:

```bash
uvx searxng-mcp
# or with explicit config
SEARXNG_MCP_USE_FREE_PROVIDERS=true SEARXNG_MCP_FREE_PARALLEL=true uvx searxng-mcp
```

Then in goose you can use tools like `search`, `search_many`, `research`, etc. provided by the searxng-mcp server. Because of the parallel free pool, even a basic `search` call becomes a multi-instance aggregated search.

### Demo

See `docs/searxng-mcp/demo_parallel_free.py` (in the goose-plus repo) for a standalone demonstration of the parallel + merge behavior.

### Comparison to Other Search MCPs

- Brave, Tavily, Exa, etc.: single commercial backend, rate-limited, paid.
- This: many independent public metasearchers + your own instance(s), all hit in parallel, merged, free/privacy-respecting by default.

The parallel free provider design is the differentiator that makes searxng-mcp the most powerful search surface available to goose.

### Self-Hosting Recommendation

For maximum reliability and to avoid public instance bot protection, run your own SearXNG (Docker: `searxng/searxng`) and point `SEARXNG_MCP_BASE_URL` at it. The free public pool can still be kept enabled in parallel for extra breadth.

### Files (Reference Implementation)

The core changes live in the 88plug/searxng-mcp repository:

- `src/searxng_mcp/settings.py` — free provider defaults + flags
- `src/searxng_mcp/client.py` — `search_parallel()`, `_try_search_one()` (JSON + HTML)
- `src/searxng_mcp/service.py` — parallel path inside `search()`, health reporting
- `src/searxng_mcp/html_search.py` — HTML results parser

Copies of the key files from the implementation are also kept under `docs/searxng-mcp/` in this repo for reference.

---

**This is what "search" should feel like in 2026: many independent sources, all at once, intelligently combined.**