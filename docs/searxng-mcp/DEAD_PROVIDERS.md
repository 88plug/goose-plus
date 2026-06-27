# Dead / Unreliable Free SearXNG Providers

This file tracks providers that were discovered in session logs or tested but are currently dead, rate-limited, or unreliable for automated use.

**These are NOT used by default in searxng-mcp.**

They are kept here purely for reference and possible future re-testing.

## Current Dead / Unreliable List (as of 2026-06-27)

- https://search.bus-hit.me
- https://search.ononoki.org
- https://searx.be
- https://searx.becomesovran.com
- https://searx.bjornw.nl
- https://searx.fossencdi.org
- https://searx.ox2.fr
- https://searx.priv.au
- https://searx.space
- https://searxng.bjornw.nl
- https://searxng.bmj407.com

## Notes

- Many public instances aggressively block non-browser clients or `format=json`.
- Some return CAPTCHA / "making sure you're not a bot" even on normal HTML requests.
- Only the verified working set (see `settings.py` `DEFAULT_FREE_PROVIDERS` or the main documentation) is used at runtime.
- Fast-fail logic in the client makes it safe to experiment with adding more, but we deliberately keep the default list to only what demonstrably works.

Last updated: 2026-06-27
