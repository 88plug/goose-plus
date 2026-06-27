# searxng-mcp Compatibility Report (2026-06-27)

**Question:** Which free public SearXNG instances actually work with `searxng-mcp`?

## What searxng-mcp Actually Requires

From inspecting the installed package (`~/.local/share/uv/tools/searxng-mcp/...`):

- Uses `httpx` to call `GET /search` with `format=json`
- **Strict payload validation** in `client.py` (`_is_valid_search_payload`):
  - Response **must** be a JSON object
  - Must contain key `"results"` that is a **list of dicts**
  - Optional keys (`answers`, `corrections`, `infoboxes`, `suggestions`, `unresponsive_engines`) must be lists or absent
- On 403/429/5xx/non-JSON/malformed → it treats as error and falls back (or fails)
- User-Agent it sends: `searxng-mcp/<version>`
- Default target (if not configured): `http://127.0.0.1:8890`

It does **not** accept HTML pages, even if they look like search results.

## Test Results on All Discovered Instances (strict JSON + results check)

| Instance                        | Ping | /search status | Returns JSON? | Has "results" list >0 | Usable by searxng-mcp? | Reason |
|---------------------------------|------|----------------|---------------|-----------------------|------------------------|--------|
| http://127.0.0.1:8890 (default) | —    | Connection refused | —             | —                     | ❌ No                 | No local instance running |
| http://127.0.0.1:8888           | —    | 404            | —             | —                     | ❌ No                 | Wrong port / not SearXNG |
| https://searx.tiekoetter.com    | 200  | 429            | No (HTML)     | 0                     | ❌ No                 | Rate limited |
| https://baresearch.org          | 200  | 200            | No (HTML)     | 0                     | ❌ No                 | Returns HTML even with format=json |
| https://searxng.site            | 200  | 403            | No            | 0                     | ❌ No                 | Bot blocked |
| https://search.2b9t.xyz         | 200  | 429            | No            | 0                     | ❌ No                 | Rate limited |
| https://searx.be                | 200  | 403            | No            | 0                     | ❌ No                 | Bot blocked (openresty) |
| https://search.abohiccups.com   | 200  | 429            | No            | 0                     | ❌ No                 | Rate limited |
| https://failsearx.culturanerd.it| 200  | 429            | No            | 0                     | ❌ No                 | Rate limited |
| https://searx.prvcy.eu          | 200  | 403            | No            | 0                     | ❌ No                 | Bot blocked |
| https://search.bladerunn.in     | 200  | 429            | No            | 0                     | ❌ No                 | Rate limited |
| https://search.ononoki.org      | 200  | 429            | No            | 0                     | ❌ No                 | Rate limited |

**Result: 0 usable public instances at test time (2026-06-27 ~11:06).**

## Why This Happens

Most public SearXNG instances have become aggressive about:
- Blocking non-browser User-Agents
- Rate-limiting `format=json` requests
- Returning HTML "please solve captcha" or "too many requests" pages instead of JSON

This is a widespread problem for tools that rely on the JSON API.

## HTML Fallback Feasibility (Deep Dive, 2026-06-27)

After reading SearXNG source (`searx/webapp.py`, `searx/webutils.py`, `searx/results.py`, `searx/result_types/*`, templates in `searx/templates/simple/`) and the official docs (`docs/dev/search_api.rst`, `docs/admin/api.rst`):

- SearXNG supports `format=json|csv|rss|html`. `format` is honored only if listed in `settings['search']['formats']`. Many public instances set only `['html']` or have bot/limiter rules that 403 `format=json`.
- The JSON path (`get_json_response`) emits exactly:
  ```json
  { "query": "...", "results": [ { "title", "url", "content", "engine", "engines": [...], "score", ... }, ... ], "answers": [], "suggestions": [], "corrections": [], "infoboxes": [], "unresponsive_engines": [] }
  ```
- The HTML path (simple theme) renders:
  - `<article class="result ... category-XXX">`
  - `<h3><a href="...">Title</a></h3>`
  - `<p class="content">...</p>`
  - `<div class="engines"><span>engine</span>...</div>`
  - Sidebar: suggestions, corrections, answers, infoboxes.
- `searxng-mcp` already has a high-quality HTML extractor (`extract.py` using BeautifulSoup + lxml, plus optional Playwright rendering). The same techniques apply to search results pages.

**Conclusion**: A reliable HTML→JSON-shape parser is feasible for the common "simple" theme. A prototype lives at `docs/searxng-html-results-parser.py` (produces MCP-compatible payloads with `"results"` list etc.).

**However, in practice (live tests)**:
- Many public instances return "Making sure you're not a bot", 403 Forbidden, or tiny CAPTCHA pages even for normal HTML requests with a desktop UA.
- HTML fallback helps only against instances that allow HTML but disable `format=json`.
- It does **not** solve aggressive bot detection.

## Proposed Changes for 88plug/searxng-mcp

### 1. New parser module (drop-in, no new deps)
Copy/adapt `docs/searxng-html-results-parser.py` → `src/searxng_mcp/html_search.py`.

Key function:
```python
def parse_searxng_html_results(html: str, *, base_url: str = "", query: str = "") -> dict[str, Any]:
    # returns the same shape as SearXNG JSON
```

Also expose:
```python
def make_search_payload_from_html(...) -> dict[str, Any]
```

### 2. Client changes (`src/searxng_mcp/client.py`)
- In `_search_on_backend`, on non-JSON / 403 / 429 that `_should_fallback` currently allows, optionally attempt an HTML fetch + parse.
- Add a setting-controlled path:
  ```python
  if settings.allow_html_fallback and "format" in cleaned:
      # retry without format=json, parse HTML
      html = (await client.get("/search", params=...)).text
      payload = parse_searxng_html_results(html, base_url=backend_url, query=...)
      if _is_valid_search_payload(payload):
          return BackendResponse(..., payload=payload)
  ```
- Consider a more browser-like UA for the HTML path (configurable).

### 3. Service / params (`src/searxng_mcp/service.py`)
- `_search_params` currently hardcodes `"format": "json"`.
- Make it conditional:
  ```python
  if not settings.allow_html_fallback:
      params["format"] = "json"
  ```
- In `_search_once`, wrap the call; on `BackendRequestError` that is fallback-eligible, try the raw HTML path using the new parser and synthesize a `QueryOutcome`.

### 4. Settings (`src/searxng_mcp/settings.py`)
Add:
```python
allow_html_fallback: bool = _env_bool("SEARXNG_MCP_ALLOW_HTML_FALLBACK", False)
html_fallback_ua: str = _env("SEARXNG_MCP_HTML_FALLBACK_UA", "")  # empty = use default browser-ish
```

### 5. Other
- `pyproject.toml` / `uv-receipt`: ensure `beautifulsoup4`, `lxml` are explicit (they already are transitively via extract).
- Add a `searxng://html-fallback` resource or health note when enabled.
- Tests: add `tests/test_html_search.py` with synthetic simple-theme HTML + expected payloads.
- Docs: update `docs/faq.md`, `docs/configuration.md`, README quickstart.

### Integration Sketch (minimal)
In `client.py` (inside the except or a new `_search_on_backend_html`):
```python
# after the json attempt fails with a fallback-able error
if self.settings.allow_html_fallback:
    resp = await client.get("/search", params={k:v for k,v in cleaned.items() if k != "format"})
    if resp.status_code < 400:
        payload = parse_searxng_html_results(resp.text, base_url=backend_url)
        if _is_valid_search_payload(payload):
            return BackendResponse(backend_url=backend_url, ..., payload=payload)
```

Then callers in `service.py` (`_search_once`) get a normal `BackendResponse` and everything downstream (caching, rendering, `result_summary`, etc.) works unchanged.

## Recommendations / Next Steps (updated)

1. **Still best: Run your own local SearXNG** (full JSON, no blocks).
2. **For public instances**: Enable HTML fallback as a best-effort layer. Expect it to work on a minority of lenient instances.
3. **For production/research use**: Self-host or use a proxy/VPS instance with relaxed `search.formats` and bot rules.
4. **Contribute the parser + fallback logic** to 88plug/searxng-mcp (or upstream if they accept it). The prototype is already written and tested against the documented theme structure.

## Files

- `docs/searxng-instances.md` — full list + engine counts
- This file — compatibility matrix for the actual MCP tool

**Last tested:** 2026-06-27 11:07
