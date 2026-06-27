# searxng-mcp HTML Fallback Integration Guide

This documents how to make `88plug/searxng-mcp` work with public SearXNG instances that block or disable `format=json`.

## Background (from deep dive)

- SearXNG serves JSON only when `format=json` **and** the format is enabled in `settings['search']['formats']`.
- Public instances frequently:
  - Only enable `['html']`
  - Rate-limit / bot-block `format=json` (403/429) while still serving HTML
- The MCP client hard-requires a specific JSON shape (`"results": list[dict]` + optional list fields) in `client.py:_is_valid_search_payload`.
- The MCP already has excellent HTML parsing (`extract.py` + BeautifulSoup/lxml + optional Playwright).

## Files created / proposed

- `docs/searxng-html-results-parser.py` — standalone reference parser + CLI test
- `src/searxng_mcp/html_search.py` — production module (copy of the reference, cleaned)
- `docs/searxng-mcp-compatibility.md` — updated with full analysis + matrix
- This file — concrete integration steps + code locations

## Minimal patch locations (in the 88plug/searxng-mcp tree)

### 1. Add the parser
```
src/searxng_mcp/html_search.py   (new file)
```

Key exports:
```python
def parse_searxng_html_results(html: str, *, base_url: str = "", query: str = "") -> dict[str, Any]: ...
def make_search_payload_from_html(html: str, *, base_url: str = "", query: str = "") -> dict[str, Any]: ...
```

### 2. Settings (`src/searxng_mcp/settings.py`)
Add near the other `_env_*` fields and `Settings` dataclass:

```python
allow_html_fallback: bool = _env_bool("SEARXNG_MCP_ALLOW_HTML_FALLBACK", False)
html_fallback_user_agent: str = _env("SEARXNG_MCP_HTML_FALLBACK_UA", "")
```

Update `load_settings()` to pass them.

### 3. Client changes (`src/searxng_mcp/client.py`)

Import:
```python
from .html_search import make_search_payload_from_html
```

Modify `_search_on_backend` (or add a helper):

```python
# after the JSON attempt raises BackendRequestError that is fallback-eligible
if getattr(self, "_settings", None) and self._settings.allow_html_fallback:
    try:
        # re-issue without forcing format=json
        params_no_fmt = {k: v for k, v in cleaned.items() if k != "format"}
        # optionally use a more browser-like UA for this path
        ua = self._settings.html_fallback_user_agent or self._settings.mcp_user_agent
        headers = {**client.headers, "Accept": "text/html,application/xhtml+xml", "User-Agent": ua}
        r = await client.get("/search", params=params_no_fmt, headers=headers)
        if r.status_code < 400 and "html" in (r.headers.get("content-type", "") or ""):
            payload = make_search_payload_from_html(r.text, base_url=backend_url, query=cleaned.get("q", ""))
            if _is_valid_search_payload(payload):
                elapsed_ms = (time.perf_counter() - started) * 1000
                return BackendResponse(
                    backend_url=backend_url,
                    url=str(r.request.url),
                    status_code=r.status_code,
                    elapsed_ms=elapsed_ms,
                    payload=payload,
                )
    except Exception:
        pass  # fall through to normal error handling
```

You can store `settings` on the client (or pass it) so the fallback logic can see `allow_html_fallback`.

### 4. Service changes (`src/searxng_mcp/service.py`)

In `_search_params`:

```python
params: dict[str, Any] = {
    "q": query,
    "categories": ...,
    ...
}
if not getattr(self.settings, "allow_html_fallback", False):
    params["format"] = "json"
```

In `_search_once`, after:

```python
response = await self.search_client.search(params)
```

wrap it so that on a fallback-eligible `BackendRequestError` you can attempt the HTML path by calling a new method on the client or directly using the parser + httpx.

Because `search_client.search` already does fallback across backends, the cleanest approach is to make the HTML attempt **inside the client** (as shown above). Then `_search_once` and the rest of the pipeline are unchanged.

### 5. Tests
Add `tests/test_html_search.py`:

```python
from searxng_mcp.html_search import parse_searxng_html_results, make_search_payload_from_html

def test_parses_simple_theme():
    html = open("tests/fixtures/simple_results.html").read()
    p = parse_searxng_html_results(html, base_url="https://ex.org")
    assert p["query"]
    assert isinstance(p["results"], list) and p["results"]
    assert all("url" in r and "title" in r for r in p["results"])

def test_makes_valid_mcp_payload():
    html = "<article class='result'><h3><a href='https://x'>X</a></h3><p class='content'>y</p></article>"
    payload = make_search_payload_from_html(html)
    assert _is_valid_search_payload(payload)  # import from client for the test
```

Include a couple of fixture HTML files (one good synthetic, one from a real lenient instance if you can capture one without bot walls).

### 6. Docs / README
- Mention the new env var in `docs/configuration.md` and README.
- Add a FAQ entry: "Why do some public instances only return HTML or 403 on format=json?"
- Note that HTML fallback is best-effort and many public instances will still block even HTML.

### 7. Optional: expose via resources / health
In `service.py` health or a resource, surface:
```python
"html_fallback_enabled": bool(settings.allow_html_fallback),
```

## Realistic expectations

- HTML fallback will **increase** the number of public instances that can be used at all.
- It will **not** make every public instance magically work — bot detection, Cloudflare challenges, and rate limits still apply.
- The highest quality experience remains running your own SearXNG (or a relaxed VPS/proxy instance).

## References used for this work

- SearXNG source (webapp.py, webutils.py, results.py, result_types, templates/simple/*)
- SearXNG docs (search_api.rst, admin/api.rst, settings)
- 88plug/searxng-mcp (client.py, service.py, settings.py, extract.py, render.py)
- Live captures + synthetic HTML matching the simple theme structure

Last updated: 2026-06-27
