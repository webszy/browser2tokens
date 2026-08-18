# Browser2Tokens Agent Guide

> Project: **Browser2Tokens (B2T)**
>
> Slogan: **Browser AI in. Local tokens out.**
>
> This file defines the engineering rules for coding agents working in this repository.

---

## 1. Mission

Browser2Tokens is a **Rust-first local Browser AI Runtime**.

Its purpose is to turn AI capabilities that a user can already access in a browser into **local, programmable protocol interfaces** for agents, CLI tools, IDEs, apps, and other local software.

B2T is not primarily a ChatGPT scraper, a browser automation demo, an account pool, or a public proxy.

The long-term architecture is:

```text
Client
  ↓
Protocol Plugin
  ↓
B2T Kernel
  ↓
Provider Plugin
  ↓
Transport Plugin
  ↓
Browser AI
```

Core principle:

> **Core does not own capabilities. Core orchestrates capabilities.**

Architectural direction:

> **Everything is Plugin.**

Implementation constraint:

> **Do not over-engineer plugin infrastructure before a second implementation proves the abstraction is needed.**

---

## 2. Current v0 Scope

The current v0 scope is intentionally narrow.

### Supported

- Rust-first
- Single user
- Local only
- ChatGPT Web only
- Managed Chrome
- Dedicated persistent Chrome profile
- CDP first
- OpenAI-compatible protocol
- Responses API
- Responses SSE
- Chat Completions API
- Responses WebSocket
- Text input/output first
- In-memory state where persistence is not yet necessary

### Not v0

Do not add these unless the task explicitly requires them:

- Claude
- Grok
- Gemini
- Multiple users
- Account pools
- Remote workers
- Multi-machine scheduling
- Public SaaS hosting
- PostgreSQL
- Admin UI
- Chrome extension
- Playwright sidecar
- Node.js runtime dependency
- Dynamic `.so` plugin loading
- WASM plugin runtime
- Plugin marketplace
- Hot reload
- Complex dependency injection framework
- Distributed tracing infrastructure
- Premature persistence

When requirements are ambiguous, prefer the smaller v0 interpretation.

---

## 3. Technology Baseline

Primary stack:

```text
Language         Rust stable
Async runtime    Tokio
HTTP             Axum
Serialization    Serde / serde_json
CLI              clap
Logging          tracing / tracing-subscriber
Errors           thiserror + anyhow
IDs              uuid / typed newtypes where valuable
Browser          Chrome DevTools Protocol
```

The project should remain **Rust-first, not Rust-only**.

Use a non-Rust component only when the target domain makes it materially more correct or maintainable. Do not introduce another runtime merely for convenience.

---

## 4. Repository Strategy

### v0 stays a single crate

Do not split the repository into a Cargo workspace merely because the architecture contains plugin concepts.

Current logical module boundaries should remain visible inside one crate:

```text
src/
├── main.rs
├── kernel/
├── protocol/
├── provider/
├── transport/
├── runtime/
├── session/
├── config/
└── error/
```

Possible future components:

```text
protocol-openai
provider-chatgpt
transport-cdp
```

These are **architectural plugin boundaries**, not necessarily separate crates yet.

Split crates only when there is concrete pressure such as:

- independent reuse,
- independent testing/public API,
- dependency isolation,
- compilation boundary value,
- multiple implementations requiring a stable interface.

Do not create abstractions solely to make the directory tree look sophisticated.

---

## 5. Required Agent Workflow

Before modifying code:

1. Read the relevant modules and current `Cargo.toml`.
2. Identify the smallest execution path affected.
3. Check existing types and abstractions before creating new ones.
4. State or internally establish the invariant being changed.
5. Make the smallest coherent change.
6. Run formatting, static checks, and relevant tests.
7. Inspect warnings rather than suppressing them automatically.
8. Report behavior changes, trade-offs, and remaining risk.

For small low-risk changes, do not create unnecessary planning ceremony.

For changes affecting protocol semantics, session state, concurrency, plugin interfaces, browser lifecycle, or public APIs, inspect the full call path before editing.

Never assume a design document is more current than the code. **Code is evidence; architecture text is intent. Reconcile both.**

---

## 6. Rust Style

Follow the standard Rust style produced by `rustfmt`.

Required:

```bash
cargo fmt --check
```

Do not hand-format against `rustfmt`.

Naming follows normal Rust conventions:

- `snake_case` for functions, methods, modules, variables.
- `CamelCase` for structs, enums, traits.
- `SCREAMING_SNAKE_CASE` for constants/statics.
- Constructors normally use `new`, or domain verbs such as `connect`, `bind`, `open`, `from_*`.
- Do not prefix ordinary getters with `get_` unless the semantics justify it.

Prefer idiomatic Rust over patterns mechanically ported from TypeScript, Java, Go, or C++.

---

## 7. Clippy Is Part of Correctness

Required before task completion:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

If `--all-features` is inappropriate for the current repository state, explain why and run the strongest applicable command.

Do not blindly silence lints.

When a lint is intentionally allowed:

- keep the scope narrow,
- document the reason,
- prefer fixing the design first.

Avoid project-wide `#![allow(...)]` unless there is a strong documented reason.

---

## 8. Error Handling

### Rules

Library/domain layers:

```text
thiserror
```

Application boundaries / orchestration:

```text
anyhow
```

Prefer:

```rust
Result<T, DomainError>
```

inside stable domain boundaries.

Prefer:

```rust
anyhow::Result<T>
```

at CLI/startup orchestration boundaries where heterogeneous errors are being assembled.

### Do not

- use `unwrap()` in normal runtime paths,
- use `expect()` for recoverable external failures,
- panic on network/browser/protocol input,
- erase useful error context,
- convert every error into a string too early.

`unwrap()` may be acceptable in:

- tests,
- compile-time/static invariants,
- code where impossibility is obvious and locally proven.

If using `expect()`, the message should explain the invariant, not repeat the operation.

Bad:

```rust
value.expect("failed")
```

Better:

```rust
value.expect("provider registry must contain the provider selected during capability resolution")
```

At external boundaries, attach useful context:

- operation,
- target,
- request/session id when available,
- protocol/provider/transport identity.

Never log secrets to improve diagnostics.

---

## 9. Type Safety Over Stringly-Typed Design

Use types to encode stable domain meaning.

Good candidates for newtypes:

```text
RequestId
SessionId
ResponseId
ConversationId
PluginId
CapabilityId
```

Prefer:

```rust
struct RequestId(Uuid);
```

over passing unrelated `String` values everywhere.

Use enums for closed semantic sets.

Avoid boolean-heavy function signatures.

Bad:

```rust
send(true, false, true)
```

Prefer named types/options/builders when a call becomes ambiguous.

Validate data at the boundary where untrusted or external data becomes a domain type.

---

## 10. Async Rust Rules

B2T is an async runtime. Async correctness is a first-class design requirement.

### Never block Tokio worker threads

Do not perform blocking filesystem/process/CPU-heavy work directly inside async tasks when it can materially block the runtime.

Use:

- Tokio async APIs where appropriate,
- `spawn_blocking` for unavoidable blocking work,
- dedicated processes/tasks for long blocking operations.

### Never hold a lock across `.await` unless explicitly justified

Bad pattern:

```rust
let mut state = shared.lock().await;
network_call().await;
state.update();
```

Prefer extracting the minimum state, releasing the lock, awaiting, then reacquiring if necessary.

Favor ownership/message passing over large shared mutable state.

### Prefer bounded concurrency

Avoid unbounded:

- task creation,
- channels,
- request queues,
- stream buffering.

A local single-user runtime can still OOM or deadlock.

### Cancellation is normal

Client disconnects, WebSocket closes, Chrome exits, tab reloads, and requests are cancelled.

Code should treat cancellation as an expected lifecycle event, not an exceptional impossible case.

Long-lived operations should have explicit ownership and shutdown behavior.

### Timeouts belong at external boundaries

Potentially hanging operations should eventually have explicit timeout policy, especially:

- CDP connection,
- page/target discovery,
- browser command execution,
- provider response start,
- streaming idle timeout,
- shutdown.

Do not scatter arbitrary timeout literals. Centralize policy once real values are known.

---

## 11. State and Synchronization

Keep global mutable state minimal.

Prefer:

```text
immutable config
owned task state
typed registries
message passing
small synchronized maps
```

over a giant `Arc<Mutex<AppState>>`.

If shared maps are required, define:

- owner,
- mutation points,
- cleanup lifecycle,
- concurrency expectations.

Every request-scoped resource must have an obvious terminal state.

Examples:

```text
pending → streaming → completed
pending → cancelled
pending → failed
```

Do not leave request/session entries indefinitely after completion.

---

## 12. Internal Protocol Is the Stable Center

Provider code must not directly consume OpenAI wire types.

Transport code must not directly produce OpenAI response objects.

The intended direction is:

```text
OpenAI wire format
      ↓
protocol-openai
      ↓
B2T internal request/event model
      ↓
provider-chatgpt
      ↓
browser capability
      ↓
transport-cdp
```

And on output:

```text
transport-cdp
      ↓
provider-chatgpt
      ↓
B2T internal events
      ↓
protocol-openai
      ↓
OpenAI wire format
```

The internal model should be expressive enough for event streams.

Do not model the provider result as only:

```rust
String
```

The direction is event-oriented, for example:

```text
ResponseStarted
OutputItemAdded
TextDelta
ToolCall
ResponseCompleted
Error
```

Do not add event variants speculatively unless an actual protocol/provider requirement needs them.

---

## 13. OpenAI Protocol Plugin

`protocol-openai` is not merely a Chat Completions endpoint.

Its intended surface includes:

```text
GET  /v1/models
POST /v1/responses
POST /v1/chat/completions
WS   /v1/responses
```

Streaming includes:

- Responses SSE
- Chat Completions SSE
- Responses WebSocket events

The OpenAI protocol layer owns:

- request schema parsing,
- validation,
- OpenAI ↔ B2T normalization,
- OpenAI event formatting,
- HTTP/SSE/WebSocket semantics.

It does **not** own ChatGPT browser behavior.

If the underlying provider cannot implement an OpenAI capability faithfully, return an explicit unsupported-capability/protocol error.

Never silently discard an unsupported field if doing so changes semantics.

---

## 14. Plugin Architecture

Architectural principle:

> Plugins depend on capabilities, not concrete plugins.

Bad:

```text
provider-chatgpt → transport-cdp implementation type
```

Desired:

```text
provider-chatgpt
    requires browser.page / browser.network / browser.runtime
        ↓
kernel capability resolution
        ↓
transport-cdp
    provides those capabilities
```

### v0 implementation rule

Do not build a full dynamic plugin runtime yet.

A Rust trait/module registered statically at startup is acceptable.

The architecture should allow eventual pluginization without paying its full complexity today.

### Kernel responsibility

Kernel may own:

- plugin registration,
- capability registration,
- capability resolution,
- lifecycle,
- dispatch,
- request context,
- event routing.

Kernel should not contain:

```text
ChatGPT URL selectors
OpenAI JSON field knowledge
raw CDP command logic
provider-specific conversation parsing
```

---

## 15. Provider Rules

`provider-chatgpt` owns ChatGPT Web semantics.

It may know:

- ChatGPT page behavior,
- conversation identifiers,
- request/response semantics,
- model mapping,
- stream interpretation,
- provider-specific errors.

It must not own:

- Axum routing,
- OpenAI response JSON,
- Chrome process startup,
- generic CDP lifecycle,
- persistence policy unrelated to provider semantics.

Provider behavior should be isolated so ChatGPT website changes do not require kernel changes.

---

## 16. Transport Rules

`transport-cdp` owns browser transport mechanics.

It may know:

- Chrome process lifecycle,
- CDP connection,
- target discovery,
- Runtime domain,
- Network domain,
- page evaluation,
- navigation,
- browser events.

It must not know OpenAI API semantics.

It should not contain ChatGPT-specific protocol mapping except where an unavoidable browser primitive requires a selector supplied by the provider.

Expose semantic browser capabilities rather than leaking the entire CDP client through every layer.

Do not over-wrap CDP prematurely: wrap what B2T actually uses.

---

## 17. Managed Chrome and Authentication Boundary

v0 uses a B2T-managed, visible Chrome instance with a dedicated persistent profile.

Conceptual profile location:

```text
~/.b2t/chrome-profile
```

Rules:

- Do not use or mutate the user's default Chrome profile.
- Do not ask B2T to store usernames/passwords.
- The user performs interactive login in Chrome.
- CAPTCHA/MFA/login confirmation remains in the browser.
- Do not copy authentication tokens/cookies out of the browser unless a specific approved design absolutely requires it.
- Never log cookies, authorization headers, session tokens, CSRF tokens, or credential material.
- Prefer executing authenticated browser work within the browser/page context.

Principle:

> **Browser owns authentication. B2T owns orchestration.**

Chrome/profile cleanup code must never accidentally delete an unrelated user directory.

Any code deleting profile paths requires strong path validation.

---

## 18. Process Management

Managed Chrome is a child/external process with lifecycle semantics.

Track:

- executable path,
- profile path,
- debugging endpoint,
- process identity,
- startup state,
- shutdown state.

Handle:

- Chrome already running,
- port unavailable,
- startup timeout,
- child exits unexpectedly,
- B2T shutdown,
- stale profile lock,
- no ChatGPT target,
- page reload/navigation.

Avoid assuming PID existence implies CDP readiness.

Readiness should be proven through the protocol endpoint/connection.

---

## 19. Session Model

Maintain the distinction:

```text
Request ≠ B2T Session ≠ Provider Conversation
```

A request is one operation.

A B2T session is continuity owned by B2T.

A provider conversation is continuity owned by ChatGPT Web.

Do not conflate an HTTP connection with a session.

The eventual mapping may resemble:

```text
B2T Session
    ↕
OpenAI response/session identifiers
    ↕
ChatGPT conversation identifier
```

v0 may keep this mapping in memory.

Do not add SQLite merely to persist data that has not demonstrated persistence requirements.

---

## 20. Logging and Observability

Use `tracing`.

Prefer structured fields:

```rust
tracing::info!(
    request_id = %request_id,
    provider = "chatgpt",
    "request started"
);
```

over concatenated log strings.

Useful correlation dimensions:

```text
request_id
session_id
plugin_id
provider
transport
target_id
```

when they exist.

Do not log full prompts/responses by default.

Never log authentication material.

Log lifecycle transitions and failures at the layer that has enough context to explain them.

Avoid duplicate error logs at every propagation layer.

---

## 21. Configuration

Configuration should start small.

Initial expected values include:

```text
host
port
Chrome executable/path if needed
Chrome profile path
CDP settings
```

Prefer typed config with defaults.

Separate:

- defaults,
- user-supplied configuration,
- runtime-discovered values.

Do not add a complex configuration framework before needed.

Do not hardcode user-specific absolute paths.

Platform-specific defaults should live behind a clear abstraction/helper.

---

## 22. Dependencies

Every dependency has maintenance, compilation, security, and API costs.

Before adding a crate:

1. Check whether `std`, Tokio, Axum, or an existing dependency already solves the problem.
2. Prefer well-maintained, focused crates.
3. Avoid overlapping crates providing the same responsibility.
4. Avoid adding a heavyweight framework for one helper.
5. Keep feature flags narrow where practical.
6. Explain dependencies that introduce a runtime/process/platform commitment.

Do not manually pin exact versions without a reason.

Do not casually modify `Cargo.lock` beyond dependency changes produced by Cargo.

For this application repository, commit `Cargo.lock`.

---

## 23. Public API and Traits

Follow predictable Rust APIs.

Prefer:

- domain nouns for types,
- explicit ownership,
- borrowing where it naturally reduces copies,
- constructors with clear invariants,
- small cohesive traits,
- minimal generic complexity.

Avoid speculative generic frameworks.

Bad tendency:

```rust
trait Provider<TTransport, TRequest, TEvent, TSession, TConfig, ...>
```

Prefer a concrete internal protocol and small behavior-oriented traits until multiple implementations justify greater generality.

Trait objects are acceptable when runtime polymorphism is the actual requirement.

Generics are appropriate when compile-time polymorphism materially improves correctness/performance without harming readability.

---

## 24. Documentation

Document:

- invariants,
- non-obvious lifecycle semantics,
- public interfaces,
- safety assumptions,
- protocol mapping decisions,
- intentional deviations from standards.

Do not write comments that merely restate code.

Good comment:

```text
We release the session lock before awaiting CDP because a response event may
need the same session entry to complete the request.
```

Bad comment:

```text
// Acquire lock
```

When a public item becomes a stable reusable API, add Rustdoc examples where useful.

---

## 25. Unsafe Code

Default policy:

> **Do not introduce `unsafe` code.**

If `unsafe` becomes genuinely necessary:

- isolate it in the smallest module,
- document the safety invariant,
- explain why safe alternatives are insufficient,
- add focused tests,
- request explicit review.

Never use `unsafe` merely to bypass ownership design problems.

---

## 26. Testing Strategy

Tests should focus on stable behavior and boundaries.

### Unit tests

Good targets:

- internal protocol conversions,
- capability matching,
- state transitions,
- error mapping,
- OpenAI schema normalization,
- provider stream parsing using fixtures.

### Integration tests

Good targets:

- Axum endpoints,
- SSE formatting,
- WebSocket protocol behavior,
- kernel → provider dispatch with fake plugins/transports.

### Browser/CDP tests

Keep real-browser tests separate from ordinary unit tests.

They are slower and environment-sensitive.

Where practical, define fake/test transports so most provider behavior can be tested without launching Chrome.

Do not mock implementation details so heavily that tests merely reproduce the code.

---

## 27. Required Verification

Before reporting a coding task complete, run the applicable subset, preferably all:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

When changing HTTP behavior, also perform a minimal runtime smoke test.

Example:

```bash
cargo run -- start
curl http://127.0.0.1:8787/health
```

For CDP/browser changes, verify the concrete browser path affected.

Never claim a command passed if it was not run.

If a check cannot be run, say exactly why.

---

## 28. Performance Philosophy

Do not prematurely optimize B2T.

Prioritize:

1. correctness,
2. lifecycle clarity,
3. cancellation,
4. bounded resource use,
5. protocol correctness,
6. observability,
7. performance.

However, avoid obviously expensive architecture such as:

- cloning entire conversations repeatedly without need,
- unbounded event accumulation,
- unbounded channels,
- holding locks while streaming,
- spawning a task per token if avoidable.

Measure before optimizing CPU or allocation details.

---

## 29. Security and Privacy

B2T lives close to authenticated browser sessions, so treat browser state as sensitive.

Never:

- print cookies/tokens,
- commit browser profiles,
- commit secrets,
- expose the local API publicly by default,
- bind to `0.0.0.0` by default,
- weaken Chrome security flags casually,
- disable TLS/certificate validation for external traffic without an explicit reason,
- execute arbitrary page-provided code outside the browser boundary.

Default server binding:

```text
127.0.0.1
```

Any future remote-access feature must be designed as a separate security boundary.

---

## 30. Protocol Fidelity

Compatibility means behavior, not just matching endpoint names.

When implementing OpenAI-compatible APIs:

- validate required fields,
- preserve event ordering,
- preserve streaming termination semantics,
- use appropriate HTTP/WebSocket status/error behavior,
- distinguish unsupported capability from provider failure,
- do not fabricate metadata that B2T cannot know reliably.

If exact compatibility is impossible through ChatGPT Web, document the divergence explicitly.

---

## 31. Backpressure and Streaming

Streaming must not assume the consumer is infinitely fast.

Design streams so that:

- buffering is bounded,
- cancellation propagates,
- producer shutdown is observable,
- client disconnect stops unnecessary browser work when possible.

Do not collect a full provider response in memory merely to emulate streaming afterward unless the current provider makes true streaming impossible and the limitation is documented.

---

## 32. Browser Research / Reverse Engineering Discipline

ChatGPT Web behavior is an unstable external boundary.

When investigating it:

1. Observe before abstracting.
2. Keep experimental code isolated.
3. Record what was observed versus what is inferred.
4. Do not spread raw endpoint/DOM assumptions throughout the repository.
5. Move validated behavior behind `provider-chatgpt`.
6. Add fixtures/tests for parsers when possible.
7. Expect website changes.

A browser experiment is not yet an architecture.

Do not promote hacks into kernel APIs merely because they made the first request work.

---

## 33. Change Discipline

Prefer small vertical increments.

Recommended progression:

```text
health endpoint
→ managed Chrome
→ CDP connect
→ target discovery
→ Runtime.evaluate
→ one ChatGPT request
→ one streamed response
→ provider abstraction
→ Responses HTTP/SSE
→ Chat Completions
→ session continuity
→ Responses WebSocket
→ stronger plugin framework
```

Avoid horizontal framework building before a working vertical slice exists.

Key project rule:

> **First prove B2T can obtain one real response through the browser. Then prove Everything is Plugin.**

---

## 34. Architectural Decision Rules

When choosing between two designs, prefer the one that:

1. preserves protocol/provider/transport separation,
2. keeps authentication inside the browser,
3. reduces kernel knowledge of external platforms,
4. uses typed state instead of string conventions,
5. has explicit lifecycle/cancellation behavior,
6. adds less infrastructure,
7. can be replaced when the external website changes.

Do not optimize primarily for theoretical extensibility.

Optimize for **replaceable boundaries and a working v0**.

---

## 35. Dependency Direction

Desired high-level dependency direction:

```text
main/runtime
    ↓
kernel
    ↓
internal domain model

protocol-openai ─┐
provider-chatgpt ├─ interact through kernel/domain contracts
transport-cdp  ──┘
```

Do not let:

```text
kernel → provider-chatgpt implementation
kernel → OpenAI-specific wire types
kernel → CDP concrete client types
provider-chatgpt → Axum handlers
transport-cdp → OpenAI models
```

become permanent dependencies.

Temporary spikes may break clean layering, but must be isolated and cleaned once the experiment is validated.

---

## 36. Definition of Done

A change is done when:

- behavior is implemented,
- architecture boundaries are preserved or intentionally changed,
- errors are meaningful,
- no secret data is logged,
- formatting passes,
- checks/tests pass,
- warnings are resolved,
- relevant runtime behavior is smoke-tested,
- unnecessary speculative code is not included.

Do not mark a task complete merely because the happy path compiles.

---

## 37. Sources of Engineering Truth

When Rust conventions are unclear, prioritize:

1. Rust language/reference documentation.
2. The Rust Style Guide / `rustfmt`.
3. Clippy documentation.
4. The Cargo Book.
5. Rust API Guidelines.
6. Tokio/Axum official documentation for async/runtime behavior.
7. Existing project code and established local conventions.

Prefer primary documentation over blog posts.

---

## 38. Current Architecture Summary

```text
                     Browser2Tokens

                  ┌───────────────┐
                  │   B2T Kernel  │
                  │ Rust / Tokio  │
                  └───────┬───────┘
                          │
                 Capability Registry
                          │
         ┌────────────────┼────────────────┐
         │                │                │
         ▼                ▼                ▼
 protocol-openai   provider-chatgpt   transport-cdp
         │                │                │
         │                │                ▼
         │                │         Managed Chrome
         │                │                │
         │                └───────────────►│
         │                                 ▼
         │                            chatgpt.com
         │
         ▼
OpenAI-compatible local API
Responses / Chat Completions / SSE / WebSocket
```

Remember:

> **Rust First, not Rust Only.**

> **Everything is Plugin.**

> **Core does not own capabilities. Core orchestrates capabilities.**

> **Plugins depend on capabilities, not concrete plugins.**

> **Browser owns authentication. B2T owns orchestration.**

> **Make the architecture visible, but keep the implementation minimal.**
