# A2A (Agent2Agent) support — goose-plus

goose-plus speaks the [A2A protocol](https://a2a-protocol.org/) both ways: it can
**be called** by other agents (server) and can **call** other A2A agents (client).
The wire format is the **v0.3 JSON-RPC binding** — the form the reference SDKs
(`a2a-js` v0.3, `a2a-python`'s v0.3 compatibility adapter) interoperate with today.

## Server — expose goose as an A2A agent

Opt-in. Enable it and (re)start `goosed`:

```bash
export GOOSE_A2A_ENABLE=true
# optional: the externally reachable JSON-RPC URL advertised in the Agent Card
# (defaults to http://<bind-addr>/a2a). Set this when behind a proxy.
export GOOSE_A2A_URL="https://my-host.example.com/a2a"
```

The A2A endpoints are mounted **without** the `x-secret-key` middleware (remote
agents authenticate per the Agent Card's own security scheme, not goose's internal
secret). They are not exposed at all unless `GOOSE_A2A_ENABLE` is set.

### Endpoints

| Method / Path | Purpose |
|---|---|
| `GET /.well-known/agent-card.json` | Agent Card (also served at the legacy `/.well-known/agent.json`) |
| `POST /a2a` | JSON-RPC 2.0: `message/send`, `tasks/get`, `tasks/cancel` |

`message/send` runs a goose turn (the A2A `contextId` maps to a goose session) and
returns a **completed `Task`** whose `status.message` and `history` carry the
agent's reply. `tasks/get` returns a previously-completed task; `tasks/cancel`
returns `-32002 TaskNotCancelable` (tasks complete synchronously).

Streaming (`message/stream`) is **advertised as unsupported** (`capabilities.streaming: false`)
until it lands — goose does not claim a capability it doesn't implement.

### Example

```bash
curl http://localhost:3000/.well-known/agent-card.json

curl -X POST http://localhost:3000/a2a -H 'content-type: application/json' -d '{
  "jsonrpc":"2.0","id":1,"method":"message/send",
  "params":{"message":{"kind":"message","role":"user","messageId":"m1",
    "parts":[{"kind":"text","text":"Summarize the README"}]}}
}'
```

## Client — call another A2A agent from goose

When the A2A client is wired as a tool, goose can delegate to a remote agent:
it fetches the remote Agent Card from `<base>/.well-known/agent-card.json`
(legacy fallback included), sends `message/send`, and returns the reply text
(extracted from the result `Message`, the `Task.status.message`, the last agent
turn in `history`, or artifacts — in that order).

## Notes & limitations

- **Binding:** v0.3 JSON-RPC. v1.0 (PascalCase methods, `TASK_STATE_*` enums) is
  not yet negotiated; an empty `A2A-Version` header is assumed `0.3` per spec.
- **Parts:** text and image file parts map cleanly to goose message content;
  other file/data parts are flattened to text (goose has no native File/Data
  message content).
- **No third-party A2A crate:** implemented directly against the v0.3 JSON Schema
  — the Rust A2A ecosystem is fragmented and the needed surface is small.
