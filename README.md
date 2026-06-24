> **🦆 goose-plus** — the community **[88plug/goose-plus](https://github.com/88plug/goose-plus)** fork of [aaif-goose/goose](https://github.com/aaif-goose/goose): every dependency on its latest version, first-class **A2A (Agent2Agent)** support (JSON-RPC + REST + WebSocket), native **NATS** event publishing, and a self-maintaining CI/CD + supply-chain pipeline. The install instructions below pull from goose-plus releases. See **[GOOSE_PLUS.md](GOOSE_PLUS.md)** for what's added.

<div align="center">

# goose

_your native open source AI agent — desktop app, CLI, and API — for code, workflows, and everything in between_

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0"
    ><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg"></a>
  <a href="https://discord.gg/goose-oss"
    ><img src="https://img.shields.io/discord/1287729918100246654?logo=discord&logoColor=white&label=Join+Us&color=blueviolet" alt="Discord"></a>
  <a href="https://github.com/aaif-goose/goose/actions/workflows/ci.yml"
     ><img src="https://img.shields.io/github/actions/workflow/status/aaif-goose/goose/ci.yml?branch=main" alt="CI"></a>
  <a href="https://insights.linuxfoundation.org/project/goose"><img src="https://insights.linuxfoundation.org/api/badge/health-score?project=goose"></a>
  <a href="https://repology.org/project/goose-cli/versions"><img src="https://repology.org/badge/tiny-repos/goose-cli.svg" alt="Packaging status"></a>
</p>
</div>

goose is a general-purpose AI agent that runs on your machine. Not just for code — use it for research, writing, automation, data analysis, or anything you need to get done.

A native desktop app for macOS, Linux, and Windows. A full CLI for terminal workflows. An API to embed it anywhere. Built in Rust for performance and portability.

goose works with 15+ providers — Anthropic, OpenAI, Google, Ollama, OpenRouter, Azure, Bedrock, and more. Use API keys or your existing Claude, ChatGPT, or Gemini subscriptions via [ACP](https://goose-docs.ai/docs/guides/acp-providers). Connect to 70+ extensions via the [Model Context Protocol](https://modelcontextprotocol.io/) open standard.

goose is part of the [Agentic AI Foundation (AAIF)](https://aaif.io/) at the Linux Foundation.

# Get started

**[Download the goose-plus desktop app](https://github.com/88plug/goose-plus/releases/latest)** for macOS, Linux, and Windows.

> goose-plus desktop bundles are **unsigned** (community fork, no signing certs): on macOS right-click → **Open** to pass Gatekeeper; on Windows choose **More info → Run anyway** at the SmartScreen prompt.

Or install the CLI from goose-plus:

```bash
curl -fsSL https://github.com/88plug/goose-plus/releases/download/stable/download_cli.sh | bash
```

**Portable builds (no installer):** every release also ships a portable Windows
`.zip` and a portable Linux `.AppImage` on the [releases page](https://github.com/88plug/goose-plus/releases/latest) —
unzip / `chmod +x` and run, nothing to install.

**Run goose in the browser (no install) via Docker:** `docker compose up` starts
the `goosed` backend **and** a web build of the desktop UI — the same renderer,
served as a web app through a browser shim for the Electron APIs:

```bash
echo "GOOSE_SERVER__SECRET_KEY=$(openssl rand -hex 32)" > .env
echo "ANTHROPIC_API_KEY=sk-ant-..." >> .env        # or your provider's key
docker compose up --build
#  → API:  http://localhost:3000   (goosed)
#  → UI:   http://localhost:8080   (open in your browser)
```

You can also drive `goosed` directly from scripts/tooling over its HTTP API, or
build just the web bundle with `cd ui/desktop && pnpm build:web` (output in
`dist-web/`, servable by any static file server pointed at a goosed backend).

> Intended for local/trusted use: the desktop bundles are unsigned and the Docker
> stack binds to localhost. Put authentication in front of `goosed` before
> exposing it on a network.

See [`Dockerfile.server`](Dockerfile.server), [`Dockerfile.web`](Dockerfile.web),
and [`docker-compose.yml`](docker-compose.yml).

# Quick links
- [Quickstart](https://goose-docs.ai/docs/quickstart)
- [Installation](https://goose-docs.ai/docs/getting-started/installation)
- [Tutorials](https://goose-docs.ai/docs/category/tutorials)
- [Documentation](https://goose-docs.ai/docs/category/getting-started)
- [Governance](https://github.com/aaif-goose/goose/blob/main/GOVERNANCE.md)
- [Custom Distributions](https://github.com/aaif-goose/goose/blob/main/CUSTOM_DISTROS.md) — build your own goose distro with preconfigured providers, extensions, and branding

## Need help?
- [Diagnostics & Reporting](https://goose-docs.ai/docs/troubleshooting/diagnostics-and-reporting)
- [Known Issues](https://goose-docs.ai/docs/troubleshooting/known-issues)

# a little goose humor 🪿

> Why did the developer choose goose as their AI agent?
> 
> Because it always helps them "migrate" their code to production! 🚀

# goose around with us
- [Discord](https://discord.gg/goose-oss)
- [YouTube](https://www.youtube.com/@goose-oss)
- [LinkedIn](https://www.linkedin.com/company/goose-oss)
- [Twitter/X](https://x.com/goose_oss)
