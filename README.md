# kf2: electric boogaloo

## Dependencies

In order to build and develop KF2 you will need the following toolchains:

- A working Rust toolchain
- A working NodeJS toolchain (recommend using an `nvm`-compatible version manager)
- `just` (installable via `cargo install just`)

If you make database schema changes you will additionally need `diesel_cli` with the sqlite feature.

### Setup

Assuming your toolchains are working, run `just dev-setup`.

## Running
