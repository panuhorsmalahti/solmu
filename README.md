# Solmu

Solmu is an autonomous agent with a Rust backend and multiple frontend applications.

```text
backend/          Rust binary crate (solmu-backend)
  Cargo.toml
  src/main.rs     API server
clients/          Frontend applications
docs/             Usage and configuration
```

## Run locally

With a Rust toolchain installed, run the backend from the repository root:

```sh
cargo run --manifest-path backend/Cargo.toml
```

## Run with Docker

Install [Docker](https://docs.docker.com/get-started/get-docker/) and start it.
On Windows, use Docker Desktop with Linux containers.

From the repository root, build the image and run Solmu:

```sh
docker build -t solmu .
docker run --rm -p 127.0.0.1:3000:3000 solmu
```

The Docker build installs Rust and compiles the backend inside the container;
Rust does not need to be installed on your computer for this workflow.

Solmu listens on `http://127.0.0.1:3000`. The API has no routes yet, so requests
return `404 Not Found`. WebSocket support is enabled for future streaming
endpoints; no WebSocket endpoint exists yet.

Stop a local run with Ctrl+C. For a container, use `docker stop <container-id>`
from another terminal.

See [configuration](docs/configuration.md) for the listen address and LLM provider
environment variables.
