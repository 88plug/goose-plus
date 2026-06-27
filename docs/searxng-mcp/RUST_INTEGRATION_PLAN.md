# Native Rust SearXNG Support in goose (goose-mcp)

**Goal**: Bring the "secret sauce" (full parallel free public SearXNG providers + fast-fail + HTML fallback + merge) natively into the Rust goose codebase so goose can use it without an external Python process.

Date: 2026-06-27

## Current State (Python reference)
- 17 sources discovered from session logs.
- 8 pre-wired by default in `DEFAULT_FREE_PROVIDERS`.
- Full concurrent parallel via `search_parallel()`.
- Per-backend fast-fail (`_should_fallback` on 403/429/non-JSON/malformed).
- Automatic JSON → HTML fallback using the simple theme parser.
- Smart merge (`_merge_query_outcomes`).
- Default-on power mode.
- Demonstrated: using all 17 is safe because fast-fails are near-instant.

## Why Native Rust Matters for goose
- No external `uvx searxng-mcp` dependency.
- Tighter integration (same process, better observability, auth, caching).
- Can be exposed as a built-in extension (`builtin!(searxng, SearxngServer)`).
- Can participate in the same tool routing / context as other goose MCPs.
- Can use goose's existing reqwest + tokio stack.

## Proposed Crate Location
`crates/goose-mcp/src/searxng/`

Files (minimal first pass):
- `mod.rs` — SearxngServer (rmcp ServerHandler)
- `client.rs` — Parallel SearXNG client (reqwest + tokio)
- `html.rs` — Minimal HTML results parser (use scraper or quick-xml + regex, or html5ever if added)
- `merge.rs` — Result dedup + merge logic (port of Python's MergedHit)
- `settings.rs` or use goose config

Or start simpler: a single module + tools exposed via an existing server (e.g. add web search tools to computercontroller or a new lightweight one).

## Dependencies to Consider Adding
- `reqwest` (already in workspace + goose-mcp)
- HTML parsing: `scraper` (CSS selectors, easy) or `lol_html` / `kuchiki` / `quick-xml`
- `serde` + `serde_json` (already present)
- `tokio` + `futures` for parallel fan-out + semaphore
- Optional: `url`, `tracing`

Current goose-mcp already pulls reqwest.

## Core Behaviors to Port
1. Configurable list of free providers (default to the 8 or all 17).
2. `search_parallel(query, ...)` → hit many instances concurrently.
3. Per-request: try `?format=json`, on failure try raw HTML + parse.
4. Fast-fail: drop 4xx/5xx/non-JSON quickly, don't wait for them.
5. Merge results across backends (by canonical URL, collect engines, boost score by hit count).
6. Expose tools:
   - `searxng_search`
   - `searxng_search_many` (fan-out queries)
   - `searxng_research` (multi-step using parallel)
   - Optional: `searxng_fetch` (reuse existing fetch or computercontroller web_scrape)

7. Health / config resource: list active free providers + stats.

## Fast-Fail is the Enabler
Because of aggressive fast-fail, it is safe and efficient to enable **all 17** discovered sources by default (or a larger curated set). Dead or slow ones are ignored almost immediately.

Demo evidence (Python reference, same logic):
- Using all 17: wall time ~62 ms when only 3 succeeded (fast-fails in ~10 ms).

## Migration / Dual Mode Options
- Keep Python searxng-mcp as an optional external extension.
- Add native Rust version as a builtin (`searxng` extension).
- Allow users to prefer native vs external via config.

## Implementation Phases (suggested)

### Phase 1 (small)
- Add `searxng` module skeleton in goose-mcp.
- Implement a simple parallel client that does JSON + basic HTML fallback.
- Hardcode or env-config the 8 default free providers.
- Expose one tool: `searx_search(q: string) -> results`.
- Use reqwest directly, collect successes, naive merge by URL.

### Phase 2
- Proper HTML parser (scraper crate or vendored simple theme logic).
- Full merge logic (engines list, hit counts, scores).
- Support for categories, engines, safesearch, etc.
- Parallel across user-provided base_url + free pool.

### Phase 3
- Add to BUILTIN_EXTENSIONS.
- Expose resources (`searxng://providers`, `searxng://config`).
- Caching, rate limiting awareness, user-agent rotation.
- Integration with goose's existing fetch/render if needed.
- Tests + self-test recipe update.

### Phase 4 (stretch)
- Use goose's RenderedFetchClient equivalent (Playwright via computercontroller or new browser MCP) for the most protected instances.
- Native result ranking / re-ranking.

## Open Questions
- Should the native implementation live in `goose-mcp` or a new `goose-searx` crate?
- Do we want to depend on a full scraper crate, or keep a very small custom parser (like the Python one)?
- How to surface "which backends contributed" in the final tool output (important for the parallel value prop)?

## References
- Python reference implementation: `docs/searxng-mcp/{client.py,service.py,html_search.py}`
- Docs: `documentation/docs/mcp/searxng-mcp.md`
- All 17 sources: `docs/searxng-mcp/SOURCES.md`
- Capability proof: `docs/searxng-mcp/CAPABILITY_SUMMARY.md`

This plan turns the Python prototype into first-class goose capability.
