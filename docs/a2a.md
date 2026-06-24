# A2A (Agent2Agent) support — goose-plus

goose-plus speaks the [A2A protocol](https://a2a-protocol.org/) both ways: it can
**be called** by other agents (server) and can **call** other A2A agents (client).

It is built on our app-plus'd fork **[88plug/a2a-rs](https://github.com/88plug/a2a-rs)**
(`main`, pinned by commit) — upstream `a2aproject/a2a-rs` **v1.0** plus a WebSocket
transport binding and spec fixes. goose-plus carries no hand-rolled protocol code:
the wire types, JSON-RPC + REST routers, agent-card endpoint, client transport
negotiation, and WebSocket transport all come from the crate. The only
goose-specific glue is the bridge to goose's agent and message model.

## Server — expose goose as an A2A agent

Opt-in. Enable it and (re)start `goosed`:

```bash
export GOOSE_A2A_ENABLE=true
# optional: externally reachable HTTP origin advertised in the Agent Card
# (defaults to http://<bind-addr>). Interface URLs are derived from it.
# Set this when behind a proxy.
export GOOSE_A2A_URL="https://my-host.example.com"
```

The A2A endpoints are mounted **without** the `x-secret-key` middleware (remote
agents authenticate per the Agent Card's own security scheme, not goose's internal
secret). They are not exposed at all unless `GOOSE_A2A_ENABLE` is set.

### Endpoints

| Method / Path | Transport | Purpose |
|---|---|---|
| `GET /.well-known/agent-card.json` | — | Agent Card (advertises all three interfaces) |
| `POST /jsonrpc` | JSON-RPC | `message:send`, `message:stream`, `tasks:get`, `tasks:cancel`, … |
| `POST /rest/…` | HTTP+JSON | REST binding of the same service |
| `GET /a2a/ws` | WebSocket | bidirectional streaming over one connection (`a2a.v1` subprotocol) |

A send runs a goose turn (the A2A `contextId` maps to a goose session) and yields
a **completed `Task`** whose `status.message` carries the agent's reply. The
executor streams a `Working` status then the terminal `Completed` task, so
streaming sends and `tasks:subscribe` work as well — `capabilities.streaming` is
advertised as **true**.

### Example

```bash
curl http://localhost:3000/.well-known/agent-card.json

curl -X POST http://localhost:3000/jsonrpc -H 'content-type: application/json' -d '{
  "jsonrpc":"2.0","id":1,"method":"message:send",
  "params":{"message":{"role":"ROLE_USER","messageId":"m1",
    "parts":[{"text":"Summarize the README"}]}}
}'
```

## Client — call another A2A agent from goose

When the A2A client is wired as a tool (`GOOSE_A2A_CLIENT_ENABLE`), goose can
delegate to a remote agent. It resolves the remote Agent Card, lets the crate's
client factory **negotiate the best transport** — JSON-RPC and REST by default,
plus WebSocket which goose registers explicitly — sends the message, and returns
the reply text (from the result `Message`, the `Task.status.message`, or the last
agent turn in `history`, in that order).

## Notes & limitations

- **Binding:** A2A **v1.0** (uppercase `ROLE_*` / `TASK_STATE_*` enums,
  field-presence `Part`s, `method:verb` names) via the fork crate.
- **Parts:** text and raw image parts map cleanly to goose message content; other
  raw/url/data parts are flattened to text (goose has no native binary/file/data
  message content).
- **Fork upgrades:** improvements needed for goose integration land in
  88plug/a2a-rs first, then goose-plus bumps the pinned rev.
