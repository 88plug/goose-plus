# goose-plus

A community **plus fork** of [goose](https://github.com/aaif-goose/goose) — the version that finishes the migrations the maintainers had in flight and fixes the user-facing failures their seams caused, plus net-new agent-interop support.

> Built on upstream `main`. Every change is intended to be one a thoughtful maintainer would accept: small, focused, tested, and verified (`cargo clippy -D warnings`, `cargo test`, desktop `lint:check` all green).

## Headline additions

- **xAI SuperGrok native provider** — OAuth (loopback + device-code), live-correct Grok context windows (1M), and **working thinking-effort**: your Off/Low/…/Max selection is actually transmitted as `reasoning_effort` on chat/completions (live-verified to scale), with graceful fallback for models that reject it.
- **A2A (Agent2Agent) support** — goose speaks A2A both ways: serves an Agent Card and a JSON-RPC endpoint (`message/send`, `message/stream`, `tasks/get`, `tasks/cancel`), and can call remote A2A agents as tools. (See `docs/a2a.md`.)
- **Native NATS publishing** — opt-in: set `GOOSE_NATS_URL` and goose publishes session / message / tool-call events to a NATS subject for fleet observability and agent-to-agent buses. (See `docs/nats.md`.)
- **Agent-readiness** — the genuinely relevant items for an agent *framework* (most of the isitagentready.com checklist is website/storefront discoverability and does not apply): the A2A **Agent Card** at `/.well-known/agent-card.json` (covers "A2A Agent Card" + "Agent Skills"); `AUTH.md` documenting goose's standards-based MCP OAuth client (RFC 9728 Protected Resource + RFC 8414/OIDC discovery, already provided via `rmcp`). Deliberately **not** done: MCP Server Card (two competing unratified draft SEPs — would be guessing), and `SKILL.md` consumption (goose already ships it in `crates/goose/src/skills/`).

## Bug fixes & improvements (mined from the issue/PR graveyard)

**Providers / reasoning**
- DeepSeek/openai-compatible reasoning_content no longer dropped (#9397, #9675)
- Streaming: final text segment after tool calls no longer lost (#8503)
- Unmapped `/v1/models` surfaced instead of silently dropped (#8321)
- Env-configurable retry + request timeout (#9124, #7987); sane 429/Retry-After handling
- Ollama keep-alive so the model stays warm (#9489)
- Databricks serving-endpoints pagination — all models surface (#9476)
- Context-limit floor stops Context-Length-Exceeded on fresh install (#8512)
- Codex / `v1/responses` handling (#2564); DeepSeek V4 (#9333)
- Gemini empty-reply-after-tool-use (#6293) and fetch-400 (#1863)
- devstral context limits (#6573); `OPENAI_CUSTOM_HEADERS` with commas (#8495)
- MCP auto-reconnect on dropped transport (#7063)

**Agent / CLI / server**
- Recipe-level tool blocking (denylist) (#7808)
- Graceful unknown-tool calls with suggestions (#8183)
- No-progress loop guard — stops runaway/looping turns (#9082, #9640)
- Unix login-shell process-group isolation (#8777)
- `detect_image_path` handles shell escapes (#9398)
- `GOOSE.md` recognized as a project context file (#3085)
- CLI bracketed paste — pasting no longer auto-executes (#8059)
- `GET /sessions/{id}` message pagination (#9358)

**Desktop**
- Provider parity (xAI SuperGrok icon/metadata; audit of the whole list)
- Folders for organizing chats (#6926); model favorites (#9080)
- Close-to-tray (#9391); per-window pinned certs (#7554); same-window link nav (#9143)
- In-chat find filter (#1505); a11y font scaling (#8288); syntax-highlighted diffs (#9390)
- Delete apps (#7965); configurable sidebar session limit (#8140)
- Chat history not loading (#9342); reply-render delay under reduced-motion (#8997)
- MCP Apps `ui/update-model-context` (#6472)

## Relationship to upstream
This fork tracks upstream `main`; the plus work lives on top. Items already shipped by upstream maintainers (LM Studio, Mistral, Ollama, `/model`, Azure, PreToolUse hooks, Gemini ACP, …) are intentionally **not** re-implemented here.

## Build
```bash
source bin/activate-hermit
cargo build --release          # CLI + server
cd ui/desktop && pnpm install  # desktop
```
