from __future__ import annotations

from collections import Counter
from dataclasses import dataclass, field
import inspect
from typing import Any
import hashlib
import asyncio
import time

import httpx
import orjson
from diskcache import Cache
from mcp.server.fastmcp import Context
from mcp.types import CallToolResult

from .browser import RenderedFetchClient, RenderedFetchError
from .client import BackendRequestError, FetchClient, SearxngClient
from .extract import ExtractedDocument, extract_from_html, extract_from_response, normalize_requested_url
from .render import (
    approximate_tokens,
    clean_text,
    domain_from_url,
    list_as_text,
    make_result,
    normalize_url,
    result_bullets,
    result_summary,
    result_summary_line,
    truncate_text,
)
from .settings import Settings


def _csv(value: str | None) -> str | None:
    if value is None:
        return None
    items = []
    seen = set()
    for part in value.split(","):
        item = clean_text(part)
        if not item or item in seen:
            continue
        seen.add(item)
        items.append(item)
    return ",".join(items) if items else None


def _float(value: Any) -> float | None:
    if value is None:
        return None
    try:
        result = float(value)
    except Exception:
        return None
    if result != result:  # NaN
        return None
    return result


def _int(value: Any) -> int | None:
    if value is None:
        return None
    try:
        return int(value)
    except Exception:
        return None


async def _call_ctx_method(method: Any, *args: Any, **kwargs: Any) -> None:
    if method is None:
        return

    async def _await_result(result: Any) -> None:
        if inspect.isawaitable(result):
            await result

    try:
        await _await_result(method(*args, **kwargs))
        return
    except RuntimeError as exc:
        if "outside of a request" in str(exc).lower():
            return
        raise
    except TypeError as exc:
        if not kwargs or "unexpected keyword argument" not in str(exc).lower():
            raise

    try:
        await _await_result(method(*args))
    except RuntimeError as exc:
        if "outside of a request" in str(exc).lower():
            return
        raise


async def _ctx_info(ctx: Context | None, message: str, **kwargs: Any) -> None:
    if ctx is None:
        return
    await _call_ctx_method(ctx.info, message, **kwargs)


async def _ctx_report_progress(ctx: Context | None, current: int, total: int, message: str) -> None:
    if ctx is None:
        return
    await _call_ctx_method(ctx.report_progress, current, total, message)


def _canonical_fetch_url(url: str) -> str:
    return normalize_url(normalize_requested_url(url))


@dataclass(slots=True)
class QueryOutcome:
    query: str
    backend_url: str
    params: dict[str, Any]
    payload: dict[str, Any]
    elapsed_ms: float
    cache_hit: bool
    cache_key: str
    request_url: str

    @property
    def results(self) -> list[dict[str, Any]]:
        raw = self.payload.get("results", [])
        return raw if isinstance(raw, list) else []

    @property
    def result_count(self) -> int:
        value = self.payload.get("number_of_results")
        if isinstance(value, int):
            return value
        if isinstance(value, str) and value.isdigit():
            return int(value)
        return len(self.results)

    def top_raw_results(self, limit: int) -> list[dict[str, Any]]:
        return [item for item in self.results[:limit] if isinstance(item, dict)]


@dataclass(slots=True)
class FetchOutcome:
    url: str
    document: ExtractedDocument
    elapsed_ms: float
    cache_hit: bool
    rendered: bool
    render_mode: str
    cache_key: str
    request_url: str


@dataclass(slots=True)
class MergedHit:
    canonical_url: str
    best_raw: dict[str, Any]
    raw_hits: list[dict[str, Any]] = field(default_factory=list)
    queries: list[str] = field(default_factory=list)
    engines: list[str] = field(default_factory=list)
    best_score: float = 0.0
    hit_count: int = 0
    first_seen_rank: int = 10**9
    first_seen_query_index: int = 10**9

    def add(self, raw: dict[str, Any], query: str, query_index: int, raw_rank: int) -> None:
        score = _float(raw.get("score"))
        if score is None:
            score = 0.0
        if raw_rank < self.first_seen_rank or (
            raw_rank == self.first_seen_rank and query_index < self.first_seen_query_index
        ):
            self.first_seen_rank = raw_rank
            self.first_seen_query_index = query_index
        self.best_score = max(self.best_score, score)
        self.hit_count += 1
        self.raw_hits.append(raw)
        if query not in self.queries:
            self.queries.append(query)
        engine = clean_text(str(raw.get("engine") or ""))
        if engine and engine not in self.engines:
            self.engines.append(engine)
        if not self.best_raw:
            self.best_raw = raw
        else:
            current_score = _float(self.best_raw.get("score")) or 0.0
            if score > current_score:
                self.best_raw = raw

    @property
    def merged_score(self) -> float:
        return self.best_score + (self.hit_count - 1) * 0.35 - self.first_seen_rank * 0.01

    def to_summary(self) -> dict[str, Any]:
        summary = result_summary(self.best_raw, None)
        summary.update(
            {
                "canonical_url": self.canonical_url,
                "hit_count": self.hit_count,
                "queries": self.queries,
                "engines": self.engines,
                "merged_score": round(self.merged_score, 4),
            }
        )
        return summary


def _search_cache_key(params: dict[str, Any]) -> str:
    payload = orjson.dumps(params, option=orjson.OPT_SORT_KEYS)
    return hashlib.sha256(payload).hexdigest()


def _document_cache_payload(document: ExtractedDocument) -> dict[str, Any]:
    return {
        "url": document.url,
        "final_url": document.final_url,
        "content_type": document.content_type,
        "title": document.title,
        "description": document.description,
        "author": document.author,
        "text": document.text,
        "excerpt": document.excerpt,
        "headings": document.headings,
        "links": document.links,
        "word_count": document.word_count,
        "char_count": document.char_count,
        "truncated": document.truncated,
        "rendered": document.rendered,
        "metadata": document.metadata,
    }


def _document_from_cache(data: dict[str, Any], excerpt_limit: int) -> ExtractedDocument:
    document = ExtractedDocument(
        url=str(data.get("url") or ""),
        final_url=str(data.get("final_url") or data.get("url") or ""),
        content_type=str(data.get("content_type") or "application/octet-stream"),
        title=data.get("title"),
        description=data.get("description"),
        author=data.get("author"),
        text=str(data.get("text") or ""),
        excerpt=str(data.get("excerpt") or ""),
        headings=list(data.get("headings") or []),
        links=list(data.get("links") or []),
        word_count=int(data.get("word_count") or 0),
        char_count=int(data.get("char_count") or 0),
        truncated=bool(data.get("truncated") or False),
        rendered=bool(data.get("rendered") or False),
        metadata=dict(data.get("metadata") or {}),
    )
    document.excerpt = truncate_text(document.text, excerpt_limit)
    document.truncated = document.char_count > excerpt_limit if excerpt_limit > 0 else False
    return document


def _fetch_cache_key(params: dict[str, Any]) -> str:
    payload = orjson.dumps(params, option=orjson.OPT_SORT_KEYS)
    return hashlib.sha256(payload).hexdigest()


def _summarize_render_modes(modes: list[str]) -> str:
    cleaned = [mode if mode in {"off", "auto", "forced"} else "off" for mode in modes]
    if not cleaned:
        return "off"
    unique = set(cleaned)
    if len(unique) == 1:
        return cleaned[0]
    active = {mode for mode in cleaned if mode != "off"}
    if not active:
        return "off"
    if len(active) == 1:
        return active.pop()
    return "mixed"


def _merge_query_outcomes(outcomes: list[QueryOutcome]) -> tuple[list[MergedHit], dict[str, Any]]:
    groups: dict[str, MergedHit] = {}
    for query_index, outcome in enumerate(outcomes):
        for raw_rank, raw in enumerate(outcome.results):
            if not isinstance(raw, dict):
                continue
            url = clean_text(str(raw.get("url") or ""))
            if not url:
                fallback = f"{outcome.query}:{raw_rank}:{raw.get('title') or ''}"
                canonical = normalize_url(fallback)
            else:
                canonical = normalize_url(url)
            group = groups.get(canonical)
            if group is None:
                group = MergedHit(canonical_url=canonical, best_raw=raw)
                groups[canonical] = group
            group.add(raw, outcome.query, query_index, raw_rank)

    merged = sorted(
        groups.values(),
        key=lambda item: (-item.merged_score, item.first_seen_query_index, item.first_seen_rank, clean_text(str(item.best_raw.get("title") or ""))),
    )
    stats = {
        "total_raw_results": sum(len(outcome.results) for outcome in outcomes),
        "unique_results": len(groups),
        "queries": [outcome.query for outcome in outcomes],
        "top_domains": Counter(
            domain_from_url(item.best_raw.get("url") or "") for item in merged if item.best_raw.get("url")
        ).most_common(5),
    }
    return merged, stats


def _search_params(
    *,
    query: str,
    categories: str,
    engines: str | None,
    enabled_engines: str | None,
    disabled_engines: str | None,
    language: str,
    pageno: int,
    time_range: str | None,
    safesearch: int,
) -> dict[str, Any]:
    params: dict[str, Any] = {
        "q": query,
        "format": "json",
        "categories": _csv(categories) or categories,
        "language": language,
        "pageno": max(1, pageno),
        "safesearch": safesearch,
    }
    for key, value in {
        "engines": _csv(engines),
        "enabled_engines": _csv(enabled_engines),
        "disabled_engines": _csv(disabled_engines),
        "time_range": clean_text(time_range) if time_range else None,
    }.items():
        if value:
            params[key] = value
    return params


class SearxngMCPService:
    def __init__(
        self,
        settings: Settings,
        *,
        search_client: SearxngClient | None = None,
        fetch_client: FetchClient | None = None,
        render_client: RenderedFetchClient | None = None,
        cache: Cache | None = None,
    ) -> None:
        self.settings = settings
        self.search_client = search_client or SearxngClient(settings)
        self.fetch_client = fetch_client or FetchClient(settings)
        self.render_client = render_client or RenderedFetchClient(settings)
        self.cache = cache or Cache(settings.cache_dir, size_limit=settings.cache_size_limit)
        self.cache.stats(enable=True)
        self._owns_search_client = search_client is None
        self._owns_fetch_client = fetch_client is None
        self._owns_render_client = render_client is None

    async def close(self) -> None:
        if self._owns_search_client:
            await self.search_client.close()
        if self._owns_fetch_client:
            await self.fetch_client.close()
        if self._owns_render_client:
            await self.render_client.close()
        self.cache.close()

    def _cache_get(self, key: str) -> tuple[Any | None, bool]:
        value = self.cache.get(key)
        return value, value is not None

    def _cache_set(self, key: str, value: Any, ttl: int) -> None:
        self.cache.set(key, value, expire=ttl)

    async def _search_once(
        self,
        *,
        query: str,
        categories: str,
        engines: str | None,
        enabled_engines: str | None,
        disabled_engines: str | None,
        language: str,
        pageno: int,
        time_range: str | None,
        safesearch: int,
        ttl: int | None = None,
    ) -> QueryOutcome:
        clean_query = clean_text(query)
        if not clean_query:
            raise ValueError("query is required")
        params = _search_params(
            query=clean_query,
            categories=categories,
            engines=engines,
            enabled_engines=enabled_engines,
            disabled_engines=disabled_engines,
            language=language or self.settings.default_language,
            pageno=pageno,
            time_range=time_range,
            safesearch=safesearch,
        )
        cache_key = f"search:{_search_cache_key(params)}"
        cached, cache_hit = self._cache_get(cache_key)
        if cache_hit and isinstance(cached, dict):
            payload = cached.get("payload") if isinstance(cached.get("payload"), dict) else cached
            backend_url = clean_text(str(cached.get("backend_url") or self.settings.normalized_base_url))
            request_url = clean_text(str(cached.get("request_url") or f"{backend_url}/search"))
            return QueryOutcome(
                query=clean_query,
                backend_url=backend_url,
                params=params,
                payload=payload,
                elapsed_ms=0.0,
                cache_hit=True,
                cache_key=cache_key,
                request_url=request_url,
            )

        response = await self.search_client.search(params)
        payload = response.payload
        self._cache_set(
            cache_key,
            {
                "payload": payload,
                "backend_url": response.backend_url,
                "request_url": response.url,
            },
            ttl or self.settings.search_cache_ttl,
        )
        return QueryOutcome(
            query=clean_query,
            backend_url=response.backend_url,
            params=params,
            payload=payload,
            elapsed_ms=response.elapsed_ms,
            cache_hit=False,
            cache_key=cache_key,
            request_url=response.url,
        )

    def _backend_response_to_outcome(
        self, resp: Any, params: dict[str, Any], query: str
    ) -> QueryOutcome | None:
        """Convert a successful BackendResponse (from parallel or single) into a QueryOutcome."""
        if resp is None or not hasattr(resp, "payload"):
            return None
        payload = getattr(resp, "payload", {}) or {}
        if not isinstance(payload, dict):
            return None
        return QueryOutcome(
            query=query,
            backend_url=getattr(resp, "backend_url", "unknown"),
            params=params,
            payload=payload,
            elapsed_ms=getattr(resp, "elapsed_ms", 0.0),
            cache_hit=False,
            cache_key="",  # not cached at this level for parallel
            request_url=getattr(resp, "url", ""),
        )

    def _render_search(
        self,
        outcome: QueryOutcome,
        *,
        max_results: int,
    ) -> tuple[str, dict[str, Any], dict[str, Any]]:
        raw_results = outcome.top_raw_results(max_results)
        visible_results = [result_summary(item, rank + 1) for rank, item in enumerate(raw_results)]
        answers = outcome.payload.get("answers") or []
        corrections = outcome.payload.get("corrections") or []
        infoboxes = outcome.payload.get("infoboxes") or []
        suggestions = outcome.payload.get("suggestions") or []
        unresponsive = outcome.payload.get("unresponsive_engines") or []
        unresponsive_text = list_as_text(unresponsive, limit=8)
        text_parts = [
            f"Query: {outcome.query}",
            f"Results: {outcome.result_count} total, {len(visible_results)} shown",
            f"Cache: {'hit' if outcome.cache_hit else 'miss'}",
            f"Backend: {outcome.backend_url}",
            f"Latency: {outcome.elapsed_ms:.1f} ms",
        ]
        if answers:
            text_parts.append(f"Answers: {list_as_text(answers, limit=5)}")
        if corrections:
            text_parts.append(f"Corrections: {list_as_text(corrections, limit=5)}")
        if infoboxes:
            first = infoboxes[0]
            if isinstance(first, dict):
                title = clean_text(str(first.get("title") or first.get("infobox") or first.get("engine") or ""))
                content = clean_text(str(first.get("content") or first.get("description") or ""))
                piece = title
                if content:
                    piece = f"{piece}: {truncate_text(content, 220)}" if piece else truncate_text(content, 220)
                if piece:
                    text_parts.append(f"Infobox: {piece}")
        if suggestions:
            text_parts.append(f"Suggestions: {list_as_text(suggestions, limit=8)}")
        if unresponsive_text:
            text_parts.append(f"Unresponsive engines: {unresponsive_text}")
        if visible_results:
            text_parts.append("")
            text_parts.append("Top results:")
            text_parts.append(result_bullets([item for item in raw_results], start_rank=1))
        text = "\n".join(text_parts).strip()

        structured = {
            "query": outcome.query,
            "backend_url": outcome.backend_url,
            "result_count": outcome.result_count,
            "shown_results": len(visible_results),
            "cache_hit": outcome.cache_hit,
            "elapsed_ms": round(outcome.elapsed_ms, 2),
            "answers": answers,
            "corrections": corrections,
            "infoboxes": infoboxes[:3],
            "suggestions": suggestions[:10],
            "unresponsive_engines": unresponsive,
            "top_results": visible_results,
        }
        meta = {
            "query": outcome.query,
            "backend_url": outcome.backend_url,
            "params": outcome.params,
            "raw_payload": outcome.payload,
            "cache_key": outcome.cache_key,
            "request_url": outcome.request_url,
            "token_estimate": approximate_tokens(text),
            "visible_results": visible_results,
        }
        return text, structured, meta

    def _content_type_is_htmlish(self, content_type: str) -> bool:
        lowered = (content_type or "").lower()
        return not lowered or lowered.startswith("text/") or "html" in lowered or "xml" in lowered

    def _should_auto_render(self, document: ExtractedDocument) -> bool:
        if not self.settings.render_auto_fallback:
            return False
        if document.rendered or not self._content_type_is_htmlish(document.content_type):
            return False

        profile = document.metadata.get("render_profile")
        if isinstance(profile, dict):
            hints = profile.get("hints")
            if isinstance(hints, list) and any(clean_text(str(item)) for item in hints):
                return True

            script_count = _int(profile.get("script_count")) or 0
            text_density = _float(profile.get("text_density")) or 0.0
            if script_count >= 3 and text_density <= 0.06:
                return True
            if script_count >= 1 and (
                document.word_count <= self.settings.render_auto_min_words
                or document.char_count <= self.settings.render_auto_min_chars
            ):
                return True

        lowered = f"{document.title or ''} {document.description or ''} {document.excerpt or ''}".lower()
        if any(marker in lowered for marker in ("javascript required", "enable javascript", "please enable javascript", "turn on javascript")):
            return True
        return False

    async def _render_and_extract(
        self,
        *,
        url: str,
        max_excerpt_chars: int,
        max_links: int,
        render_wait_ms: int | None,
    ) -> tuple[Any, ExtractedDocument]:
        render_response = await self.render_client.get(url, wait_ms=render_wait_ms)
        document = extract_from_html(
            render_response.html,
            url=url,
            final_url=render_response.final_url,
            content_type=render_response.content_type,
            max_excerpt_chars=max_excerpt_chars,
            max_links=max(max_links, 32),
            rendered=True,
        )
        return render_response, document

    async def search(
        self,
        *,
        query: str,
        categories: str | None = None,
        engines: str | None = None,
        enabled_engines: str | None = None,
        disabled_engines: str | None = None,
        language: str | None = None,
        pageno: int = 1,
        time_range: str | None = None,
        safesearch: int | None = None,
        max_results: int | None = None,
        ttl: int | None = None,
        ctx: Context | None = None,
    ) -> CallToolResult:
        """
        The main search tool.

        SECRET SAUCE (when enabled):
        If use_free_providers + free_parallel, we fan the query out to ALL
        configured public free SearXNG instances **concurrently** (not sequential
        fallback). Each backend tries JSON then HTML. Successful result sets
        are merged (dedup + engine aggregation + hit boosting). This gives
        dramatically richer results than any single instance.
        """
        clean_query = clean_text(query)
        if not clean_query:
            return make_result(
                "search failed: query is required",
                structured={"error": "query is required"},
                meta={"error": "query is required"},
                is_error=True,
            )

        use_free = getattr(self.settings, "use_free_providers", False)
        parallel = getattr(self.settings, "free_parallel", False)

        if use_free and parallel:
            # === THE POWER PATH: full parallel across free public providers ===
            try:
                params = _search_params(
                    query=clean_query,
                    categories=categories or self.settings.default_categories,
                    engines=engines,
                    enabled_engines=enabled_engines,
                    disabled_engines=disabled_engines,
                    language=language or self.settings.default_language,
                    pageno=pageno,
                    time_range=time_range,
                    safesearch=self.settings.default_safesearch if safesearch is None else safesearch,
                )

                # Fire all free (and main) backends concurrently
                raw_responses = await self.search_client.search_parallel(
                    params,
                    max_concurrency=getattr(self.settings, "free_max_concurrency", 8),
                )

                # Convert to QueryOutcomes (one per successful backend)
                outcomes: list[QueryOutcome] = []
                for r in raw_responses:
                    oc = self._backend_response_to_outcome(r, params, clean_query)
                    if oc is not None:
                        outcomes.append(oc)

                if not outcomes:
                    # Fall back to classic single if everything failed
                    outcome = await self._search_once(
                        query=clean_query,
                        categories=categories or self.settings.default_categories,
                        engines=engines,
                        enabled_engines=enabled_engines,
                        disabled_engines=disabled_engines,
                        language=language or self.settings.default_language,
                        pageno=pageno,
                        time_range=time_range,
                        safesearch=self.settings.default_safesearch if safesearch is None else safesearch,
                        ttl=ttl,
                    )
                    text, structured, meta = self._render_search(outcome, max_results=max_results or self.settings.default_max_results)
                    return make_result(text, structured=structured, meta=meta)

                # Merge across all parallel backends (the magic)
                merged_hits, merge_stats = _merge_query_outcomes(outcomes)

                # Build a synthetic "super" outcome for rendering (use first as base)
                base = outcomes[0]
                # We will render using merged data directly below
                visible_limit = max_results or self.settings.default_max_results
                visible = [h.to_summary() for h in merged_hits[:visible_limit]]

                # Rich text
                text_lines = [
                    f"Query: {clean_query}",
                    f"Parallel free providers: {len(outcomes)} backends succeeded",
                    f"Unique results: {merge_stats.get('unique_results', len(merged_hits))} (raw {merge_stats.get('total_raw_results', 0)})",
                ]
                if merge_stats.get("top_domains"):
                    top_d = ", ".join(f"{d} ({c})" for d, c in merge_stats["top_domains"][:5])
                    text_lines.append(f"Top domains: {top_d}")
                text_lines.append("")
                text_lines.append("Top merged results (from many SearXNG instances at once):")
                text_lines.append(
                    "\n".join(result_summary_line(h.best_raw, rank + 1) for rank, h in enumerate(merged_hits[:visible_limit]))
                )
                text = "\n".join(text_lines).strip()

                structured = {
                    "query": clean_query,
                    "parallel_mode": True,
                    "backends_used": len(outcomes),
                    "unique_results": merge_stats.get("unique_results", len(merged_hits)),
                    "total_raw_results": merge_stats.get("total_raw_results", 0),
                    "top_domains": merge_stats.get("top_domains", []),
                    "merged_results": visible,
                }
                meta = {
                    "query": clean_query,
                    "parallel_mode": True,
                    "backends_used": [o.backend_url for o in outcomes],
                    "merged_results": visible,
                }

                await _ctx_info(ctx, "search (parallel free) completed", query=clean_query, backends=len(outcomes))
                return make_result(text, structured=structured, meta=meta)

            except Exception as exc:  # noqa: BLE001
                # On any parallel error, degrade gracefully to normal single search
                pass

        # === Classic single-backend path ===
        try:
            outcome = await self._search_once(
                query=clean_query,
                categories=categories or self.settings.default_categories,
                engines=engines,
                enabled_engines=enabled_engines,
                disabled_engines=disabled_engines,
                language=language or self.settings.default_language,
                pageno=pageno,
                time_range=time_range,
                safesearch=self.settings.default_safesearch if safesearch is None else safesearch,
                ttl=ttl,
            )
        except (ValueError, BackendRequestError, httpx.HTTPError) as exc:
            return make_result(
                f"search failed: {exc}",
                structured={"error": str(exc)},
                meta={"error": str(exc)},
                is_error=True,
            )

        await _ctx_info(ctx, "search completed", query=clean_query, cache_hit=outcome.cache_hit, elapsed_ms=round(outcome.elapsed_ms, 2))
        text, structured, meta = self._render_search(outcome, max_results=max_results or self.settings.default_max_results)
        return make_result(text, structured=structured, meta=meta)

    async def _fetch_once(
        self,
        *,
        url: str,
        max_excerpt_chars: int,
        max_links: int,
        rendered: bool = False,
        render_wait_ms: int | None = None,
        ttl: int | None = None,
    ) -> FetchOutcome:
        normalized = _canonical_fetch_url(url)
        effective_render_wait_ms = self.settings.render_wait_ms if render_wait_ms is None else render_wait_ms
        cache_key = f"fetch:{_fetch_cache_key({'url': normalized, 'rendered': rendered, 'render_wait_ms': effective_render_wait_ms})}"
        cached, cache_hit = self._cache_get(cache_key)
        if cache_hit and isinstance(cached, dict):
            cached_document = cached.get("document") if isinstance(cached.get("document"), dict) else cached
            document = _document_from_cache(cached_document, max_excerpt_chars)
            render_mode = clean_text(str(cached.get("render_mode") or ("forced" if document.rendered else "off")))
            if render_mode not in {"off", "auto", "forced"}:
                render_mode = "forced" if document.rendered else "off"
            return FetchOutcome(
                url=normalized,
                document=document,
                elapsed_ms=0.0,
                cache_hit=True,
                rendered=document.rendered,
                render_mode=render_mode,
                cache_key=cache_key,
                request_url=document.final_url,
            )

        ttl_value = ttl or self.settings.fetch_cache_ttl
        render_mode = "forced" if rendered else "off"
        request_url = normalized
        elapsed_ms = 0.0
        try:
            if rendered:
                render_response, document = await self._render_and_extract(
                    url=normalized,
                    max_excerpt_chars=max_excerpt_chars,
                    max_links=max_links,
                    render_wait_ms=effective_render_wait_ms,
                )
                elapsed_ms = render_response.elapsed_ms
                request_url = render_response.final_url
            else:
                response = await self.fetch_client.get(normalized)
                document = extract_from_response(
                    response.response,
                    max_excerpt_chars=max_excerpt_chars,
                    max_links=max(max_links, 32),
                )
                elapsed_ms = response.elapsed_ms
                request_url = str(response.response.url)
                if self._should_auto_render(document):
                    try:
                        render_response, rendered_document = await self._render_and_extract(
                            url=normalized,
                            max_excerpt_chars=max_excerpt_chars,
                            max_links=max_links,
                            render_wait_ms=effective_render_wait_ms,
                        )
                    except RenderedFetchError:
                        render_response = None
                    else:
                        document = rendered_document
                        elapsed_ms = render_response.elapsed_ms
                        request_url = render_response.final_url
                        render_mode = "auto"
            cache_payload = {
                "document": _document_cache_payload(document),
                "render_mode": render_mode,
            }
            self._cache_set(cache_key, cache_payload, ttl_value)
            if render_mode == "auto":
                forced_cache_key = f"fetch:{_fetch_cache_key({'url': normalized, 'rendered': True, 'render_wait_ms': effective_render_wait_ms})}"
                self._cache_set(
                    forced_cache_key,
                    {
                        "document": _document_cache_payload(document),
                        "render_mode": "forced",
                    },
                    ttl_value,
                )
            return FetchOutcome(
                url=normalized,
                document=document,
                elapsed_ms=elapsed_ms,
                cache_hit=False,
                rendered=document.rendered,
                render_mode=render_mode,
                cache_key=cache_key,
                request_url=request_url,
            )
        except (BackendRequestError, httpx.HTTPError) as exc:
            if rendered or not self.settings.render_auto_fallback:
                raise
            try:
                render_response, document = await self._render_and_extract(
                    url=normalized,
                    max_excerpt_chars=max_excerpt_chars,
                    max_links=max_links,
                    render_wait_ms=effective_render_wait_ms,
                )
            except RenderedFetchError:
                raise exc
            elapsed_ms = render_response.elapsed_ms
            request_url = render_response.final_url
            render_mode = "auto"
            cache_payload = {
                "document": _document_cache_payload(document),
                "render_mode": render_mode,
            }
            self._cache_set(cache_key, cache_payload, ttl_value)
            forced_cache_key = f"fetch:{_fetch_cache_key({'url': normalized, 'rendered': True, 'render_wait_ms': effective_render_wait_ms})}"
            self._cache_set(
                forced_cache_key,
                {
                    "document": _document_cache_payload(document),
                    "render_mode": "forced",
                },
                ttl_value,
            )
            return FetchOutcome(
                url=normalized,
                document=document,
                elapsed_ms=elapsed_ms,
                cache_hit=False,
                rendered=document.rendered,
                render_mode=render_mode,
                cache_key=cache_key,
                request_url=request_url,
            )

    def _render_fetch(
        self,
        outcome: FetchOutcome,
        *,
        max_excerpt_chars: int,
        max_links: int,
    ) -> tuple[str, dict[str, Any], dict[str, Any]]:
        document = outcome.document
        excerpt = truncate_text(document.excerpt or document.text, max_excerpt_chars)
        links = document.links[:max_links]
        lines = [
            f"URL: {document.url}",
            f"Final URL: {document.final_url}",
            f"Content type: {document.content_type}",
            f"Cache: {'hit' if outcome.cache_hit else 'miss'}",
            f"Rendered: {outcome.render_mode if outcome.render_mode != 'off' else 'no'}",
            f"Latency: {outcome.elapsed_ms:.1f} ms",
        ]
        if document.title:
            lines.append(f"Title: {document.title}")
        if document.description:
            lines.append(f"Description: {truncate_text(document.description, 260)}")
        if document.author:
            lines.append(f"Author: {document.author}")
        lines.append(f"Words: {document.word_count} | Characters: {document.char_count}")
        if document.headings:
            lines.append(f"Headings: {list_as_text(document.headings, limit=8)}")
        if excerpt:
            lines.append("")
            lines.append("Excerpt:")
            lines.append(excerpt)
        if links:
            lines.append("")
            lines.append("Links:")
            for link in links:
                lines.append(f"- [{link['text']}]({link['url']})")
        text = "\n".join(lines).strip()
        structured = {
            "url": document.url,
            "final_url": document.final_url,
            "content_type": document.content_type,
            "title": document.title,
            "description": document.description,
            "author": document.author,
            "word_count": document.word_count,
            "char_count": document.char_count,
            "cache_hit": outcome.cache_hit,
            "rendered": outcome.rendered,
            "render_mode": outcome.render_mode,
            "elapsed_ms": round(outcome.elapsed_ms, 2),
            "excerpt": excerpt,
            "headings": document.headings,
            "links": links,
        }
        meta = {
            "cache_key": outcome.cache_key,
            "request_url": outcome.request_url,
            "render_mode": outcome.render_mode,
            "document": {
                **_document_cache_payload(document),
                "excerpt": excerpt,
                "links": links,
            },
            "token_estimate": approximate_tokens(text),
        }
        return text, structured, meta

    async def search_many(
        self,
        *,
        queries: list[str],
        categories: str | None = None,
        engines: str | None = None,
        enabled_engines: str | None = None,
        disabled_engines: str | None = None,
        language: str | None = None,
        pageno: int = 1,
        time_range: str | None = None,
        safesearch: int | None = None,
        max_results: int | None = None,
        concurrency: int | None = None,
        ttl: int | None = None,
        ctx: Context | None = None,
    ) -> CallToolResult:
        clean_queries: list[str] = []
        seen_queries: set[str] = set()
        for query in queries:
            cleaned = clean_text(query)
            if not cleaned or cleaned in seen_queries:
                continue
            seen_queries.add(cleaned)
            clean_queries.append(cleaned)
        if not clean_queries:
            return make_result(
                "search_many failed: at least one non-empty query is required",
                structured={"error": "at least one non-empty query is required"},
                meta={"error": "at least one non-empty query is required"},
                is_error=True,
            )

        limit = max(1, concurrency or self.settings.search_concurrency)
        outcomes: list[QueryOutcome] = []
        errors: list[dict[str, Any]] = []
        started = time.perf_counter()

        async def run_query(index: int, query: str) -> QueryOutcome | None:
            try:
                await _ctx_report_progress(ctx, index, len(clean_queries), f"searching {query}")
                return await self._search_once(
                    query=query,
                    categories=categories or self.settings.default_categories,
                    engines=engines,
                    enabled_engines=enabled_engines,
                    disabled_engines=disabled_engines,
                    language=language or self.settings.default_language,
                    pageno=pageno,
                    time_range=time_range,
                    safesearch=self.settings.default_safesearch if safesearch is None else safesearch,
                    ttl=ttl,
                )
            except Exception as exc:  # noqa: BLE001
                errors.append({"query": query, "error": str(exc)})
                return None

        # Simple bounded fan-out without an extra dependency.
        pending = list(enumerate(clean_queries, start=1))
        while pending:
            batch = pending[:limit]
            pending = pending[limit:]
            batch_outcomes = await asyncio.gather(*(run_query(index, query) for index, query in batch))
            for item in batch_outcomes:
                if item is not None:
                    outcomes.append(item)

        elapsed_ms = (time.perf_counter() - started) * 1000
        if not outcomes:
            return make_result(
                "search_many failed: all queries failed",
                structured={"error": "all queries failed", "query_errors": errors},
                meta={"error": "all queries failed", "query_errors": errors},
                is_error=True,
            )

        merged, stats = _merge_query_outcomes(outcomes)
        visible_limit = max_results or self.settings.default_max_results
        visible_hits = [hit.to_summary() for hit in merged[:visible_limit]]
        text_lines = [
            f"Queries: {len(clean_queries)}",
            f"Successful: {len(outcomes)}",
            f"Cache hits: {sum(1 for item in outcomes if item.cache_hit)}",
            f"Elapsed: {elapsed_ms:.1f} ms",
            f"Unique results: {stats['unique_results']} / raw {stats['total_raw_results']}",
        ]
        if stats["top_domains"]:
            top_domains = ", ".join(f"{domain} ({count})" for domain, count in stats["top_domains"][:5])
            text_lines.append(f"Top domains: {top_domains}")
        if errors:
            text_lines.append(f"Query errors: {len(errors)}")
        if visible_hits:
            text_lines.append("")
            text_lines.append("Top merged results:")
            text_lines.append(
                "\n".join(
                    result_summary_line(hit.best_raw, rank + 1)
                    for rank, hit in enumerate(merged[:visible_limit])
                )
            )

        text = "\n".join(text_lines).strip()
        structured = {
            "queries": clean_queries,
            "successful_queries": len(outcomes),
            "query_errors": errors,
            "elapsed_ms": round(elapsed_ms, 2),
            "cache_hits": sum(1 for item in outcomes if item.cache_hit),
            "unique_results": stats["unique_results"],
            "total_raw_results": stats["total_raw_results"],
            "top_domains": stats["top_domains"],
            "merged_results": visible_hits,
        }
        meta = {
            "queries": [item.query for item in outcomes],
            "query_outcomes": [
                {
                    "query": item.query,
                    "backend_url": item.backend_url,
                    "cache_hit": item.cache_hit,
                    "elapsed_ms": round(item.elapsed_ms, 2),
                    "cache_key": item.cache_key,
                    "request_url": item.request_url,
                    "params": item.params,
                    "raw_payload": item.payload,
                }
                for item in outcomes
            ],
            "merged_results": [hit.to_summary() for hit in merged],
            "query_errors": errors,
            "token_estimate": approximate_tokens(text),
        }
        await _ctx_info(ctx, "search_many completed", queries=len(clean_queries), successful=len(outcomes), elapsed_ms=round(elapsed_ms, 2))
        return make_result(text, structured=structured, meta=meta)

    async def search_and_fetch(
        self,
        *,
        query: str,
        categories: str | None = None,
        engines: str | None = None,
        enabled_engines: str | None = None,
        disabled_engines: str | None = None,
        language: str | None = None,
        pageno: int = 1,
        time_range: str | None = None,
        safesearch: int | None = None,
        max_results: int | None = None,
        fetch_limit: int = 3,
        fetch_excerpt_chars: int | None = None,
        rendered: bool = False,
        render_wait_ms: int | None = None,
        ttl: int | None = None,
        ctx: Context | None = None,
    ) -> CallToolResult:
        search_result = await self.search(
            query=query,
            categories=categories,
            engines=engines,
            enabled_engines=enabled_engines,
            disabled_engines=disabled_engines,
            language=language,
            pageno=pageno,
            time_range=time_range,
            safesearch=safesearch,
            max_results=max_results,
            ttl=ttl,
            ctx=ctx,
        )
        if search_result.isError:
            return search_result

        structured = search_result.structuredContent or {}
        meta = search_result.meta or {}
        raw_payload = meta.get("raw_payload") or {}
        raw_results = raw_payload.get("results") if isinstance(raw_payload, dict) else []
        raw_results = [item for item in raw_results if isinstance(item, dict)]
        selected = []
        seen_urls: set[str] = set()
        for raw in raw_results:
            url = clean_text(str(raw.get("url") or ""))
            canonical = normalize_url(url) if url else ""
            if not canonical or canonical in seen_urls:
                continue
            seen_urls.add(canonical)
            selected.append(raw)
            if len(selected) >= max(1, fetch_limit):
                break
        if not selected:
            return search_result

        excerpts: list[dict[str, Any]] = []
        errors: list[dict[str, Any]] = []
        started = time.perf_counter()

        async def fetch_one(raw: dict[str, Any]) -> dict[str, Any] | None:
            url = clean_text(str(raw.get("url") or ""))
            if not url:
                return None
            try:
                outcome = await self._fetch_once(
                    url=url,
                    max_excerpt_chars=fetch_excerpt_chars or self.settings.default_excerpt_chars,
                    max_links=8,
                    rendered=rendered,
                    render_wait_ms=render_wait_ms,
                    ttl=ttl,
                )
                document = outcome.document
                document_summary = document.to_summary()
                document_summary["links"] = document_summary["links"][:8]
                document_summary["render_mode"] = outcome.render_mode
                return {
                    "source": result_summary(raw, None),
                    "document": document_summary,
                    "content": document.excerpt,
                    "cache_hit": outcome.cache_hit,
                    "elapsed_ms": round(outcome.elapsed_ms, 2),
                    "request_url": outcome.request_url,
                    "render_mode": outcome.render_mode,
                }
            except Exception as exc:  # noqa: BLE001
                errors.append({"url": url, "error": str(exc)})
                return None

        fetch_results = await asyncio.gather(*(fetch_one(item) for item in selected))
        for item in fetch_results:
            if item is not None:
                excerpts.append(item)

        elapsed_ms = (time.perf_counter() - started) * 1000
        render_mode = _summarize_render_modes([item.get("render_mode") or ("forced" if item.get("document", {}).get("rendered") else "off") for item in excerpts])
        await _ctx_info(
            ctx,
            "search_and_fetch completed",
            query=query,
            fetched=len(excerpts),
            render_mode=render_mode,
            elapsed_ms=round(elapsed_ms, 2),
        )

        lines = [search_result.content[0].text, "", f"Fetched pages: {len(excerpts)}"]
        lines.append(f"Rendered fetch: {render_mode}")
        for index, item in enumerate(excerpts, start=1):
            source = item["source"]
            document = item["document"]
            lines.append("")
            lines.append(f"{index}. [{source['title']}]({source['url']})")
            if item.get("cache_hit"):
                lines.append("   Cache: hit")
            lines.append(f"   Rendered: {item.get('render_mode') or ('forced' if document.get('rendered') else 'no')}")
            if document.get("title") and document.get("title") != source["title"]:
                lines.append(f"   Page title: {document['title']}")
            if document.get("description"):
                lines.append(f"   Description: {truncate_text(document['description'], 240)}")
            lines.append(f"   Content: {truncate_text(item['content'], 900)}")
            if document.get("headings"):
                lines.append(f"   Headings: {list_as_text(document['headings'], limit=6)}")
            if document.get("links"):
                lines.append("   Links:")
                for link in document["links"][:4]:
                    lines.append(f"   - [{link['text']}]({link['url']})")
        if errors:
            lines.append("")
            lines.append(f"Fetch errors: {len(errors)}")
        text = "\n".join(lines).strip()
        structured.update(
            {
                "fetched_pages": excerpts,
                "fetch_errors": errors,
                "fetch_elapsed_ms": round(elapsed_ms, 2),
                "fetch_cache_hits": sum(1 for item in excerpts if item.get("cache_hit")),
                "rendered": render_mode != "off",
                "render_mode": render_mode,
                "render_wait_ms": render_wait_ms,
            }
        )
        meta = {
            **meta,
            "fetched_pages": excerpts,
            "fetch_errors": errors,
            "fetch_elapsed_ms": round(elapsed_ms, 2),
            "fetch_cache_hits": sum(1 for item in excerpts if item.get("cache_hit")),
            "rendered": render_mode != "off",
            "render_mode": render_mode,
            "render_wait_ms": render_wait_ms,
            "token_estimate": approximate_tokens(text),
        }
        return make_result(text, structured=structured, meta=meta)

    async def research(
        self,
        *,
        queries: list[str],
        categories: str | None = None,
        engines: str | None = None,
        enabled_engines: str | None = None,
        disabled_engines: str | None = None,
        language: str | None = None,
        pageno: int = 1,
        time_range: str | None = None,
        safesearch: int | None = None,
        max_results: int | None = None,
        fetch_limit: int = 3,
        fetch_excerpt_chars: int | None = None,
        rendered: bool = False,
        render_wait_ms: int | None = None,
        concurrency: int | None = None,
        ttl: int | None = None,
        ctx: Context | None = None,
    ) -> CallToolResult:
        search_result = await self.search_many(
            queries=queries,
            categories=categories,
            engines=engines,
            enabled_engines=enabled_engines,
            disabled_engines=disabled_engines,
            language=language,
            pageno=pageno,
            time_range=time_range,
            safesearch=safesearch,
            max_results=max_results,
            concurrency=concurrency,
            ttl=ttl,
            ctx=ctx,
        )
        if search_result.isError:
            return search_result

        search_structured = search_result.structuredContent or {}
        search_meta = search_result.meta or {}
        merged_results = search_meta.get("merged_results") or search_structured.get("merged_results") or []
        candidates: list[dict[str, Any]] = []
        seen_urls: set[str] = set()
        for raw in merged_results:
            if not isinstance(raw, dict):
                continue
            url = clean_text(str(raw.get("url") or ""))
            canonical = normalize_url(url) if url else ""
            if not canonical or canonical in seen_urls:
                continue
            seen_urls.add(canonical)
            candidates.append(raw)
            if len(candidates) >= max(1, fetch_limit):
                break
        if not candidates:
            return search_result

        fetch_result = await self.fetch_many(
            urls=[str(item.get("url") or "") for item in candidates],
            max_excerpt_chars=fetch_excerpt_chars or self.settings.default_excerpt_chars,
            max_links=8,
            rendered=rendered,
            render_wait_ms=render_wait_ms,
            concurrency=concurrency,
            ttl=ttl,
            ctx=ctx,
        )
        if fetch_result.isError:
            return fetch_result

        fetch_structured = fetch_result.structuredContent or {}
        fetch_meta = fetch_result.meta or {}
        fetched_pages = fetch_structured.get("documents") or []
        render_mode = clean_text(str(fetch_structured.get("render_mode") or "off")) or "off"

        lines = [
            f"Queries: {len(search_structured.get('queries') or queries)}",
            f"Search results: {search_structured.get('unique_results', 0)} unique / {search_structured.get('total_raw_results', 0)} raw",
            f"Fetched pages: {len(fetched_pages)}",
            f"Rendered fetch: {render_mode}",
        ]
        if search_structured.get("top_domains"):
            top_domains = ", ".join(
                f"{domain} ({count})" for domain, count in search_structured["top_domains"][:5] if domain
            )
            if top_domains:
                lines.append(f"Top domains: {top_domains}")
        lines.append("")
        lines.append("Merged results:")
        for index, item in enumerate(candidates, start=1):
            lines.append(result_summary_line(item, index))
        lines.append("")
        lines.append("Fetched sources:")
        for index, item in enumerate(fetched_pages, start=1):
            document = item.get("document") or {}
            title = document.get("title") or document.get("final_url") or item.get("requested_url") or ""
            final_url = document.get("final_url") or item.get("requested_url") or ""
            lines.append(f"{index}. [{title}]({final_url})")
            if item.get("cache_hit"):
                lines.append("   Cache: hit")
            lines.append(f"   Rendered: {item.get('render_mode') or ('forced' if document.get('rendered') else 'no')}")
            if document.get("description"):
                lines.append(f"   Description: {truncate_text(str(document['description']), 240)}")
            excerpt = clean_text(str(document.get("excerpt") or ""))
            if excerpt:
                lines.append(f"   Content: {truncate_text(excerpt, 900)}")
            if document.get("headings"):
                lines.append(f"   Headings: {list_as_text(document['headings'], limit=6)}")
            if document.get("links"):
                lines.append("   Links:")
                for link in document["links"][:4]:
                    lines.append(f"   - [{link['text']}]({link['url']})")

        text = "\n".join(lines).strip()
        structured = {
            "queries": search_structured.get("queries") or queries,
            "successful_queries": search_structured.get("successful_queries", 0),
            "query_errors": search_structured.get("query_errors", []),
            "unique_results": search_structured.get("unique_results", 0),
            "total_raw_results": search_structured.get("total_raw_results", 0),
            "top_domains": search_structured.get("top_domains", []),
            "merged_results": candidates,
            "fetched_pages": fetched_pages,
            "fetch_errors": fetch_structured.get("url_errors", []),
            "fetch_cache_hits": fetch_structured.get("cache_hits", 0),
            "fetch_elapsed_ms": fetch_structured.get("elapsed_ms", 0.0),
            "rendered": render_mode != "off",
            "render_mode": render_mode,
            "render_wait_ms": render_wait_ms,
        }
        meta = {
            "search": search_meta,
            "fetch": fetch_meta,
            "merged_results": candidates,
            "fetched_pages": fetched_pages,
            "rendered": render_mode != "off",
            "render_mode": render_mode,
            "render_wait_ms": render_wait_ms,
            "token_estimate": approximate_tokens(text),
        }
        await _ctx_info(
            ctx,
            "research completed",
            queries=len(search_structured.get("queries") or queries),
            fetched=len(fetched_pages),
            render_mode=render_mode,
        )
        return make_result(text, structured=structured, meta=meta)

    async def fetch_url(
        self,
        *,
        url: str,
        max_excerpt_chars: int | None = None,
        max_links: int = 8,
        rendered: bool = False,
        render_wait_ms: int | None = None,
        ttl: int | None = None,
        ctx: Context | None = None,
    ) -> CallToolResult:
        try:
            normalized = normalize_requested_url(url)
        except ValueError as exc:
            return make_result(str(exc), structured={"error": str(exc)}, meta={"error": str(exc)}, is_error=True)

        excerpt_limit = max_excerpt_chars or self.settings.default_excerpt_chars
        try:
            outcome = await self._fetch_once(
                url=normalized,
                max_excerpt_chars=excerpt_limit,
                max_links=max_links,
                rendered=rendered,
                render_wait_ms=render_wait_ms,
                ttl=ttl,
            )
        except (BackendRequestError, RenderedFetchError, httpx.HTTPError, ValueError) as exc:
            return make_result(
                f"fetch_url failed: {exc}",
                structured={"error": str(exc), "url": normalized},
                meta={"error": str(exc), "url": normalized},
                is_error=True,
            )

        await _ctx_info(
            ctx,
            "fetch_url completed",
            url=normalized,
            cache_hit=outcome.cache_hit,
            render_mode=outcome.render_mode,
            elapsed_ms=round(outcome.elapsed_ms, 2),
        )

        text, structured, meta = self._render_fetch(outcome, max_excerpt_chars=excerpt_limit, max_links=max_links)
        return make_result(text, structured=structured, meta=meta)

    async def fetch_many(
        self,
        *,
        urls: list[str],
        max_excerpt_chars: int | None = None,
        max_links: int = 8,
        rendered: bool = False,
        render_wait_ms: int | None = None,
        concurrency: int | None = None,
        ttl: int | None = None,
        ctx: Context | None = None,
    ) -> CallToolResult:
        clean_urls: list[str] = []
        seen_urls: set[str] = set()
        errors: list[dict[str, Any]] = []
        effective_render_wait_ms = self.settings.render_wait_ms if render_wait_ms is None else render_wait_ms
        for raw_url in urls:
            try:
                normalized = normalize_requested_url(raw_url)
                canonical = _canonical_fetch_url(normalized)
            except ValueError as exc:
                errors.append({"url": raw_url, "error": str(exc)})
                continue
            if canonical in seen_urls:
                continue
            seen_urls.add(canonical)
            clean_urls.append(normalized)

        if not clean_urls:
            return make_result(
                "fetch_many failed: at least one valid URL is required",
                structured={"error": "at least one valid URL is required", "url_errors": errors},
                meta={"error": "at least one valid URL is required", "url_errors": errors},
                is_error=True,
            )

        limit = max(1, concurrency or self.settings.fetch_concurrency)
        excerpt_limit = max_excerpt_chars or self.settings.default_excerpt_chars
        outcomes: list[FetchOutcome] = []
        started = time.perf_counter()

        async def run_url(index: int, normalized_url: str) -> FetchOutcome | None:
            try:
                await _ctx_report_progress(ctx, index, len(clean_urls), f"fetching {normalized_url}")
                return await self._fetch_once(
                    url=normalized_url,
                    max_excerpt_chars=excerpt_limit,
                    max_links=max_links,
                    rendered=rendered,
                    render_wait_ms=effective_render_wait_ms,
                    ttl=ttl,
                )
            except Exception as exc:  # noqa: BLE001
                errors.append({"url": normalized_url, "error": str(exc)})
                return None

        pending = list(enumerate(clean_urls, start=1))
        while pending:
            batch = pending[:limit]
            pending = pending[limit:]
            batch_outcomes = await asyncio.gather(*(run_url(index, url) for index, url in batch))
            for item in batch_outcomes:
                if item is not None:
                    outcomes.append(item)

        elapsed_ms = (time.perf_counter() - started) * 1000
        if not outcomes:
            return make_result(
                "fetch_many failed: all URLs failed",
                structured={"error": "all URLs failed", "url_errors": errors},
                meta={"error": "all URLs failed", "url_errors": errors},
                is_error=True,
            )

        documents = [
            {
                "requested_url": outcome.url,
                "render_mode": outcome.render_mode,
                "document": {
                    **outcome.document.to_summary(),
                    "links": outcome.document.links[:max_links],
                    "render_mode": outcome.render_mode,
                },
                "cache_hit": outcome.cache_hit,
                "elapsed_ms": round(outcome.elapsed_ms, 2),
                "excerpt": outcome.document.excerpt,
                "request_url": outcome.request_url,
            }
            for outcome in outcomes
        ]
        render_mode = _summarize_render_modes([item["render_mode"] for item in documents])
        top_domains = Counter(domain_from_url(item["document"].get("final_url") or item["document"].get("url") or item["requested_url"]) for item in documents)
        text_lines = [
            f"URLs: {len(clean_urls)}",
            f"Successful: {len(outcomes)}",
            f"Cache hits: {sum(1 for item in outcomes if item.cache_hit)}",
            f"Elapsed: {elapsed_ms:.1f} ms",
            f"Rendered fetch: {render_mode}",
        ]
        if top_domains:
            text_lines.append(
                "Top domains: "
                + ", ".join(f"{domain} ({count})" for domain, count in top_domains.most_common(5) if domain)
            )
        if errors:
            text_lines.append(f"URL errors: {len(errors)}")
        text_lines.append("")
        text_lines.append("Fetched pages:")
        for index, item in enumerate(documents, start=1):
            document = item["document"]
            title = document.get("title") or document.get("final_url") or item["requested_url"]
            final_url = document.get("final_url") or item["requested_url"]
            text_lines.append(f"{index}. [{title}]({final_url})")
            text_lines.append(
                f"   {document.get('content_type')} | cache={'hit' if item['cache_hit'] else 'miss'} | rendered={item.get('render_mode') or ('forced' if document.get('rendered') else 'no')} | {document.get('word_count', 0)} words"
            )
            excerpt = clean_text(str(item.get("excerpt") or ""))
            if excerpt:
                text_lines.append(f"   {truncate_text(excerpt, 280)}")

        text = "\n".join(text_lines).strip()
        structured = {
            "urls": clean_urls,
            "successful_urls": len(outcomes),
            "url_errors": errors,
            "elapsed_ms": round(elapsed_ms, 2),
            "cache_hits": sum(1 for item in outcomes if item.cache_hit),
            "documents": documents,
            "rendered": render_mode != "off",
            "render_mode": render_mode,
            "render_wait_ms": effective_render_wait_ms,
        }
        meta = {
            "urls": clean_urls,
            "documents": [
                {
                    "requested_url": outcome.url,
                    "cache_hit": outcome.cache_hit,
                    "elapsed_ms": round(outcome.elapsed_ms, 2),
                    "cache_key": outcome.cache_key,
                    "request_url": outcome.request_url,
                    "document": {
                        **_document_cache_payload(outcome.document),
                        "excerpt": outcome.document.excerpt,
                    },
                    "render_mode": outcome.render_mode,
                }
                for outcome in outcomes
            ],
            "url_errors": errors,
            "rendered": render_mode != "off",
            "render_mode": render_mode,
            "render_wait_ms": effective_render_wait_ms,
            "token_estimate": approximate_tokens(text),
        }
        await _ctx_info(
            ctx,
            "fetch_many completed",
            urls=len(clean_urls),
            successful=len(outcomes),
            render_mode=render_mode,
            elapsed_ms=round(elapsed_ms, 2),
        )
        return make_result(text, structured=structured, meta=meta)

    async def health(self, *, ctx: Context | None = None) -> CallToolResult:
        started = time.perf_counter()
        try:
            ping = await self.search_client.ping()
        except Exception as exc:  # noqa: BLE001
            return make_result(
                f"health failed: {exc}",
                structured={"ok": False, "error": str(exc)},
                meta={"ok": False, "error": str(exc)},
                is_error=True,
            )
        elapsed_ms = (time.perf_counter() - started) * 1000
        hits, misses = self.cache.stats()
        render_status = self.render_client.status()
        configured_backends = 1 + len(self.settings.normalized_fallback_base_urls)
        render_dependency = "installed" if render_status.get("playwright_installed") else "missing"
        render_browser = render_status.get("browser_executable") or "none"

        free_count = len(getattr(self.search_client, "_free_clients", []))
        use_free = getattr(self.settings, "use_free_providers", False)
        parallel = getattr(self.settings, "free_parallel", False)
        free_list = list(getattr(self.settings, "free_providers", []))

        text = "\n".join(
            [
                "searxng-mcp: healthy",
                f"backend: {ping.backend_url} ({ping.status_code} in {ping.elapsed_ms:.1f} ms)",
                f"configured backends: {configured_backends}",
                f"free public providers: {free_count} (use_free={use_free}, parallel={parallel})",
                f"render dependency: {render_dependency}",
                f"render browser candidate: {render_browser}",
                f"render auto fallback: {'enabled' if self.settings.render_auto_fallback else 'disabled'}",
                f"health check elapsed: {elapsed_ms:.1f} ms",
                f"cache entries: {len(self.cache)} | cache volume: {self.cache.volume()} bytes | hits: {hits} | misses: {misses}",
            ]
        )
        await _ctx_info(ctx, "health check completed", backend=ping.backend_url, elapsed_ms=round(elapsed_ms, 2))
        structured = {
            "ok": True,
            "backend_url": ping.backend_url,
            "backend_status": ping.status_code,
            "backend_elapsed_ms": round(ping.elapsed_ms, 2),
            "health_elapsed_ms": round(elapsed_ms, 2),
            "configured_backends": configured_backends,
            "free_public_providers": {
                "enabled": use_free,
                "parallel": parallel,
                "count": free_count,
                "list": free_list,
            },
            "render_support": render_status,
            "render_auto_fallback": self.settings.render_auto_fallback,
            "cache_entries": len(self.cache),
            "cache_volume_bytes": self.cache.volume(),
            "cache_hits": hits,
            "cache_misses": misses,
        }
        meta = {
            "backend_url": ping.backend_url,
            "configured_backends": configured_backends,
            "free_public_providers": {
                "enabled": use_free,
                "parallel": parallel,
                "count": free_count,
                "list": free_list[:4],  # first few in meta to keep small
            },
            "render_support": render_status,
            "cache_dir": str(self.settings.cache_dir),
            "cache_entries": len(self.cache),
            "cache_volume_bytes": self.cache.volume(),
        }
        return make_result(text, structured=structured, meta=meta)
