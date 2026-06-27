#!/usr/bin/env python3
"""
Demo: searxng-mcp "Secret Sauce" - Full Parallel Free Public Providers

This shows the unique power: one query is sent to ALL pre-wired free SearXNG
instances CONCURRENTLY (not one at a time), results are merged.

Run with the modified 88plug/searxng-mcp code from /tmp/searxng-mcp-src or equivalent.

Usage:
    python docs/searxng-mcp/demo_parallel_free.py "searxng public instances"
"""

import asyncio
import sys
import os

# Make sure we can import the patched version
sys.path.insert(0, "/tmp/searxng-mcp-src/src")

from searxng_mcp.settings import load_settings, DEFAULT_FREE_PROVIDERS
from searxng_mcp.client import SearxngClient
from searxng_mcp.service import _search_params, _merge_query_outcomes, QueryOutcome

async def main():
    query = " ".join(sys.argv[1:]) or "searxng public instances 2026"
    print("=" * 70)
    print("SEARXNG-MCP SECRET SAUCE DEMO")
    print("Full parallel fan-out to free public providers + merge")
    print("=" * 70)
    print(f"Query: {query}")
    print()

    s = load_settings()

    print("Pre-wired free providers (DEFAULT_FREE_PROVIDERS):")
    for p in DEFAULT_FREE_PROVIDERS:
        print(f"  - {p}")
    print(f"\nTotal free providers: {len(DEFAULT_FREE_PROVIDERS)}")
    print(f"use_free_providers={s.use_free_providers}  free_parallel={s.free_parallel}")
    print()

    client = SearxngClient(s)

    params = _search_params(
        query=query,
        categories=s.default_categories,
        engines=None,
        enabled_engines=None,
        disabled_engines=None,
        language=s.default_language,
        pageno=1,
        time_range=None,
        safesearch=s.default_safesearch,
    )

    print("Firing search_parallel() across all free providers + main backend...")
    started = asyncio.get_event_loop().time()
    responses = await client.search_parallel(params, max_concurrency=s.free_max_concurrency)
    elapsed = (asyncio.get_event_loop().time() - started) * 1000

    print(f"Completed in {elapsed:.0f} ms")
    print(f"Successful backends: {len(responses)} / {len(client._free_clients) + len(client._backends)}")
    print()

    if not responses:
        print("No successful responses (most public instances are heavily protected right now).")
        print("This is expected in 2026. The architecture is ready.")
        return

    # Convert to QueryOutcome for merging (same logic service uses)
    outcomes = []
    for r in responses:
        oc = QueryOutcome(
            query=query,
            backend_url=r.backend_url,
            params=params,
            payload=r.payload,
            elapsed_ms=r.elapsed_ms,
            cache_hit=False,
            cache_key="",
            request_url=r.url,
        )
        outcomes.append(oc)

    merged, stats = _merge_query_outcomes(outcomes)

    print("=== MERGED RESULTS (Secret Sauce) ===")
    print(f"Unique results: {stats['unique_results']} (from {stats['total_raw_results']} raw across {len(outcomes)} instances)")
    if stats.get("top_domains"):
        print("Top domains:", ", ".join(f"{d}({c})" for d, c in stats["top_domains"][:5]))
    print()

    for i, hit in enumerate(merged[:8], 1):
        title = hit.best_raw.get("title") or hit.best_raw.get("url")
        url = hit.best_raw.get("url")
        engines = hit.engines
        print(f"{i}. {title}")
        print(f"   {url}")
        print(f"   engines: {', '.join(engines) if engines else 'unknown'} | hits={hit.hit_count}")
        print()

    print("This is the power: results from many independent SearXNG instances")
    print("queried at the same time and intelligently merged.")

if __name__ == "__main__":
    asyncio.run(main())

# Optional: FlareSolverr rendered fallback
#
# Set SEARXNG_FLARESOLVERR_URL=http://localhost:8191 (or your instance)
# to enable a last-resort rendered path for backends that fail direct
# JSON + HTML (e.g. Cloudflare-protected "dead" instances).
#
# This is completely optional and per-backend only.
# It does not change the default full-parallel behavior of the 8 free providers.
#
# Example:
#   SEARXNG_FLARESOLVERR_URL=http://localhost:8191 \
#   python docs/searxng-mcp/demo_parallel_free.py "ai agents"
#
# When enabled, any backend that would otherwise return no results
# will be retried through FlareSolverr (long timeout). Other backends
# continue in parallel as usual.
