from __future__ import annotations

from dataclasses import dataclass
from typing import Any
import asyncio
import time

import httpx

from .html_search import make_search_payload_from_html
from .settings import Settings


class BackendRequestError(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        url: str | None = None,
        status_code: int | None = None,
        body: str | None = None,
    ) -> None:
        super().__init__(message)
        self.url = url
        self.status_code = status_code
        self.body = body


@dataclass(slots=True)
class BackendResponse:
    backend_url: str
    url: str
    status_code: int
    elapsed_ms: float
    payload: dict[str, Any]


@dataclass(slots=True)
class FetchResponse:
    url: str
    status_code: int
    elapsed_ms: float
    response: httpx.Response


def _is_valid_search_payload(payload: Any) -> bool:
    if not isinstance(payload, dict):
        return False

    results = payload.get("results")
    if not isinstance(results, list):
        return False
    if any(not isinstance(item, dict) for item in results):
        return False

    for key in ("answers", "corrections", "infoboxes", "suggestions", "unresponsive_engines"):
        value = payload.get(key)
        if value is not None and not isinstance(value, list):
            return False
    return True


def _should_fallback(error: BackendRequestError) -> bool:
    message = str(error).lower()
    if "non-json" in message or "malformed" in message or "unexpected search payload" in message:
        return True
    if error.status_code is None:
        return True
    return error.status_code in {403, 404, 408, 425, 429} or error.status_code >= 500


class SearxngClient:
    def __init__(self, settings: Settings, *, transport: Any | None = None) -> None:
        self.settings = settings
        limits = httpx.Limits(
            max_connections=settings.search_connections,
            max_keepalive_connections=settings.search_keepalive,
        )
        timeout = httpx.Timeout(settings.search_timeout, connect=min(settings.search_timeout, 5.0))
        headers = {
            "Accept": "application/json",
            "User-Agent": settings.mcp_user_agent,
        }
        base_urls = [settings.normalized_base_url, *settings.normalized_fallback_base_urls]
        self._backends: list[tuple[str, httpx.AsyncClient]] = []
        seen: set[str] = set()
        for base_url in base_urls:
            if base_url in seen:
                continue
            seen.add(base_url)
            self._backends.append(
                (
                    base_url,
                    httpx.AsyncClient(
                        base_url=base_url,
                        follow_redirects=True,
                        headers=headers,
                        limits=limits,
                        timeout=timeout,
                        trust_env=settings.trust_env,
                        http2=True,
                        transport=transport,
                    ),
                )
            )

        # Pre-create lightweight clients for free public providers (the secret sauce pool)
        self._free_clients: list[tuple[str, httpx.AsyncClient]] = []
        free_seen: set[str] = set()
        for base in settings.free_providers:
            b = base.rstrip("/")
            if b in free_seen:
                continue
            free_seen.add(b)
            self._free_clients.append(
                (
                    b,
                    httpx.AsyncClient(
                        base_url=b,
                        follow_redirects=True,
                        headers={
                            "Accept": "application/json,text/html;q=0.9,*/*;q=0.8",
                            "User-Agent": settings.mcp_user_agent,
                        },
                        limits=limits,
                        timeout=timeout,
                        trust_env=settings.trust_env,
                        http2=True,
                        transport=transport,
                    ),
                )
            )

    @staticmethod
    def _clean_params(params: dict[str, Any]) -> dict[str, Any]:
        return {key: value for key, value in params.items() if value not in (None, "", [], ())}

    async def _try_search_one(
        self, backend_url: str, client: httpx.AsyncClient, params: dict[str, Any]
    ) -> BackendResponse | None:
        """
        Try a single backend.
        1. Try with format=json (fast path)
        2. On soft failure (403,429,non-json, etc) fall back to raw HTML + parse.
        Returns BackendResponse on success, None on hard failure.
        """
        cleaned = self._clean_params(params)

        # --- JSON attempt ---
        started = time.perf_counter()
        try:
            resp = await client.get("/search", params=cleaned)
            elapsed = (time.perf_counter() - started) * 1000
            if resp.status_code < 400:
                try:
                    payload = resp.json()
                    if _is_valid_search_payload(payload):
                        return BackendResponse(
                            backend_url=backend_url,
                            url=str(resp.request.url),
                            status_code=resp.status_code,
                            elapsed_ms=elapsed,
                            payload=payload,
                        )
                except Exception:
                    pass  # fall to HTML
            # If we reach here, JSON path "failed softly"
            if not _should_fallback(
                BackendRequestError(
                    f"soft fail {resp.status_code}",
                    url=str(resp.request.url),
                    status_code=resp.status_code,
                    body=resp.text[:800],
                )
            ):
                return None
        except Exception as exc:
            if not _should_fallback(BackendRequestError(str(exc), url=backend_url)):
                return None

        # --- HTML fallback (the power move for public instances) ---
        try:
            # Remove format so we get the nice HTML page
            html_params = {k: v for k, v in cleaned.items() if k != "format"}
            started2 = time.perf_counter()
            resp2 = await client.get(
                "/search",
                params=html_params,
                headers={
                    "Accept": "text/html,application/xhtml+xml",
                    "User-Agent": self.settings.mcp_user_agent,
                },
            )
            elapsed2 = (time.perf_counter() - started2) * 1000
            if resp2.status_code < 400:
                ctype = resp2.headers.get("content-type", "")
                if "html" in ctype or resp2.text.strip().startswith("<"):
                    payload = make_search_payload_from_html(
                        resp2.text, base_url=backend_url, query=cleaned.get("q", "")
                    )
                    if _is_valid_search_payload(payload):
                        return BackendResponse(
                            backend_url=backend_url,
                            url=str(resp2.request.url),
                            status_code=resp2.status_code,
                            elapsed_ms=elapsed2,
                            payload=payload,
                        )
        except Exception:
            pass

        return None

    async def _search_on_backend(self, backend_url: str, client: httpx.AsyncClient, params: dict[str, Any]) -> BackendResponse:
        """Legacy single-backend JSON path (kept for compatibility)."""
        cleaned = self._clean_params(params)
        started = time.perf_counter()
        response = await client.get("/search", params=cleaned)
        elapsed_ms = (time.perf_counter() - started) * 1000
        if response.status_code >= 400:
            raise BackendRequestError(
                f"SearXNG search failed with HTTP {response.status_code}",
                url=str(response.request.url),
                status_code=response.status_code,
                body=response.text[:1200],
            )
        try:
            payload = response.json()
        except Exception as exc:  # noqa: BLE001
            raise BackendRequestError(
                f"SearXNG returned non-JSON search response: {exc!r}",
                url=str(response.request.url),
                status_code=response.status_code,
                body=response.text[:1200],
            ) from exc
        if not _is_valid_search_payload(payload):
            raise BackendRequestError(
                "SearXNG returned a malformed search payload",
                url=str(response.request.url),
                status_code=response.status_code,
                body=response.text[:1200],
            )
        return BackendResponse(
            backend_url=backend_url,
            url=str(response.request.url),
            status_code=response.status_code,
            elapsed_ms=elapsed_ms,
            payload=payload,
        )

    async def search(self, params: dict[str, Any]) -> BackendResponse:
        """Single result (first success). Used for classic single-backend mode."""
        errors: list[BackendRequestError] = []
        for backend_url, client in self._backends:
            try:
                return await self._search_on_backend(backend_url, client, params)
            except BackendRequestError as exc:
                errors.append(exc)
                if not _should_fallback(exc):
                    raise
        if errors:
            message = "; ".join(str(error) for error in errors)
            raise BackendRequestError(f"SearXNG search failed across backends: {message}")
        raise BackendRequestError("SearXNG search failed: no backends configured")

    async def search_parallel(
        self, params: dict[str, Any], *, max_concurrency: int | None = None
    ) -> list[BackendResponse]:
        """
        THE SECRET SAUCE.

        Fire the same search parameters against ALL configured free public providers
        (and optionally the main backend) **concurrently**, not one-after-another.

        Each backend tries:
          1. format=json
          2. HTML fallback + parser (if JSON blocked)

        Returns list of successful BackendResponses (can be 0 to N).
        Results are meant to be merged higher up (in service layer).
        """
        concurrency = max_concurrency or getattr(self.settings, "free_max_concurrency", 8)
        sem = asyncio.Semaphore(max(1, concurrency))

        # Build the full pool: main backends first, then the free public pool
        pool: list[tuple[str, httpx.AsyncClient]] = []
        seen: set[str] = set()
        for b in self._backends:
            if b[0] not in seen:
                seen.add(b[0])
                pool.append(b)
        for f in self._free_clients:
            if f[0] not in seen:
                seen.add(f[0])
                pool.append(f)

        if not pool:
            return []

        async def _run_one(backend_url: str, client: httpx.AsyncClient) -> BackendResponse | None:
            async with sem:
                try:
                    return await self._try_search_one(backend_url, client, params)
                except Exception:
                    return None

        tasks = [asyncio.create_task(_run_one(url, cli)) for url, cli in pool]
        results = await asyncio.gather(*tasks, return_exceptions=True)

        successes: list[BackendResponse] = []
        for r in results:
            if isinstance(r, BackendResponse):
                successes.append(r)
            # ignore exceptions and Nones (failed backends)

        return successes

    async def _ping_backend(self, backend_url: str, client: httpx.AsyncClient) -> BackendResponse:
        started = time.perf_counter()
        response = await client.get("/")
        elapsed_ms = (time.perf_counter() - started) * 1000
        if response.status_code >= 400:
            raise BackendRequestError(
                f"SearXNG ping failed with HTTP {response.status_code}",
                url=str(response.request.url),
                status_code=response.status_code,
                body=response.text[:1200],
            )
        return BackendResponse(
            backend_url=backend_url,
            url=str(response.request.url),
            status_code=response.status_code,
            elapsed_ms=elapsed_ms,
            payload={},
        )

    async def ping(self) -> BackendResponse:
        errors: list[BackendRequestError] = []
        for backend_url, client in self._backends:
            try:
                return await self._ping_backend(backend_url, client)
            except BackendRequestError as exc:
                errors.append(exc)
                if not _should_fallback(exc):
                    raise
        if errors:
            message = "; ".join(str(error) for error in errors)
            raise BackendRequestError(f"SearXNG ping failed across backends: {message}")
        raise BackendRequestError("SearXNG ping failed: no backends configured")

    async def close(self) -> None:
        for _, client in self._backends:
            await client.aclose()


class FetchClient:
    def __init__(self, settings: Settings) -> None:
        limits = httpx.Limits(
            max_connections=settings.fetch_connections,
            max_keepalive_connections=settings.fetch_keepalive,
        )
        timeout = httpx.Timeout(settings.fetch_timeout, connect=min(settings.fetch_timeout, 5.0))
        self._client = httpx.AsyncClient(
            follow_redirects=True,
            headers={
                "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,text/plain;q=0.8,*/*;q=0.5",
                "User-Agent": settings.mcp_user_agent,
            },
            limits=limits,
            timeout=timeout,
            trust_env=settings.trust_env,
            verify=settings.fetch_verify_tls,
            http2=True,
        )

    async def get(self, url: str) -> FetchResponse:
        started = time.perf_counter()
        response = await self._client.get(url)
        elapsed_ms = (time.perf_counter() - started) * 1000
        if response.status_code >= 400:
            raise BackendRequestError(
                f"Fetch failed with HTTP {response.status_code}",
                url=str(response.request.url),
                status_code=response.status_code,
                body=response.text[:1200],
            )
        return FetchResponse(
            url=str(response.request.url),
            status_code=response.status_code,
            elapsed_ms=elapsed_ms,
            response=response,
        )

    async def close(self) -> None:
        await self._client.aclose()
