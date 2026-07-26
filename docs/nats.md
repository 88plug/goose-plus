# Native NATS event publishing — goose-plus

goose-plus can publish a best-effort **event firehose** to a [NATS](https://nats.io/)
bus for fleet observability and agent-to-agent buses. It is **opt-in** and
**never** affects the agent if the broker is slow or down.

## Enable

```bash
export GOOSE_NATS_URL="nats://localhost:4222"
# optional subject prefix (default: "goose")
export GOOSE_NATS_SUBJECT="goose"
```

With `GOOSE_NATS_URL` unset, the feature is a complete no-op.

## Subjects & payload

One subject per session and event type:

```
<prefix>.<session_id>.message.user
<prefix>.<session_id>.message.assistant
<prefix>.<session_id>.tool.requested
<prefix>.<session_id>.tool.responded
```

Subscribe selectively, e.g. `goose.>` (everything), `goose.*.tool.>` (all tool
events), `goose.<session>.>` (one session). The session id is sanitized to a
single NATS token.

Each message is a versioned JSON envelope:

```json
{
  "v": 1,
  "type": "message.assistant",
  "ts": "2026-06-23T12:34:56.789Z",
  "session_id": "019ef2e3-...",
  "payload": { "role": "assistant", "text": "...", "content_kinds": ["text"] }
}
```

Text is truncated to 8 KiB so a large tool output never produces an oversized
message.

## Design (why it's safe)

Built on `async-nats` 0.49 (Core NATS, at-most-once):

- **Non-blocking:** the agent path only `try_send`s onto a bounded channel and
  **drops on overflow** — a slow/dead broker never stalls a turn.
- **Startup-safe:** the client connects in the background with
  `retry_on_initial_connect` + unlimited reconnects, so a missing broker never
  fails startup and self-heals when the broker returns.
- **Best-effort telemetry**, not a transactional log. (JetStream/at-least-once is
  a future opt-in for those who need durability.)

Every published envelope also carries:

- `seq` — a process-global monotonic counter, so a subscriber can order events
  from one instance even if NATS delivery reorders them.
- `instance` — who produced it: `GOOSE_NATS_INSTANCE` if set, else
  `<HOSTNAME>:<pid>`. Lets a fleet attribute each event to a specific goose.

Dropped events (broker slow/full channel) increment an internal counter and are
logged at debug, so silent loss is observable rather than invisible.

## Drive goose over NATS (inbound)

The bus is bidirectional. A `goosed-plus` instance can be **driven** over NATS:
publish a command and it runs a turn and publishes the reply. Opt-in and
**off by default**:

```bash
export GOOSE_NATS_URL="nats://localhost:4222"
export GOOSE_NATS_DRIVE=true          # enable the inbound subscriber (goosed-plus)
export GOOSE_PROVIDER=... GOOSE_MODEL=...   # used to bootstrap a turn headlessly
goosed-plus agent
```

`goosed-plus` queue-subscribes (group `goose-drive`, so multiple instances share the
work) to `<prefix>.cmd`. Send a JSON command — request/reply gets the answer on
your inbox, or omit the reply subject and read `<prefix>.<session>.reply`:

```bash
# request/reply: drives a turn and returns the reply
nats req goose.cmd '{"session_id":"demo","prompt":"What is 2+2?"}'
```

The reply is a `drive.reply` envelope (`payload.text`), or `drive.error` on
failure. Driving is gated separately from publishing because it is more
sensitive — treat the NATS subject as a trusted control plane.

## Coordination (claim/lease) — agents that don't fight

When multiple goose-plus instances (or their subagents) share a bus, they can
**coordinate over NATS instead of clobbering each other**: before mutating a
file, an instance *claims* it; another instance that tries the same file sees
the live claim and waits (or proceeds with a warning) rather than racing.

Opt-in and **off by default**:

```bash
export GOOSE_NATS_URL="nats://localhost:4222"
export GOOSE_NATS_COORD=true        # enable claim/lease coordination
# requires a JetStream-enabled broker:
nats-server -js
```

Mechanism — a true distributed lock built on **JetStream KV**:

- A claim is `Store::create` on the `goose_coord` bucket (an atomic
  compare-and-set), so exactly one instance can hold a resource at a time.
- The bucket's `max_age` is the **lease TTL** (default 60s); the holder
  heartbeats to renew, so if an instance dies the claim expires and others
  can take over — no stuck locks.
- Releasing (or dropping the lease) deletes the key.
- Every claim/release also publishes a `coord.claim` / `coord.release` event so
  the fleet has a **live view** (no drift): `nats sub 'goose.coord.>'`.

Safe by design, like the firehose: when `GOOSE_NATS_COORD` is off (the common
single-user case) every claim is an instant no-op grant — zero behavior change.
If the broker lacks JetStream, coordination logs once and degrades to no-op
granted; every broker call is bounded by a short timeout so a slow/dead broker
never stalls a turn.

Today the developer `write`/`edit` tools claim the target file before writing.

```bash
# prove the lock: two instances, same file
nats-server -js &
nats kv add goose_coord
nats kv create goose_coord file_x A   # instance A: granted
nats kv create goose_coord file_x B   # instance B: DENIED while A holds it
nats kv del  goose_coord file_x       # A releases
nats kv create goose_coord file_x B   # B: now granted
```

## Try it

```bash
# terminal 1
nats-server
# terminal 2
nats sub 'goose.>'
# terminal 3
GOOSE_NATS_URL=nats://localhost:4222 goose-plus run -t "hello"
```
