#!/usr/bin/env python3
"""Interleaved A/B latency harness for two goose binaries (provider-agnostic, via ACP).

This codifies the methodology that the goose-plus performance work proved is necessary to
get TRUE latency deltas out of a non-stationary LLM backend. Read scripts/perf-ab/README.md
for the why; the four load-bearing decisions are:

  1. INTERLEAVE arms per trial (A,B,A,B,...), never batch (all-A then all-B). A provider's
     server-side latency drifts over wall-clock; a batched design hands the drift to whichever
     arm ran later and manufactures phantom wins/losses. Interleaving cancels the drift.
  2. Measure the FELT moment: cold first-token on a FRESH process (what a human feels right
     after hitting enter), separately from warm-turn throughput. They are different metrics.
  3. HARD wall-clock watchdog: a turn that streams forever (runaway generation) is not caught
     by a queue-silence timeout. Enforce a deadline regardless of stream activity, and kill
     the process so a runaway never poisons the next trial.
  4. VARIANCE awareness: reasoning/code models swing seconds run-to-run. Report median + spread
     + n, and flag deltas that sit inside the noise band instead of reporting them as signal.

Usage:
    UP_BIN=/path/to/goose-A PLUS_BIN=/path/to/goose-B \\
    GOOSE_PROVIDER=xai GOOSE_MODEL=grok-4.3 XAI_API_KEY=... \\
    python3 scripts/perf-ab/ab_latency.py --reps 20 --mode cold

    --mode cold : fresh process per trial, measure first-token (felt latency).  [default]
    --mode warm : one session, measure a warmed turn (throughput).
    --reps N    : trials per arm (default 20). Reasoning models need >=20 to beat variance.
    --think S   : seconds to pause after session/new before the first prompt (default 2.0);
                  lets a startup prewarm warm the prefix cache, matching real human cadence.
    --prompt T  : the prompt to send (default a short code task).
    --timeout S : hard per-turn deadline in seconds (default 120).

Exit status is always 0 on a completed run; measurement validity is in the printed report.
"""
import argparse, json, os, queue, statistics as st, subprocess, sys, threading, time

UP = os.environ.get("UP_BIN", "goose-A")
PLUS = os.environ.get("PLUS_BIN", "goose-B")


def _env(label):
    e = dict(os.environ)
    e["HOME"] = f"/tmp/ab_{label}_{time.time_ns()}"
    e.setdefault("GOOSE_DISABLE_KEYRING", "1")
    e.setdefault("GOOSE_DISABLE_SESSION_NAMING", "true")
    os.makedirs(e["HOME"], exist_ok=True)
    return e


class Acp:
    """Minimal ACP client over the binary's stdio, with a hard wall-clock watchdog."""

    def __init__(self, binpath, label):
        self.p = subprocess.Popen(
            [binpath, "acp", "--with-builtin", "developer"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, bufsize=1, env=_env(label),
        )
        self.q = queue.Queue()
        threading.Thread(target=self._pump, daemon=True).start()
        self._id = 0

    def _pump(self):
        for line in self.p.stdout:
            self.q.put(line)
        self.q.put(None)

    def _send(self, method, params):
        self._id += 1
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self._id, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        return self._id

    def _collect(self, mid, timeout, want_ttft=False):
        deadline = time.time() + timeout
        ttft = None
        while True:
            if time.time() > deadline:  # HARD wall-clock deadline (runaway-stream proof)
                return None, ttft
            try:
                line = self.q.get(timeout=max(0.05, deadline - time.time()))
            except queue.Empty:
                return None, ttft
            if line is None:
                return None, ttft
            line = line.strip()
            if not line:
                continue
            if want_ttft and '"agent_message_chunk"' in line and ttft is None:
                ttft = time.time()
            try:
                o = json.loads(line)
            except json.JSONDecodeError:
                continue
            if o.get("method") and o.get("id") is not None and "result" not in o:
                # auto-approve any agent->client request (tool permission, etc.)
                self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": o["id"], "result": {}}) + "\n")
                self.p.stdin.flush()
                continue
            if o.get("id") == mid:
                return o, ttft

    def initialize(self):
        self._collect(self._send("initialize", {"protocolVersion": 1, "clientCapabilities": {}}), 30)

    def new_session(self):
        o, _ = self._collect(self._send("session/new", {"cwd": "/tmp", "mcpServers": []}), 30)
        return ((o or {}).get("result") or {}).get("sessionId")

    def prompt(self, sid, text, timeout):
        t0 = time.time()
        mid = self._send("session/prompt", {"sessionId": sid, "prompt": [{"type": "text", "text": text}]})
        o, ttft = self._collect(mid, timeout, want_ttft=True)
        total = (time.time() - t0) * 1000
        if not o:
            return None  # timed out / runaway
        usage = (o.get("result") or {}).get("usage") or {}
        return {
            "total": total,
            "ttft": (ttft - t0) * 1000 if ttft else None,
            "inp": usage.get("inputTokens"),
            "out": usage.get("outputTokens"),
        }

    def kill(self):
        try:
            self.p.kill()
            self.p.wait(timeout=5)
        except Exception:
            pass


def trial_cold(binpath, prompt, think, timeout):
    """One fresh-process first-prompt trial (the felt latency)."""
    c = Acp(binpath, "cold")
    try:
        c.initialize()
        sid = c.new_session()
        if not sid:
            return None
        time.sleep(think)  # human think-time; lets a startup prewarm complete
        return c.prompt(sid, prompt, timeout)
    finally:
        c.kill()


def trial_warm(binpath, prompt, timeout):
    """One warmed-turn trial: a throwaway turn warms the session, then measure turn 2."""
    c = Acp(binpath, "warm")
    try:
        c.initialize()
        sid = c.new_session()
        if not sid:
            return None
        c.prompt(sid, "Reply: OK", timeout)  # warm-up turn (discarded)
        return c.prompt(sid, prompt, timeout)
    finally:
        c.kill()


def summarize(rows):
    rows = [r for r in rows if r]
    tt = [r["ttft"] for r in rows if r.get("ttft") is not None]
    tot = [r["total"] for r in rows if r.get("total") is not None]
    inp = [r["inp"] for r in rows if r.get("inp") is not None]
    out = [r["out"] for r in rows if r.get("out") is not None]
    med = lambda xs: st.median(xs) if xs else None
    return {
        "n": len(rows),
        "ttft": med(tt), "ttft_min": min(tt) if tt else None, "ttft_max": max(tt) if tt else None,
        "total": med(tot), "inp": med(inp), "out": med(out),
    }


def main():
    ap = argparse.ArgumentParser(description="Interleaved A/B latency harness for two goose binaries.")
    ap.add_argument("--reps", type=int, default=20)
    ap.add_argument("--mode", choices=["cold", "warm"], default="cold")
    ap.add_argument("--think", type=float, default=2.0)
    ap.add_argument("--timeout", type=float, default=120.0)
    ap.add_argument("--prompt", default="Write a Python function fib(n) (iterative). Code only.")
    args = ap.parse_args()

    if not (os.path.exists(UP) or "/" not in UP) or not (os.path.exists(PLUS) or "/" not in PLUS):
        print("Set UP_BIN and PLUS_BIN to the two goose binaries to compare.", file=sys.stderr)

    def one(binpath):
        if args.mode == "cold":
            return trial_cold(binpath, args.prompt, args.think, args.timeout)
        return trial_warm(binpath, args.prompt, args.timeout)

    rows = {"A": [], "B": []}
    for i in range(args.reps):
        # INTERLEAVE per trial: A then B, every rep. This is the load-bearing decision.
        rows["A"].append(one(UP))
        rows["B"].append(one(PLUS))
        print(f"  rep {i + 1}/{args.reps} done", file=sys.stderr)

    a, b = summarize(rows["A"]), summarize(rows["B"])
    metric = "ttft" if args.mode == "cold" else "total"
    da = a.get(metric)
    db = b.get(metric)
    delta = (db - da) if (da is not None and db is not None) else None
    ratio = (db / da) if (da and db) else None

    print(f"\n=== A/B latency ({args.mode}, metric={metric}, reps={args.reps}, interleaved) ===")
    print(f"A (UP_BIN  ={os.path.basename(UP)}): {metric} median {da:.0f}ms (n={a['n']})" if da else f"A: no data (n={a['n']})")
    print(f"B (PLUS_BIN={os.path.basename(PLUS)}): {metric} median {db:.0f}ms (n={b['n']})" if db else f"B: no data (n={b['n']})")
    if a.get("inp") is not None and b.get("inp") is not None:
        print(f"input tokens : A={a['inp']:.0f} B={b['inp']:.0f} (Δ={b['inp'] - a['inp']:+.0f})")
    if a.get("out") is not None and b.get("out") is not None:
        print(f"output tokens: A={a['out']:.0f} B={b['out']:.0f} (Δ={b['out'] - a['out']:+.0f})")
    if delta is not None and ratio is not None:
        print(f"Δ (B - A)    : {delta:+.0f}ms  ({ratio:.2f}x)")
        # VARIANCE flag: if the delta is smaller than A's own observed spread, it is noise.
        spread = (a["ttft_max"] - a["ttft_min"]) if (args.mode == "cold" and a.get("ttft_max")) else None
        if spread and abs(delta) < spread / 2:
            print(f"NOTE: |Δ| {abs(delta):.0f}ms < half of A's spread ({spread:.0f}ms) — likely within noise; "
                  f"raise --reps and re-run before treating as signal.")


if __name__ == "__main__":
    main()
