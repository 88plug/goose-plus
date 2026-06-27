#!/usr/bin/env python3
"""
HTML Results Parser for SearXNG (simple theme fallback)

Purpose:
- Allow searxng-mcp (and similar tools) to consume public SearXNG instances
  that block or disable `format=json` (403/429/non-JSON) by parsing the
  default HTML results page.

Target output shape (compatible with SearXNG's JSON API + MCP expectations):
{
  "query": str,
  "results": [
    {
      "title": str,
      "url": str,
      "content": str,
      "engine": str | None,
      "engines": list[str],
      "category": str | None,
      "score": float | None,
      "publishedDate": str | None,
      "thumbnail": str | None,
      ...
    },
    ...
  ],
  "answers": list[dict],
  "suggestions": list[str],
  "corrections": list[str],
  "infoboxes": list[dict],
  "unresponsive_engines": list,
}

Usage:
    from searxng_html_results_parser import parse_searxng_html_results
    payload = parse_searxng_html_results(html, base_url="https://searx.example.org", query=q)

This is intentionally standalone (only bs4 + stdlib) so it can be dropped into
searxng-mcp's extract/ or client layer as a fallback.

References (from deep dive):
- SearXNG source: searx/webapp.py (search(), output_format), webutils.get_json_response
- Templates: searx/templates/simple/results.html + result_templates/default.html
- Result structure: searx/results.py, searx/result_types/*
- Official docs: docs/dev/search_api.rst (notes that format=json can be disabled on public instances)
"""

from __future__ import annotations

import re
from typing import Any
from urllib.parse import urljoin

from bs4 import BeautifulSoup, Tag

_WHITESPACE = re.compile(r"\s+")
_RESULT_ARTICLE_SEL = 'article.result, article[class*="result"]'
_TITLE_LINK_SEL = 'h3 a[href], a.url_header[href], a.result-link[href]'
_CONTENT_SEL = 'p.content, .content, .result-content'
_ENGINES_SEL = '.engines span, .result-engines span, .engines'
_SIDEBAR_SUGGESTIONS = '#suggestions a, .suggestions a, .sidebar .suggestion'
_CORRECTIONS_SEL = '.corrections a, .correction a, .corrections .correction'
_INFOBOX_SEL = '#infoboxes .infobox, .infobox, details.infobox'


def _clean(text: str | None) -> str:
    if not text:
        return ""
    return _WHITESPACE.sub(" ", text).strip()


def _abs_url(base: str, href: str) -> str:
    if not href:
        return ""
    try:
        return urljoin(base, href)
    except Exception:
        return href


def _get_text(node: Tag | None) -> str:
    if node is None:
        return ""
    # Remove boilerplate children for cleaner snippets
    for bad in node.select("script, style, noscript"):
        bad.decompose()
    return _clean(node.get_text(" ", strip=True))


def parse_searxng_html_results(
    html: str,
    *,
    base_url: str = "",
    query: str = "",
) -> dict[str, Any]:
    """
    Parse a SearXNG HTML results page (simple theme) into a JSON-like structure.

    This is a best-effort parser. Public instances may show different themes,
    bot pages, or truncated results. It focuses on the common structure.
    """
    soup = BeautifulSoup(html, "lxml")

    # Recover query from the page if caller didn't supply one
    if not query:
        q_el = soup.select_one('input[name="q"], input[type="search"]')
        if q_el and q_el.get("value"):
            query = _clean(q_el.get("value"))
        elif soup.title and soup.title.string:
            # Fallback: "query - SearXNG"
            title = _clean(soup.title.string)
            if " - " in title:
                query = title.split(" - ")[0]

    results: list[dict[str, Any]] = []
    answers: list[dict[str, Any]] = []
    suggestions: list[str] = []
    corrections: list[str] = []
    infoboxes: list[dict[str, Any]] = []

    # --- Main results ---
    for article in soup.select(_RESULT_ARTICLE_SEL):
        # Title + URL
        link = article.select_one(_TITLE_LINK_SEL)
        if not link:
            # Sometimes the first external link is the result
            link = article.select_one('a[href^="http"]')
        if not link:
            continue

        url = _abs_url(base_url, link.get("href", ""))
        if not url or url.startswith(base_url + "/"):
            # Skip internal links (e.g. cached, settings)
            continue

        title = _clean(link.get_text(" ", strip=True)) or url

        # Content / snippet
        content_node = article.select_one(_CONTENT_SEL)
        content = _get_text(content_node)

        # Engines (multiple possible)
        engines: list[str] = []
        eng_container = article.select_one(_ENGINES_SEL)
        if eng_container:
            for sp in eng_container.select("span"):
                e = _clean(sp.get_text())
                if e and e not in engines:
                    engines.append(e)
        engine = engines[0] if engines else None

        # Category from article class "category-xxx"
        category = None
        for cls in article.get("class", []):
            if isinstance(cls, str) and cls.startswith("category-"):
                category = cls.split("-", 1)[1]
                break

        # Published date (if present)
        pub = article.select_one("time[datetime], time.published_date, .published_date")
        published_date = _clean(pub.get_text()) if pub else None
        pubdate_iso = pub.get("datetime") if isinstance(pub, Tag) and pub.has_attr("datetime") else None

        # Thumbnail (rare in text results, common in images)
        thumb = article.select_one("img.thumbnail, .thumbnail img, img[src*='image_proxy']")
        thumbnail = _abs_url(base_url, thumb.get("src", "")) if thumb else None

        # Length / views / author (some result types)
        length = _clean(article.select_one(".result_length").get_text()) if article.select_one(".result_length") else None

        result: dict[str, Any] = {
            "title": title,
            "url": url,
            "content": content,
            "engine": engine,
            "engines": engines,
            "category": category,
            "score": None,  # HTML rarely exposes the internal score
            "publishedDate": published_date,
            "pubdate": pubdate_iso,
        }
        if thumbnail:
            result["thumbnail"] = thumbnail
        if length:
            result["length"] = length

        results.append(result)

    # --- Sidebar / suggestions ---
    for a in soup.select(_SIDEBAR_SUGGESTIONS):
        t = _clean(a.get_text())
        if t and t not in suggestions:
            suggestions.append(t)

    # --- Corrections ---
    for a in soup.select(_CORRECTIONS_SEL):
        t = _clean(a.get_text())
        if t and t not in corrections:
            corrections.append(t)

    # --- Answers (very rough) ---
    for ans in soup.select(".answer, .answers .answer, .answer .content"):
        txt = _get_text(ans)
        if txt:
            answers.append({"answer": txt})

    # --- Infoboxes (best effort) ---
    for ib in soup.select(_INFOBOX_SEL):
        ib_title_el = ib.select_one(".title, h3, h4, .infobox-header")
        ib_title = _clean(ib_title_el.get_text()) if ib_title_el else ""
        ib_content = _get_text(ib)
        if ib_title or ib_content:
            infoboxes.append({
                "infobox": ib_title,
                "content": ib_content,
            })

    # Deduplicate results by URL (some instances duplicate across engines)
    seen_urls: set[str] = set()
    deduped: list[dict[str, Any]] = []
    for r in results:
        u = r.get("url")
        if u and u not in seen_urls:
            seen_urls.add(u)
            deduped.append(r)
    results = deduped

    payload: dict[str, Any] = {
        "query": query,
        "results": results,
        "answers": answers,
        "corrections": corrections,
        "infoboxes": infoboxes,
        "suggestions": suggestions,
        "unresponsive_engines": [],
        # Extra metadata useful for debugging fallback
        "_meta": {
            "parser": "searxng-html-fallback",
            "base_url": base_url,
            "html_result_count": len(results),
        },
    }
    return payload


# ------------------------------------------------------------------
# Convenience: turn the payload into something the MCP result_summary
# and _is_valid_search_payload logic will accept.
# ------------------------------------------------------------------
def make_search_payload_from_html(
    html: str,
    *,
    base_url: str = "",
    query: str = "",
) -> dict[str, Any]:
    """
    Returns a dict that satisfies the minimal contract used by searxng-mcp's
    client._is_valid_search_payload (has "results": list[dict]).

    You can feed this directly into the existing QueryOutcome / result rendering
    paths in service.py.
    """
    parsed = parse_searxng_html_results(html, base_url=base_url, query=query)
    # Ensure the shape the MCP client validates against
    if not isinstance(parsed.get("results"), list):
        parsed["results"] = []
    # The MCP also tolerates (and ignores) these if present as lists:
    for k in ("answers", "corrections", "infoboxes", "suggestions", "unresponsive_engines"):
        if k not in parsed or not isinstance(parsed[k], list):
            parsed[k] = []
    return parsed


if __name__ == "__main__":
    import sys
    path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/searx_real_html.html"
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            html = f.read()
    except Exception as e:
        print(f"Failed to read {path}: {e}")
        sys.exit(1)

    data = parse_searxng_html_results(html, base_url="https://searx.be", query="")
    print("query:", repr(data.get("query")))
    print("results:", len(data.get("results", [])))
    for r in data.get("results", [])[:3]:
        print(" -", r.get("title", "")[:70])
        print("   ", r.get("url", "")[:70])
        print("   engines:", r.get("engines"))
    print("suggestions:", data.get("suggestions")[:3])
    print("meta:", data.get("_meta"))
