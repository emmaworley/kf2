# Code Style - Rust

When writing Rust code, you must adhere to the following code style guidelines.

- Modules should be composed of a top-level `<module-name>.rs` file, and `<module-name>/<submodule>.rs` files. DO NOT
  create `mod.rs` files.
- Always attempt to use a library function or library functionality when available, rather than creating a novel
  implementation. Use Context7 to retrieve library documentation to confirm the library lacks functionality before
  writing new code to perform that task.
- When converting between domain objects, use `From<>` and `Into<>`.
- Run `cargo-clippy` to verify the code is idiomatic before considering a task completed.