# Frontend Technology Selection

## Bundler: Vite

### Requirements

- Fast development iteration with hot module replacement (HMR)
- TypeScript support out of the box
- Efficient production builds
- Multi-app monorepo support (two SPAs sharing packages)

### Comparison

#### Vite (Selected)

- Native ESM dev server — near-instant cold starts, sub-second HMR
- First-class TypeScript support without configuration
- Rollup-based production builds with tree-shaking, code splitting, and asset hashing
- Simple configuration (`vite.config.ts` is typically under 20 lines)
- Industry standard as of 2025 — largest ecosystem of plugins and community knowledge
- Multi-app: each SPA gets its own `vite.config.ts` with independent `base` paths

#### Parcel (Original)

- Zero-config philosophy — good developer experience for simple projects
- Slower HMR than Vite in practice
- Smaller community and plugin ecosystem
- **Why not**: Vite has surpassed Parcel in ecosystem size, performance, and community support. No benefit to staying on
  Parcel for a rewrite.

#### Webpack

- Most mature bundler with the largest plugin ecosystem
- Significantly more configuration required than Vite
- Slower dev server (bundle-based, not ESM-native)
- **Why not**: configuration overhead and slower DX for no benefit at KF2's scale.

## State Management: Redux Toolkit + RTK Query

### Requirements

- Shared state between components (playback state, queue, session)
- Caching and invalidation for server data fetched via gRPC
- Predictable state updates with dev tools support
- Type-safe integration with TypeScript

### Decision

**Redux Toolkit** with **RTK Query** for the data-fetching layer.

- `createSlice` for UI-only state (supervised mode, user identity toggles)
- RTK Query `createApi` for server data, with a custom `baseQuery` that bridges to Connect-ES gRPC clients
- Both SPAs configure their own Redux store, importing shared slices from `@kf2/common`
- Redux DevTools integration for debugging cache state and dispatched actions

RTK Query's `onCacheEntryAdded` lifecycle handles server-streaming RPCs (future): open a gRPC stream, dispatch updates
into the cache, close on unsubscribe.

## API Transport: Connect-ES (gRPC-Web)

### Requirements

- Type-safe RPC calls from browser to Rust/Tonic server
- No sidecar proxy (Envoy, grpc-web-proxy)
- Generated client code from protobuf definitions
- Support for both unary and server-streaming RPCs

### Comparison

#### Connect-ES / @connectrpc/connect-web (Selected)

- Type-safe clients generated from protobuf service descriptors
- ~80% smaller bundle than the Google `grpc-web` library
- Works directly with `tonic-web` on the server — no Envoy proxy needed
- Supports Connect, gRPC, and gRPC-Web protocols from the same client
- `createClient(ServiceDescriptor, transport)` API is minimal and composable
- v2 uses `protoc-gen-es` v2 service descriptors directly — single codegen plugin for messages and services

#### grpc-web (Google) (Rejected)

- Official gRPC-Web implementation from Google
- Larger bundle size, more complex generated code
- Historically required an Envoy proxy (now works with tonic-web, but the library is heavier)
- **Why not**: Connect-ES provides the same functionality with a smaller footprint and better TypeScript integration.

#### REST / OpenAPI (Rejected)

- Would require maintaining both protobuf (for Rust) and OpenAPI (for TypeScript) schemas
- Loses the type-safety chain from proto → server → client
- **Why not**: the server already speaks gRPC via Tonic. Adding a REST layer is unnecessary indirection.

## Proto Codegen: Buf CLI

### Requirements

- Generate TypeScript types and service descriptors from `proto/*.proto`
- Single tool for linting, breaking-change detection, and code generation
- Compatible with Connect-ES v2

### Comparison

#### Buf CLI + protoc-gen-es (Selected)

- Single `buf generate` command — no manual `protoc` plugin orchestration
- `protoc-gen-es` v2 generates both message types and service descriptors in one pass
- `buf.gen.yaml` declaratively configures output paths and options
- Also supports `buf lint` and `buf breaking` for proto quality gates

#### protoc + plugins (Rejected)

- Requires installing `protoc` binary separately, plus each plugin
- Plugin version management is manual
- **Why not**: Buf wraps protoc and handles plugin resolution, eliminating a class of "works on my machine" issues.

## Package Manager: pnpm

### Requirements

- Monorepo workspace support (multiple packages with internal dependencies)
- Fast, deterministic installs
- Disk-efficient for a project with shared dependencies across packages

### Comparison

#### pnpm (Selected)

- Content-addressable storage — packages stored once on disk, hard-linked into each `node_modules`
- Native workspace protocol (`workspace:*`) for internal package references
- Strict `node_modules` structure prevents phantom dependencies
- Fastest install times in benchmarks

#### Yarn (Berry) (Rejected)

- Plug'n'Play mode breaks compatibility with some tools
- More complex configuration (`.yarnrc.yml`, plugins)
- **Why not**: pnpm is simpler and faster for the same workspace capabilities.

#### npm (Rejected)

- Workspace support exists but is less mature
- Flat `node_modules` allows phantom dependencies
- Slower installs, more disk usage (no content-addressable storage)
- **Why not**: pnpm is strictly better for monorepo use cases.

## Styling: CSS Modules

### Requirements

- Component-scoped styles to prevent conflicts
- Co-location with component files
- No runtime overhead

### Decision

**CSS Modules** — Vite supports them natively (any `.module.css` file). Styles are scoped to the importing component at
build time, producing unique class names. This matches the previous pattern (SCSS Modules) without the Sass preprocessor
dependency.
