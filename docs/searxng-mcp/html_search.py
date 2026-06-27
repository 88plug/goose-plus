"""
HTML Results Parser for SearXNG (simple theme fallback)

Allows searxng-mcp to work with public instances that block or disable
`format=json` (very common) by parsing the default HTML results page.

This module produces a payload shape compatible with:
- SearXNG's own JSON output (webutils.get_json_response)
- searxng-mcp's _is_valid_search_payload checks and result rendering

See:
- SearXNG: searx/webapp.py (search(), output_format handling)
- SearXNG: searx/templates/simple/results.html + result_templates/default.html
- SearXNG docs: docs/dev/search_api.rst (format=json can be disabled)

The parser is intentionally standalone (beautifulsoup4 + lxml, which the project
already depends on via extract.py).
"""

from __future__ import annotations

import re
from typing import Any
from urllib.parse import urljoin

from bs4 import BeautifulSoup

_WHITESPACE = re.compile(r"\s+")
_RESULT_ARTICLE_SEL = 'article.result, article[class*="result"]'
_TITLE_LINK_SEL = 'h3 a[href], a.url_header[href], a[href^="http"]'
_CONTENT_SEL = 'p.content, .content, .result-content'
_ENGINES_SEL = '.engines span, .result-engines span, .engines'
_SIDEBAR_SUGGESTIONS = '#suggestions a, .suggestions a, .sidebar .suggestion'
_CORRECTIONS_SEL = '.corrections a, .correction a, .corrections .correction'
_INFOBOX_SEL = '#infoboxes .infobox, .infobox, details.infobox'


def _clean(text: str | None) -> str:
    if not text:
        return ""
    return _WHITESPACE.sub(" ", text).strip()


def _abs(base: str, href: str) -> str:
    if not href:
        return ""
    try:
        return urljoin(base, href)
    except Exception:
        return href


def _get_text(node) -> str:
    if node is None:
        return ""
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
    Parse a SearXNG HTML results page (simple theme) into a dict that looks
    like SearXNG's JSON response.

    Returns at minimum:
        {
          "query": str,
          "results": list[dict],
          "answers": list,
          "suggestions": list,
          "corrections": list,
          "infoboxes": list,
          "unresponsive_engines": list,
          "_meta": {"parser": "html-fallback", ...}
        }
    """
    soup = BeautifulSoup(html, "lxml")

    if not query:
        q_el = soup.select_one('input[name="q"], input[type="search"]')
        if q_el and q_el.get("value"):
            query = _clean(q_el.get("value"))
        elif soup.title and soup.title.string:
            title = _clean(soup.title.string)
            if " - " in title:
                query = title.split(" - ")[0]

    results: list[dict[str, Any]] = []
    answers: list[dict[str, Any]] = []
    suggestions: list[str] = []
    corrections: list[str] = []
    infoboxes: list[dict[str, Any]] = []

    for article in soup.select(_RESULT_ARTICLE_SEL):
        link = article.select_one(_TITLE_LINK_SEL)
        if not link:
            continue

        url = _abs(base_url, link.get("href", ""))
        if not url:
            continue

        title = _clean(link.get_text(" ", strip=True)) or url

        content_node = article.select_one(_CONTENT_SEL)
        content = _get_text(content_node)

        engines: list[str] = []
        eng_container = article.select_one(_ENGINES_SEL)
        if eng_container:
            for sp in eng_container.select("span"):
                e = _clean(sp.get_text())
                if e and e not in engines:
                    engines.append(e)
        engine = engines[0] if engines else None

        category = None
        for cls in article.get("class", []):
            if isinstance(cls, str) and cls.startswith("category-"):
                category = cls.split("-", 1)[1]
                break

        pub = article.select_one("time[datetime], time.published_date, .published_date")
        published_date = _clean(pub.get_text()) if pub else None
        pubdate_iso = None
        if pub and hasattr(pub, "get") and pub.has_attr("datetime"):
            pubdate_iso = pub.get("datetime")

        thumb = article.select_one("img.thumbnail, .thumbnail img, img[src*='image_proxy']")
        thumbnail = _abs(base_url, thumb.get("src", "")) if thumb else None

        result: dict[str, Any] = {
            "title": title,
            "url": url,
            "content": content,
            "engine": engine,
            "engines": engines,
            "category": category,
            "score": None,
            "publishedDate": published_date,
            "pubdate": pubdate_iso,
        }
        if thumbnail:
            result["thumbnail"] = thumbnail

        results.append(result)

    for a in soup.select(_SIDEBAR_SUGGESTIONS):
        t = _clean(a.get_text())
        if t and t not in suggestions:
            suggestions.append(t)

    for a in soup.select(_CORRECTIONS_SEL):
        t = _clean(a.get_text())
        if t and t not in corrections:
            corrections.append(t)

    for ans in soup.select(".answer, .answers .answer, .answer .content"):
        txt = _get_text(ans)
        if txt:
            answers.append({"answer": txt})

    for ib in soup.select(_INFOBOX_SEL):
        ib_title_el = ib.select_one(".title, h3, h4, .infobox-header")
        ib_title = _clean(ib_title_el.get_text()) if ib_title_el else ""
        ib_content = _get_text(ib)
        if ib_title or ib_content:
            infoboxes.append({"infobox": ib_title, "content": ib_content})

    # Deduplicate by URL
    seen: set[str] = set()
    deduped: list[dict[str, Any]] = []
    for r in results:
        u = r.get("url")
        if u and u not in seen:
            seen.add(u)
            deduped.append(r)
    results = deduped

    return {
        "query": query,
        "results": results,
        "answers": answers,
        "corrections": corrections,
        "infoboxes": infoboxes,
        "suggestions": suggestions,
        "unresponsive_engines": [],
        "_meta": {
            "parser": "searxng-html-fallback",
            "base_url": base_url,
            "html_result_count": len(results),
        },
    }


def make_search_payload_from_html(
    html: str,
    *,
    base_url: str = "",
    query: str = "",
) -> dict[str, Any]:
    """
    Returns a dict guaranteed to pass _is_valid_search_payload
    (has a proper "results" list of dicts).
    """
    payload = parse_searxng_html_results(html, base_url=base_url, query=query)
    if not isinstance(payload.get("results"), list):
        payload["results"] = []
    for k in ("answers", "corrections", "infoboxes", "suggestions", "unresponsive_engines"):
        if k not in payload or not isinstance(payload[k], list):
            payload[k] = []
    return payload
