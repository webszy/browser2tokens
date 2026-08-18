# Browser2Tokens

**Browser AI in. Local tokens out.**

Browser2Tokens, or **B2T**, is a Rust-first local runtime that turns AI capabilities already available in your browser into programmable local APIs.

Instead of treating ChatGPT, Claude, Grok, or other browser-based AI products as isolated web UIs, B2T aims to expose them through standard interfaces that local agents, IDEs, CLI tools, applications, and automation systems can consume.

> Browser2Tokens is not a browser scraper, an account pool, or a public proxy.
> It is a compatibility runtime for Browser AI.

---

## Status

Browser2Tokens is currently in **early v0 development**.

The initial implementation is intentionally narrow:

* Single user
* Local only
* ChatGPT Web only
* Managed Chrome
* Dedicated persistent Chrome profile
* Chrome DevTools Protocol first
* Rust-first runtime
* OpenAI-compatible APIs

The first milestone is simple:

```text
Local Client
    ↓
Browser2Tokens
    ↓
Managed Chrome
    ↓
ChatGPT Web
    ↓
Streaming response
```

---

## Why Browser2Tokens?

Browser-based AI products often provide capabilities that are only directly accessible through their web applications.

At the same time, local tools increasingly expect programmable interfaces such as:

* OpenAI-compatible APIs
* Responses API
* Chat Completions
* SSE streams
* WebSocket sessions
* Agent runtime protocols

Browser2Tokens sits between these two worlds.

```text
Browser AI
    ↓
Browser2Tokens
    ↓
Local programmable interface
```

The browser remains responsible for authentication and interactive login.

B2T is responsible for orchestration, protocol adaptation, sessions, routing, and streaming.

> **Browser owns authentication. B2T owns orchestration.**

---

# Architecture

Browser2Tokens follows a plugin-oriented architecture inspired by modern AI runtimes and harness systems.

Its central design principle is:

> **Core does not own capabilities. Core orchestrates capabilities.**

And the long-term architectural direction is:

> **Everything is Plugin.**

The runtime is organized around three primary plugin categories:

```text
                     Browser2Tokens

                  ┌────────────────┐
                  │   B2T Kernel   │
                  │  Rust / Tokio  │
                  └───────┬────────┘
                          │
                 Capability Registry
                          │
          ┌───────────────┼───────────────┐
          │               │               │
          ▼               ▼               ▼
     Protocol Plugin  Provider Plugin  Transport Plugin
```

For v0:

```text
protocol-openai
provider-chatgpt
transport-cdp
```

The complete request path is expected to look like:

```text
OpenAI Client
     ↓
protocol-openai
     ↓
B2T Internal Protocol
     ↓
B2T Kernel
     ↓
provider-chatgpt
     ↓
browser capabilities
     ↓
transport-cdp
     ↓
Managed Chrome
     ↓
chatgpt.com
```

---

## The Kernel

The B2T Kernel should remain intentionally small.

Its responsibilities include:

* Plugin lifecycle
* Plugin registry
* Capability registry
* Capability resolution
* Request dispatch
* Event routing
* Runtime lifecycle
* Session coordination

The Kernel should not contain platform-specific implementation details.

It should not know:

* How ChatGPT Web works
* How Claude Web works
* OpenAI wire-format details
* Raw CDP commands
* Website-specific selectors
* Provider-specific conversation parsing

---

## Protocol Plugins

Protocol plugins expose B2T capabilities to external clients.

The first protocol implementation is:

```text
protocol-openai
```

It is intended to support the OpenAI protocol family rather than a single endpoint.

Planned interfaces include:

```text
GET  /v1/models

POST /v1/responses

POST /v1/chat/completions

WS   /v1/responses
```

Streaming support includes:

* Responses SSE
* Chat Completions SSE
* Responses WebSocket

Future protocol plugins may include:

```text
protocol-anthropic
protocol-mcp
protocol-custom
```

Protocol plugins translate external wire formats into the B2T internal request and event model.

They do not implement browser-provider behavior.

---

## Provider Plugins

Provider plugins understand a specific Browser AI platform.

The first provider is:

```text
provider-chatgpt
```

It owns ChatGPT-specific behavior such as:

* Web conversation semantics
* Model mapping
* ChatGPT conversation identifiers
* Provider request construction
* Stream interpretation
* Provider-specific error handling

Future providers may include:

```text
provider-claude
provider-grok
provider-gemini
```

Adding support for a new platform should primarily mean adding a new provider plugin rather than modifying the B2T Kernel.

---

## Transport Plugins

Transport plugins provide browser access capabilities.

The first transport is:

```text
transport-cdp
```

It uses the Chrome DevTools Protocol to interact with a real Chrome instance.

Possible capabilities include:

```text
browser.targets
browser.page
browser.runtime
browser.network
browser.navigation
```

Future transports may include:

```text
transport-extension
transport-webview
transport-remote-browser
```

Provider plugins depend on capabilities rather than concrete transport implementations.

For example:

```text
provider-chatgpt

requires:
  browser.page
  browser.runtime
  browser.network
```

while:

```text
transport-cdp

provides:
  browser.page
  browser.runtime
  browser.network
```

This keeps providers independent from a specific browser-control implementation.

---

# Managed Chrome

Browser2Tokens v0 uses a dedicated Chrome instance managed by B2T.

The browser uses a persistent profile such as:

```text
~/.b2t/chrome-profile
```

The user logs into ChatGPT normally inside that browser.

B2T should not need to:

* Store ChatGPT usernames or passwords
* Import browser cookies manually
* Copy authentication tokens into configuration
* Automate CAPTCHA or MFA
* Modify the user's default Chrome profile

A typical lifecycle is:

```text
b2t start
    ↓
Launch Managed Chrome
    ↓
Load persistent B2T profile
    ↓
User signs in if required
    ↓
B2T connects through CDP
    ↓
Locate chatgpt.com target
    ↓
Provider becomes ready
```

---

# Internal Protocol

B2T uses an internal protocol between external protocol adapters and Browser AI providers.

This prevents provider implementations from depending directly on OpenAI-specific schemas.

Conceptually:

```text
OpenAI Responses
OpenAI Chat Completions
        │
        ▼
 protocol-openai
        │
        ▼
  B2TRequest
        │
        ▼
 provider-chatgpt
```

Responses are event-oriented.

Example internal events may include:

```text
ResponseStarted
OutputItemAdded
TextDelta
ToolCall
ResponseCompleted
Error
```

This event model is important because Browser2Tokens is designed for:

* SSE
* WebSocket sessions
* Long-running agent workflows
* Cancellation
* Streaming
* Tool-oriented interaction

---

# Sessions

Browser2Tokens keeps three concepts separate:

```text
Request
   ≠
B2T Session
   ≠
Provider Conversation
```

A request is one operation.

A B2T session represents local continuity.

A provider conversation represents continuity inside the Browser AI platform.

Eventually the mapping may look like:

```text
OpenAI response/session identifier
              ↕
         B2T Session
              ↕
   ChatGPT conversation identifier
```

v0 may initially keep session state in memory.

Persistent storage should only be introduced when persistence requirements become concrete.

---

# Technology

Browser2Tokens is **Rust-first, not Rust-only**.

Current runtime stack:

```text
Rust
Tokio
Axum
Serde
serde_json
clap
tracing
thiserror
anyhow
uuid
```

Browser access:

```text
Chrome DevTools Protocol
```

The project intentionally avoids introducing additional runtimes unless they provide clear technical value.

---

# Development

## Requirements

You will need:

* Rust stable
* Cargo
* Google Chrome
* macOS, Linux, or another supported development environment

Check your Rust installation:

```bash
rustc --version
cargo --version
rustup --version
```

Update Rust if needed:

```bash
rustup update stable
rustup default stable
```

---

## Run

During early development:

```bash
cargo run -- start
```

The default local server is expected to listen on:

```text
127.0.0.1:8787
```

Health check:

```bash
curl http://127.0.0.1:8787/health
```

Expected response:

```json
{
  "status": "ok"
}
```

---

## Development Checks

Before considering a change complete:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

See `AGENTS.md` for detailed engineering and agent-development rules.

---

# v0 Development Plan

The project is being developed vertically rather than by building the entire plugin framework upfront.

Current progression:

```text
1. Rust project foundation
        ↓
2. Local HTTP runtime
        ↓
3. Managed Chrome startup
        ↓
4. CDP connection
        ↓
5. Target discovery
        ↓
6. Runtime.evaluate
        ↓
7. First ChatGPT Web request
        ↓
8. First streamed response
        ↓
9. provider-chatgpt
        ↓
10. Responses HTTP / SSE
        ↓
11. Chat Completions
        ↓
12. Session continuity
        ↓
13. Responses WebSocket
        ↓
14. Stronger plugin infrastructure
```

Project rule:

> **First prove B2T can obtain one real response through the browser. Then prove Everything is Plugin.**

---

# Non-Goals for v0

Browser2Tokens v0 is not attempting to build:

* A multi-user gateway
* A subscription resale platform
* An AI account pool
* A distributed worker system
* A hosted proxy
* A plugin marketplace
* A full browser automation framework
* A replacement browser
* A GUI management platform

These may or may not become relevant later.

The current priority is a small, reliable local runtime.

---

# Future Direction

The long-term model is:

```text
                    B2T Kernel
                        │
                Capability Registry
                        │
     ┌──────────────────┼──────────────────┐
     │                  │                  │
     ▼                  ▼                  ▼
 Protocol Plugins   Provider Plugins   Transport Plugins
     │                  │                  │
 OpenAI             ChatGPT              CDP
 Anthropic          Claude               Extension
 MCP                Grok                 Remote Browser
 Custom             Gemini               WebView
```

Additional middleware-style capabilities could eventually include:

```text
routing
retry
fallback
usage
logging
rate limiting
cache
session persistence
observability
```

But the Kernel should remain small.

---

# Design Principles

Browser2Tokens follows several core rules:

### Rust First, Not Rust Only

The runtime and core infrastructure are Rust-first.

Use the technology that best fits the platform boundary when necessary.

### Everything Is Plugin

Protocols, providers, and transports should evolve independently.

### Core Orchestrates Capabilities

The Kernel should coordinate components rather than contain platform logic.

### Plugins Depend on Capabilities

Plugins should request capabilities instead of directly depending on another concrete plugin.

### Browser Owns Authentication

Authentication stays inside the user's real browser session whenever possible.

### Keep v0 Small

Architecture should remain visible without implementing infrastructure before it is needed.

---

# License

License has not been finalized yet.

---

# Project

**Browser2Tokens**

**Browser AI in. Local tokens out.**

Early v0 development.
