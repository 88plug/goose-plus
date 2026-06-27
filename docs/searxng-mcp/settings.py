from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import os

from . import __version__

TRUE_VALUES = {"1", "true", "yes", "on", "y", "t"}

# Working / verified free public SearXNG providers.
# These are ALWAYS used in full parallel (no concurrency limit) + fast-fail + merge.
# This is the "secret sauce".
DEFAULT_FREE_PROVIDERS: tuple[str, ...] = (
    "https://searx.tiekoetter.com",
    "https://baresearch.org",
    "https://search.2b9t.xyz",
    "https://search.abohiccups.com",
    "https://searxng.site",
    "https://failsearx.culturanerd.it",
    "https://searx.prvcy.eu",
    "https://search.bladerunn.in",
)
# Dead/unreliable providers are tracked ONLY in docs/searxng-mcp/DEAD_PROVIDERS.md
# We only ever put verified working ones in DEFAULT_FREE_PROVIDERS.
# Free providers are ALWAYS run in full parallel (no limit whatsoever) + fast-fail + merge.

