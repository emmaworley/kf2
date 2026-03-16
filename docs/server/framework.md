# Server Framework Selection

## Decision

**Axum** as the web framework.

## Requirements

From the [architecture](../architecture.md), the server must:
- Serve two SPAs (Projector and Remocon) as static file bundles
- Expose REST APIs for session management and queue control
- Push real-time state updates to connected clients (WebSocket)
- Proxy and cache assets from upstream karaoke data providers
- Persist session state and queue data

## Web Framework Comparison

### Axum (Selected)
- Built by the Tokio team — first-party integration with Tokio, Tower, and Hyper
- Macro-free routing via `Router::new().route()`
- Extractor pattern for typed request data (JSON, path params, query strings, shared state, WebSocket upgrades) — compile-time type safety
- Tower middleware ecosystem (CORS, compression, tracing, rate limiting) — any Tower-compatible middleware works out of the box
- First-class WebSocket support via `axum::extract::ws::WebSocketUpgrade` — upgrade in a handler, split into sender/receiver for concurrent I/O
- SPA serving via `tower_http::services::ServeDir` with `.fallback(ServeFile::new("index.html"))` for client-side routing
- Shared state via `State(Arc<AppState>)` extractor
- Streaming responses via `axum::body::Body::from_stream()` for large assets
- Best memory consumption in benchmarks (standby and under load)
- ~17,000–18,000 req/s per thread

### Actix Web (Rejected)
- Highest raw throughput (~19,000–20,000 req/s per thread, 10–15% ahead of Axum)
- Custom HTTP stack (not Hyper) — not directly compatible with Tower middleware without adapters
- WebSocket via actor model (`actix-web-actors`) — more boilerplate than Axum's handler-based approach
- Dual middleware systems (Actix-native and Tower compat) — confusing which to use when
- Most mature ecosystem (since 2017)
- Higher standby memory than Axum
- **Why not**: the actor model adds complexity for no benefit at KF2's scale (handful of concurrent clients). Tower middleware incompatibility means extra adapter work. Performance advantage is irrelevant for a local app.

### Rocket (Rejected)
- Batteries-included: built-in templating, form handling, cookies, TLS, database pooling
- Proc-macro routing (`#[get("/")]`) — best developer experience
- Fairings for middleware (Rocket-specific, not reusable outside the framework)
- Slowest of the three under load; worse memory under stress
- WebSocket support less mature
- **Why not**: less flexible, weaker WebSocket story, performance lags. The batteries-included features (templating, forms) aren't needed since KF2's frontends are separate SPAs.

### Poem (Rejected)
- Minimal and readable, built on Hyper/Tokio
- Unique built-in OpenAPI spec generation
- Lower performance than Axum; smaller community
- **Why not**: smaller ecosystem and community. OpenAPI generation is nice but not a priority.

### Salvo (Rejected)
- Only Rust framework with HTTP/3 (QUIC) support
- Feature-rich but newest and least battle-tested
- **Why not**: HTTP/3 isn't needed for a local karaoke app. Too new to trust for production.

### Warp (Rejected)
- Filter-based route composition — elegant but produces inscrutable type errors
- Declining development momentum; many users have migrated to Axum
- **Why not**: effectively superseded by Axum.

## References

- [Rust Web Frameworks in 2026: Axum vs Actix Web vs Rocket vs Warp vs Salvo](https://aarambhdevhub.medium.com/rust-web-frameworks-in-2026-axum-vs-actix-web-vs-rocket-vs-warp-vs-salvo-which-one-should-you-2db3792c79a2)
- [Rust Web Frameworks Compared: Actix vs Axum vs Rocket](https://dev.to/leapcell/rust-web-frameworks-compared-actix-vs-axum-vs-rocket-4bad)
- [Axum documentation](https://docs.rs/axum)
