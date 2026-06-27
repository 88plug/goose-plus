# SearXNG Sources - Full Discovery vs Pre-wired Parallel Set

## Working Providers (Pre-wired DEFAULT_FREE_PROVIDERS — 8)

**These 8 are the only ones used by default.**

They are **always** queried in full parallel (no concurrency limit whatsoever), with fast-fail + HTML fallback + merge.

This is the "secret sauce".

1. https://searx.tiekoetter.com
2. https://baresearch.org
3. https://search.2b9t.xyz
4. https://search.abohiccups.com
5. https://searxng.site
6. https://failsearx.culturanerd.it
7. https://searx.prvcy.eu
8. https://search.bladerunn.in

## All Sources Discovered in Session Logs (19 total)

Extracted from `~/.local/state/goose/logs/cli/2026-06-27/20260627_103540.log`.

Dead/unreliable ones are tracked separately in `DEAD_PROVIDERS.md` (never used in active code or DEFAULT lists).

- https://baresearch.org ★
- https://failsearx.culturanerd.it ★
- https://search.2b9t.xyz ★
- https://search.abohiccups.com ★
- https://search.bladerunn.in ★
- https://search.bus-hit.me
- https://search.ononoki.org
- https://searx.be
- https://searx.becomesovran.com
- https://searx.bjornw.nl
- https://searx.fossencdi.org
- https://searx.ox2.fr
- https://searx.priv.au
- https://searx.prvcy.eu ★
- https://searx.space
- https://searx.tiekoetter.com ★
- https://searxng.bjornw.nl
- https://searxng.bmj407.com
- https://searxng.site ★

★ = Working / pre-wired (in DEFAULT_FREE_PROVIDERS)

See `DEAD_PROVIDERS.md` for the explicit list of dead/unreliable providers.
