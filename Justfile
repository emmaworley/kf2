set windows-shell := ["cmd", "/c"]


# Install frontend dependencies
[working-directory: "src/frontend"]
install:
    pnpm install

# Build frontend SPAs
[working-directory: "src/frontend"]
build-frontend: install
    pnpm build

# Build Rust server
build-server:
    cargo build --release

# Build both frontend SPAs and the Rust server
build: build-frontend build-server

# Start all dev servers (Vite + Rust) concurrently
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    pushd src/frontend >/dev/null 2>&1
    pnpm install && pnpm run generate

    # Start Vite dev servers in background
    pnpm dev:remocon &
    REMOCON_PID=$!
    pnpm dev:projector &
    PROJECTOR_PID=$!

    # Give Vite a moment to start
    popd >/dev/null 2>&1
    sleep 2

    # Start Rust server (proxies to Vite dev servers via CLI args)
    cargo run -p server -- --projector-root http://localhost:5174 --remocon-root http://localhost:5173

    # Clean up on exit
    kill $REMOCON_PID $PROJECTOR_PID 2>/dev/null || true

# Install any tools not specified by Cargo.toml/package.json
dev-setup:
    cargo install diesel_cli --no-default-features --features sqlite-bundled

# Run TypeScript type checking across all packages
ts-typecheck:
    pnpm typecheck
