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

## Try it

```bash
# terminal 1
nats-server
# terminal 2
nats sub 'goose.>'
# terminal 3
GOOSE_NATS_URL=nats://localhost:4222 goose run -t "hello"
```
