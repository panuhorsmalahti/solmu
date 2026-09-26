# Solmu

Solmu is an autonomous agent with a Rust backend and multiple frontend applications.

```text
backend/          Rust binary crate (solmu-backend)
  Cargo.toml
  src/main.rs     Empty entry point
clients/          Future frontend applications
```

With a Rust toolchain installed, run the backend from the repository root:

```sh
cargo run --manifest-path backend/Cargo.toml
```

The backend currently has no dependencies or application behavior.
