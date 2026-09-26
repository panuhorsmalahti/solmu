# Solmu configuration

Solmu reads configuration from environment variables when it starts. Restart
Solmu after changing them. A local `.env` file is not loaded automatically.

## API server

| Variable | Local default | Docker default | Purpose |
| --- | --- | --- | --- |
| `SOLMU_BIND_ADDR` | `127.0.0.1:3000` | `0.0.0.0:3000` | IP address and port for the API server. IPv6 addresses use brackets, such as `[::1]:3000`. |

The router is currently empty. HTTP requests return `404 Not Found`. WebSocket
support is available for future endpoints, but there is no WebSocket endpoint yet.

Change the address for a local run in PowerShell:

```powershell
$env:SOLMU_BIND_ADDR = "127.0.0.1:8080"
cargo run --manifest-path backend/Cargo.toml
```

Or in a POSIX shell:

```sh
SOLMU_BIND_ADDR=127.0.0.1:8080 cargo run --manifest-path backend/Cargo.toml
```

With Docker, change the host port while leaving Solmu's container port at 3000:

```sh
docker run --rm -p 127.0.0.1:8080:3000 solmu
```

## LLM providers

Solmu includes genai, but does not make LLM requests or select models yet.
Provider credentials are not required to start the current server. When model
calls are added, genai's default provider adapters use these variable names;
set the credentials for the provider you plan to use.

| Provider | Credential variable |
| --- | --- |
| OpenAI, including Responses | `OPENAI_API_KEY` |
| Anthropic | `ANTHROPIC_API_KEY` |
| Google Gemini | `GEMINI_API_KEY` |
| xAI | `XAI_API_KEY` |
| Groq | `GROQ_API_KEY` |
| DeepSeek | `DEEPSEEK_API_KEY` |
| OpenRouter | `OPEN_ROUTER_API_KEY` |
| Together | `TOGETHER_API_KEY` |
| Fireworks | `FIREWORKS_API_KEY` |
| Cohere | `COHERE_API_KEY` |
| Nebius | `NEBIUS_API_KEY` |
| Xiaomi MiMo | `MIMO_API_KEY` |
| Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Z.ai | `ZAI_API_KEY` |
| BigModel | `BIGMODEL_API_KEY` |
| Aliyun | `ALIYUN_API_KEY` |
| Baidu | `BAIDU_API_KEY` |
| AIHubMix | `AIHUBMIX_API_KEY` |
| GitHub Models | `GITHUB_TOKEN` |
| OpenCode Go | `OPENCODE_GO_API_KEY` |
| Ollama Cloud | `OLLAMA_API_KEY` |
| Google Vertex AI | `VERTEX_API_KEY` (bearer access token) |
| AWS Bedrock with bearer authentication | `BEDROCK_API_KEY` |

Vertex AI also uses `VERTEX_PROJECT_ID` and optionally `VERTEX_LOCATION`
(defaults to `global`). Bedrock uses `AWS_REGION`, falling back to
`AWS_DEFAULT_REGION`, then `us-east-1`. AWS SigV4 authentication is not enabled
in Solmu's current build.

Local Ollama requires no API key and defaults to `http://localhost:11434/`.
That address refers to the container itself when running in Docker. Solmu does
not yet expose a configuration option for custom provider endpoints.

These names match [genai 0.6.5's provider adapters](https://docs.rs/genai/0.6.5/genai/adapter/index.html).

## Supply provider credentials

For a local PowerShell session:

```powershell
$env:ANTHROPIC_API_KEY = "your-api-key"
cargo run --manifest-path backend/Cargo.toml
```

For Docker, put the relevant variables in an untracked `.env` file:

```dotenv
ANTHROPIC_API_KEY=your-api-key
```

Pass that file when starting the container:

```sh
docker run --rm --env-file .env -p 127.0.0.1:3000:3000 solmu
```

The Docker image excludes `.env` files. Credentials supplied this way are
available at runtime and are not embedded in the image.
