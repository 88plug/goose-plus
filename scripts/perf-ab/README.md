# perf-ab — interleaved A/B latency harness

`ab_latency.py` compares two `goose` binaries' latency over the **ACP** interface (black-box,
provider-agnostic). It exists to measure *real* latency deltas against an LLM backend whose
server-side latency is **non-stationary** — which makes the naive approach actively misleading.

## Why this harness exists (hard-won methodology)

These four decisions are load-bearing. Skipping any one of them produces confident-but-wrong
numbers — every one was learned by getting it wrong first.

1. **Interleave arms per trial — never batch.**
   Running *all* of arm A and then *all* of arm B lets the provider's latency drift over the
   wall-clock window land entirely on the arm that ran later, manufacturing phantom 1.2–1.5×
   "regressions" that vanish under interleaving. The harness runs `A,B,A,B,…` so both arms see
   the same drift.

2. **Measure the felt moment, not the aggregate.**
   The latency a human notices is **first-token on the first prompt** (cold cache, right after
   hitting enter) — a one-shot event, not warm throughput. `--mode cold` spawns a fresh process
   per trial and times time-to-first-token. `--mode warm` measures a warmed turn for throughput.
   They are different metrics; don't conflate them.

3. **Hard wall-clock watchdog.**
   A turn that streams forever (runaway generation) is *not* caught by a "no data for N seconds"
   timeout, because data keeps arriving. The harness enforces a deadline regardless of stream
   activity and `kill()`s the process, so a runaway can never hang the run or poison the next
   trial.

4. **Respect variance.**
   Reasoning and code models swing **multiple seconds** run-to-run. A single low-rep median is
   not signal. The harness reports median + spread + n, and prints a NOTE when the measured Δ is
   smaller than the arm's own spread (i.e. inside the noise band). Reasoning models need
   `--reps 20+` before a sub-second delta means anything.

## Usage

```bash
UP_BIN=/path/to/goose-baseline \
PLUS_BIN=/path/to/goose-candidate \
GOOSE_PROVIDER=xai GOOSE_MODEL=grok-4.3 XAI_API_KEY=... \
python3 scripts/perf-ab/ab_latency.py --mode cold --reps 20
```

- `--mode cold` (default): fresh process per trial, measures first-token (felt latency).
- `--mode warm`: one session warmed by a throwaway turn, measures the next turn (throughput).
- `--reps N`: trials per arm (default 20).
- `--think S`: pause after `session/new` before the first prompt (default 2.0s) so a startup
  prewarm can warm the prefix cache, matching real human cadence.
- `--prompt T`, `--timeout S`: the prompt and the hard per-turn deadline.

Stdlib only; no dependencies. Provider config is taken from the environment (`GOOSE_PROVIDER`,
`GOOSE_MODEL`, the provider's API key), exactly as a normal `goose` run would read it.

## Interpreting output

- `Δ (B - A)` negative ⇒ B is faster. The `x` ratio is `B/A`.
- An `input tokens` Δ shows surface differences (system prompt + tool schemas) between the two
  binaries; an `output tokens` Δ near zero confirms no verbosity change is skewing `total`.
- A `NOTE: … within noise` line means raise `--reps` before drawing any conclusion.
