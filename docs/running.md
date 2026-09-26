# Run Solmu

## From source

Install Rust, then run from the repository root:

```sh
cp .env.example .env
# Fill OPENAI_API_KEY in .env.
cargo run -p solmu-backend
```

In PowerShell, use `Copy-Item .env.example .env` if the file does not exist.
The backend listens on `http://127.0.0.1:3000`. Start a client in another terminal.
Stop the backend with Ctrl+C. Conversations survive restarts in `solmu.db`.

## Docker backend

Install and start Docker (Linux containers on Windows). Rust is not needed on
the host for the container build.

```sh
docker build -t solmu .
docker run --rm --name solmu --env-file .env \
  -p 127.0.0.1:3000:3000 -v solmu-data:/data solmu
```

You can also use the published image `ghcr.io/panuhorsmalahti/solmu:latest`
instead of building locally. See [installation and releases](releases.md).

The named volume keeps conversations when the container stops. Provider keys
are supplied at runtime and excluded from the image. Stop with
`docker stop solmu`. Change the host port to connect clients on another port,
for example `-p 127.0.0.1:8080:3000` and
`SOLMU_BACKEND_URL=http://127.0.0.1:8080`.

See [configuration](configuration.md) and [the API guide](api.md).
