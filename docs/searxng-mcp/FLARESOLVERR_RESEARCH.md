# FlareSolverr Research for searxng-mcp (2026-06-27)

## What it is
FlareSolverr is a **proxy server** that uses a real (headless) Chrome + undetected-chromedriver + Selenium to solve Cloudflare (and some DDoS-GUARD / Turnstile) challenges.

- It waits for "cf_clearance" (and similar) cookies to appear.
- It returns the final HTML + the full cookie jar (especially `cf_clearance`).
- Downstream HTTP clients can then replay those cookies + the same User-Agent to bypass the protection on normal `requests` / `reqwest` calls.

Current latest (as of this research): **v3.5.0** (2026-05).

Docker is the recommended way (it bundles everything). Default port **8191**.

## Core API (relevant to us)
POST http://localhost:8191/v1

### request.get (the one we care about)
```json
{
  "cmd": "request.get",
  "url": "https://searx.tiekoetter.com/search?q=foo&format=json",
  "maxTimeout": 60000,
  "cookies": [...optional...],
  "returnOnlyCookies": false,
  "returnScreenshot": false,
  "proxy": {...optional...},
  "waitInSeconds": 0,
  "disableMedia": false,
  "session": "optional-permanent-session-id"
}
```

Response shape (important fields):
```json
{
  "solution": {
    "url": "...",
    "status": 200,
    "headers": {...},
    "response": "<html>...",
    "cookies": [ {"name": "cf_clearance", "value": "...", "domain": "...", ...}, ... ],
    "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ..."
  },
  "status": "ok"
}
```

You get:
- The **final rendered HTML** after challenges.
- The **cookies** you must replay (especially `cf_clearance`).
- The exact **User-Agent** the browser used (must match on your HTTP client or CF will re-challenge).

There is also `request.post`, sessions management (`sessions.create` / `destroy`), etc.

## Why many public SearXNG instances are blocked today
From our earlier work:
- Lots of public instances (including some we tested) return 403, "attention required", or HTML "checking your browser" pages even for `format=json`.
- The current searxng-mcp Rust client does **JSON first**, then a very crude **static HTML scraper** (looking for `<article class="result">` etc.).
- The crude scraper fails on heavily protected instances that require a real browser + cookies.

This is exactly the class of problem FlareSolverr was built to solve.

## How it would help searxng-mcp

### 1. As a "rendered fetch" backend (biggest win)
We could add a third path in the client:

1. `format=json` (fast, preferred)
2. plain HTML GET + our BS4-ish parser (current fallback)
3. **FlareSolverr path** (when the above two get 403 / captcha / challenge HTML)

For step 3:
- Call FlareSolverr `request.get` for the SearXNG search URL.
- Take the `solution.response` (HTML) + `solution.cookies`.
- Feed the HTML into a proper parser (the same one we already have or a better one).
- Optionally store the `cf_clearance` cookie + userAgent for a short time so follow-up requests to the same instance can reuse it without spinning up another browser.

This would let us successfully talk to several of the "dead" instances we put in `DEAD_PROVIDERS.md` (the ones that are just Cloudflare-protected but otherwise healthy).

### 2. Cookie + UA replay without keeping a browser around
FlareSolverr is intentionally **not** a permanent proxy for every request. The pattern is:
- Use FlareSolverr once to get cookies + UA.
- Replay those cookies + UA with a normal lightweight HTTP client (reqwest) for the next N minutes/hits.
- When the clearance expires or you get challenged again → ask FlareSolverr again.

This keeps resource usage reasonable (each FlareSolverr request spins up a Chrome).

### 3. Sessions (optional optimization)
You can create a named session in FlareSolverr so that multiple queries to the same SearXNG instance reuse the same browser context (cookies persist inside that browser). You then `sessions.destroy` when done. This can be faster than launching a fresh browser every time, but costs more RAM.

## Trade-offs / Costs of baking it in

**Pros**
- Unlocks many currently "dead" public SearXNG instances that are just CF-protected.
- Makes the "full parallel free providers" story even stronger (more of the 19 historical ones become usable).
- Gives us a real rendered HTML path (better than our current crude scraper) for the worst cases.
- Already a very mature, Docker-friendly project.

**Cons / Realities**
- **Heavy**. Each solve launches Chrome. Docs explicitly warn: "Web browsers consume a lot of memory. ... do not make many requests at once."
- **Latency**. 5-30+ seconds per solve in bad cases (vs <1s for direct JSON/HTML).
- **Not "always on" for 8 parallel free providers**. If you fan out to all 8 at once and several of them need FlareSolverr, you will spin up multiple Chrome instances. This is exactly the scenario they tell you not to do on low-RAM machines.
- **Operational surface**. You now have to run (or connect to) a FlareSolverr instance. Docker is easy, but it's another moving part.
- **Legal / ToS**. Using this to hammer public instances harder than they intend is still rude and can get the instance operators mad or the IP ranges banned.
- **Maintenance**. FlareSolverr has to keep up with Cloudflare changes (they do releases for this). If it lags, our "rendered" path will also lag.

## How we could integrate it (high level options)

A. **Optional companion** (recommended first step)
   - Add a `SEARXNG_FLARESOLVERR_URL=http://flaresolverr:8191` env var.
   - In the Rust client, when JSON + plain HTML both fail with 403/challenge markers, optionally call out to FlareSolverr if the URL is configured.
   - Keep the "8 free providers in full parallel" behavior, but treat FlareSolverr as a last-resort per-backend fallback (maybe with a much higher timeout and lower concurrency).
   - Document clearly that enabling FlareSolverr increases resource usage and latency.

B. **Baked-in "rendered" mode for the free pool**
   - Make FlareSolverr a first-class render path (similar to what the Python reference `browser.py` / `RenderedFetchClient` was heading toward).
   - Still keep the fast JSON/HTML paths as primary.
   - For the default free list, only use rendered path on the ones we know are heavily protected (or on demand).

C. **Per-instance "needs rendered" flag**
   - Extend settings so each free provider (or user-added backend) can be marked `requires_render: true`.
   - The parallel fan-out logic would then route those through FlareSolverr while others stay direct.

D. **Cookie cache + replay layer**
   - After a successful FlareSolverr solve for a domain, cache the relevant cookies + UA for a short TTL (e.g. 10-30 min).
   - Subsequent direct requests to the same SearXNG instance can attach those cookies + UA, avoiding repeated browser solves.
   - This is the "secret sauce" way to use FlareSolverr without destroying your machine.

## Relation to existing work in goose-plus

From our docs:
- We already have a note in `FINAL_SUMMARY.md`, `INDEX.md`, and `RUST_INTEGRATION_PLAN.md` that says:
  > "Rendered (Playwright) search path for heavily protected instances" is future work.
- The Python reference (`docs/searxng-mcp/browser.py`, `service.py`) already had some `RenderedFetchClient` concept.
- The current Rust searxng-mcp only has a very basic static HTML scraper.

FlareSolverr would be one concrete way to deliver the "rendered fallback" we already planned, without writing our own full browser automation.

## Practical recommendation for "bake it in"

**Don't** make FlareSolverr a hard dependency or spin up Chrome for every one of the 8 free providers in parallel by default.

**Do**:
- Add **optional** FlareSolverr support behind an env/config flag.
- Use it as a **per-backend fallback** when the two fast paths (JSON + simple HTML) clearly hit protection.
- Add a small cookie/UA cache so one solve can serve multiple queries.
- Keep the "all 8 in full parallel, no limit" behavior for the fast paths.
- Clearly document the resource/latency impact.
- Consider exposing a resource or health field that reports "N backends required rendered fallback in the last hour".

This way we get the power (more of the historical 19 become usable) without turning the default free parallel experience into a Chrome farm.

## Quick integration sketch (Rust side)

```rust
// pseudocode
async fn try_rendered_via_flaresolverr(
    fs_url: &str,
    search_url: &str,
) -> Option<String> {
    let body = json!({
        "cmd": "request.get",
        "url": search_url,
        "maxTimeout": 90000,
        "returnOnlyCookies": false
    });
    let resp: Value = client.post(fs_url).json(&body).send().await?.json().await?;
    let html = resp["solution"]["response"].as_str()?;
    let ua = resp["solution"]["userAgent"].as_str();
    // optionally extract cf_clearance etc. and cache them
    Some(html.to_string())
}
```

Then in the existing `search_one` / fallback logic:
- if json fails with 403/challenge-html
- and plain html also looks like a challenge page
- and flaresolverr_url is configured
- then call the above and parse the returned HTML with our existing (or improved) parser.

## Bottom line

Yes, FlareSolverr would meaningfully help us reach more of the "dead" public SearXNG instances.

It is **not** free — it costs RAM, CPU, and latency per solve.

The right way to "bake it in" is as an **optional, last-resort, cached** rendered fetch path, not as the default way to talk to the 8 free providers.

This aligns with the "future work" items we already wrote down (Playwright / rendered path) and gives us a concrete, battle-tested implementation to lean on.

## Live Empirical Test Results (2026-06-27)

We ran real tests with a live FlareSolverr container (v3.5.0) against the exact 8 DEFAULT_FREE_PROVIDERS, using the "full parallel, no limit" pattern that defines the searxng-mcp secret sauce.

### Test Setup
- FlareSolverr started fresh via Docker on localhost:8191.
- Query examples: "rust programming language", "ai agents", etc.
- Every backend routed through `request.get` (HTML path, no format=json).
- Full parallel fan-out (all 8 at the same time, matching `Semaphore(targets.len())` / "all always").
- Compared against direct `curl` (JSON and HTML paths).

### Key Measurements

**Full parallel through FlareSolverr (all 8):**
- Wall time: **~26.8 seconds**
- All 8 solved (no transport failures)
- 6/8 contained usable result markers
- Slowest single backend: ~26.8s
- Fastest: ~3.5s

**Per-backend examples (one run):**
- search.2b9t.xyz: 3.5s, 200, no result markers
- searx.tiekoetter.com: 3.6s, 200, no result markers
- baresearch.org: 3.7s, 200, has results
- search.abohiccups.com: 4.0s, 200, has results
- search.bladerunn.in: 7.6s, 200, has results
- searx.prvcy.eu: 7.9s, 200, has results
- failsearx.culturanerd.it: 16.8s, 200, has results
- searxng.site: 26.8s, 200, has results

**Direct (no FlareSolverr) on the same providers:**
- Many direct calls: 0.1s – 0.35s (even when returning 429 "Too Many Requests")
- Direct HTML on some: sub-200ms to get the challenge page or rate-limit response
- Direct is dramatically faster when it works, and fails fast when it doesn't.

**Resource usage (FlareSolverr container):**
- Idle: ~50–145 MiB RAM, near 0% CPU
- During full parallel 8 solves: spiked to **~560 MiB RAM**, CPU 10–40%+ during the burst
- After: settled back to ~150–160 MiB

**Sequential vs parallel through FS (3 providers sample):**
- Sequential wall time: ~27.5s
- Parallel wall time: ~3.1s
- Parallel wins on wall time, but absolute latency is still 10x+ a good direct call.

**Quality observations:**
- Some providers return HTTP 200 via FS but still serve no real results (or very little HTML).
- Direct 429s sometimes happened faster than FS could even start rendering.
- One provider (search.2b9t.xyz) was consistently poor for result extraction even via FS in these tests.

### Direct Answer to "Wire everything through FlareSolverr — no downside, right?"

**No.** There are major downsides:

1. **Latency explosion** — The "fast parallel free" experience (sub-second to low seconds for first useful results) becomes 3–27 seconds wall time. The streaming / A2A fast-path (`parallel_search_stream`, Working updates) would be crippled because every provider is now slow.

2. **Resource cost** — Spinning 8 Chrome instances (even briefly) is heavy. The design explicitly says "query all of them in full parallel (no concurrency limit whatsoever)". That directly contradicts FlareSolverr's own guidance ("do not make many requests at once").

3. **Wall time gated by slowest** — One slow provider tanks the entire parallel query. In the current design we rely on fast-fail + merge-as-they-arrive. With FS everywhere, you lose that.

4. **Not a silver bullet for quality** — Some backends still return 200 + little/no usable search results. Protection can be deeper than what FS solves in one shot, or the instance is just returning minimal pages.

5. **Breaks the current performance invariants** — Current code and docs emphasize:
   - JSON first (fast)
   - Fast-fail on 403/429/timeout
   - Merge updates as each provider responds
   - Low time-to-first-useful-result for A2A/ACP

   Routing everything through FS destroys all of those for the free pool.

### What the tests actually support

FlareSolverr **does** help on some currently difficult providers (it turned some 429/blocked direct calls into 200 + real HTML). It is useful as a **last-resort rendered fallback** for specific backends that are otherwise unreachable.

It is **not** a good default path for the 8 pre-wired free providers under the "full parallel always" model.


## Implementation Status (2026-06-27)

The optional, non-blocking FlareSolverr support has been implemented in the Rust searxng-mcp client:

- New env var: `SEARXNG_FLARESOLVERR_URL`
- In `search_one()`: after JSON and plain HTML both fail to produce results for a given backend, it will (if configured) call out to FlareSolverr for that backend only.
- The FlareSolverr call uses a long per-request timeout (~95s) and the same lightweight "simple" theme HTML parser used for the direct fallback.
- Results are tagged with engine `"flaresolverr"`.
- Because each backend runs in its own spawned task inside `parallel_search_stream()`, a slow FS solve for one provider never blocks the rest of the parallel query or the streaming merge updates.
- The 8 pre-wired free providers + full-parallel-no-limit behavior is completely unchanged when the env var is not set.
- Unit tests for the shared HTML parser (`parse_simple_theme_html`) were added in `crates/goose-mcp/src/searxng/client.rs`.

This matches the "optional last-resort per-backend" recommendation from the research above.

Main user docs: `documentation/docs/mcp/searxng-mcp.md` (see "Optional Rendered Fallback via FlareSolverr" and "Using the Optional FlareSolverr Fallback").

