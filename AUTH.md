# AUTH.md — how goose authenticates to agent services

This file documents goose's agent-authentication posture, for the benefit of
agents and operators discovering this repo.

goose is an **agent framework** (CLI + desktop + a localhost control-plane
daemon, `goosed`). It authenticates **outbound** to model providers and MCP
servers; it is not a public website or credential-issuing identity provider, so
the website-oriented agent-readiness checks (robots.txt, sitemaps, DNS-AID,
x402/commerce, WebMCP, etc.) do not apply to it.

## MCP authorization (standards-based, already supported)

When connecting to a protected MCP server, goose acts as a spec-compliant
**OAuth client**:

- **RFC 9728 — OAuth 2.0 Protected Resource Metadata.** goose follows the
  `401 → WWW-Authenticate: Bearer resource_metadata=… → /.well-known/oauth-protected-resource → authorization_servers`
  discovery chain.
- **RFC 8414 / OpenID Connect Discovery — Authorization Server Metadata.** goose
  resolves the authorization server's metadata to obtain the authorization and
  token endpoints.

This is implemented via `rmcp`'s `OAuthState` / `AuthorizationManager`
(`crates/goose/src/oauth/`).

## Provider authentication

Model providers are authenticated per provider — API keys (stored via the OS
keyring / config) and, for some providers, OAuth device-code / loopback flows
(e.g. the xAI SuperGrok provider added in goose-plus).

## A2A

When the A2A server is enabled (`GOOSE_A2A_ENABLE`), authentication for inbound
A2A requests is declared in the Agent Card's `securitySchemes`/`security`
(currently none — intended for trusted/local networks; put it behind your own
gateway for untrusted exposure). See [docs/a2a.md](docs/a2a.md).
