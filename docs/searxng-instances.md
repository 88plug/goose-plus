# Free SearXNG Instances - Full Engine Count Scan (2026-06-27)

**Source:** Session logs at `~/.local/state/goose/logs/cli/2026-06-27/20260627_103540.log` (egress events + discovery via searx.space)

**Method:**
- Fetched `/config` (JSON) for structured enabled engines where available.
- Fetched `/preferences` and parsed **all** `<input type="checkbox" name="engine_..." checked>` (or id starting with `engine_`) using BeautifulSoup.
- "Engines" = highest reported count (preferences usually higher as it lists per-category variants like `wikipedia__general`).
- Also ran quick `/search` availability tests.

## Working Instances (sorted by enabled search engines from /preferences)

| Rank | Instance                        | Engines (prefs) | Engines (config) | Best | Sample Enabled Engines (first few)                  | Search Test          |
|------|---------------------------------|-----------------|------------------|------|-----------------------------------------------------|----------------------|
| 1    | https://searx.tiekoetter.com    | **272**         | 23               | prefs | openlibrary__general, dictzone__general, libretranslate__general, mozhi__general, mymemory_translated__general | OK (html)           |
| 2    | https://baresearch.org          | **195**         | 84               | prefs | searchmysite__general, wiby__general, openlibrary__general, mozhi__general, bing__general | OK (html)           |
| 3    | https://searxng.site            | **194**         | 79               | prefs | openlibrary__general, mozhi__general, brave__general, duckduckgo__general, google__general | 403 (blocked)       |
| 4    | https://search.2b9t.xyz         | **189**         | 84               | prefs | openlibrary__general, mozhi__general, bing__general, mojeek__general, presearch__general | OK (html)           |
| 5    | https://searx.be                | **187**         | N/A              | prefs | openlibrary__general, dictzone__general, mozhi__general, brave__general, duckduckgo__general | 403 (blocked)       |
| 6    | https://search.abohiccups.com   | **164**         | 84               | prefs | openlibrary__general, mozhi__general, bing__general, mojeek__general, presearch__general | OK (html)           |
| 7    | https://failsearx.culturanerd.it| **162**         | 79               | prefs | openlibrary__general, mozhi__general, bing__general, brave__general, google__general | OK (html)           |
| 8    | https://searx.prvcy.eu          | **117**         | 78               | prefs | openlibrary__general, libretranslate__general, mozhi__general, bing__general, mojeek__general | 403 (blocked)       |
| 9    | https://search.bladerunn.in     | 7               | **30**           | config| arxiv, bandcamp, bing, bing images, bing videos     | OK (html)           |

## Unreachable / No Engine Data (as of 2026-06-27 ~11:04)

- https://search.ononoki.org — 403 on search, no prefs/config
- https://searx.becomesovran.com — connection error
- https://searx.bjornw.nl — connection error
- https://searx.fossencdi.org — connection error
- https://searx.ox2.fr — connection error
- https://searx.priv.au — connection error
- https://searx.space — 404 (instance list page, not a full instance)
- https://searxng.bjornw.nl — connection error

## Summary Notes

- **Preferences scan completed for all candidates** (using proper `engine_` checkbox parsing).
- Many public SearXNG instances return 403 on automated searches due to bot protection, even when `/preferences` and `/config` are fully readable.
- Highest engine counts come from `/preferences` (includes category variants).
- Fresh verification at 11:04 confirmed the same top numbers (searx.tiekoetter.com still leads at 272).

Last updated: 2026-06-27 11:04
