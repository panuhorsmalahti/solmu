# Solmu configuration

The terminal workspace uses `SOLMU_BACKEND_URL` and optionally `SOLMU_CLI_PATH`
to locate its CLI executable. See [muxer configuration](muxer.md).

For the Linux sandbox launcher, `SOLMU_CGROUP_ROOT` optionally chooses an
existing delegated cgroup v2 parent. Systemd delegation is detected automatically
when this variable is unset. See [sandbox setup and limits](boxer.md).

Solmu loads `.env` from the working directory (or a parent directory) at startup.
Environment variables already set in your shell take precedence. Restart the
backend after changing configuration.

Copy `.env.example` to `.env` and fill `OPENAI_API_KEY` to use OpenAI. `.env` is
ignored by Git. Keep provider credentials on the backend; clients never need them.

## API server

| Variable | Local default | Docker default | Purpose |
| --- | --- | --- | --- |
| `SOLMU_BIND_ADDR` | `127.0.0.1:3000` | `0.0.0.0:3000` | IP address and port for the API server. IPv6 addresses use brackets, such as `[::1]:3000`. |
| `SOLMU_DATABASE_URL` | `sqlite://solmu.db` | `sqlite:///data/solmu.db` | SQLite file. Its parent directory must exist. Tables are created automatically. |
| `SOLMU_BACKEND_URL` | `http://127.0.0.1:3000` | Same | Backend address used by Rust clients. |
| `SOLMU_WORKSPACE` | `~/.solmu/workspace` (Windows: `%USERPROFILE%\.solmu\workspace`) | `/data/workspace` | Default working folder for threads without an explicit workspace. Created automatically. |

See [the API guide](api.md) for conversation endpoints. Replies currently stream
over Server-Sent Events. All clients use `/api/v1/events` WebSocket notifications
to update conversations when another client changes them, with automatic reconnect.

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

Solmu selects the first provider with a nonempty credential in the environment.
The priority follows the table below. Set `LLM_PROVIDER` to choose explicitly
when several providers are configured, for example `openai`, `anthropic`,
`gemini`, `groq`, `openrouter`, or `ollama`. Provider names are case insensitive.

Set `LLM_MODEL` to your preferred model. Otherwise Solmu uses the first model
listed by the provider adapter. Set `LLM_ENDPOINT` to override the provider's
native API base URL, including its API path (for example `/v1/` for OpenAI).

You can change models without restarting: set the optional Model field in
[Profile](profile.md), or select a model for an individual thread. The order is
thread model, then Profile model, then `LLM_MODEL` or provider discovery. An
empty Profile model field displays the backend's actual fallback. Clearing an
override restores the next setting in this order. The provider still comes
from backend configuration.

For OpenAI, an example is `LLM_MODEL=gpt-6-sol` and
`LLM_TITLE_MODEL=gpt-6-luna`. `LLM_TITLE_MODEL` chooses a cheaper model to name
conversations after their first user message. It uses the selected provider by
default; a `provider::model` value can choose another configured provider.
When unset or when the cheaper model fails, naming uses the main model. If both
fail, the thread keeps “New conversation”. Naming runs in the background and
never overwrites a manually chosen title. This makes an additional LLM request.

Credentials are optional for starting the backend and managing conversations.
Sending a reply request without a configured provider returns an error; the
user message remains saved. Failed or interrupted replies are not saved as
completed assistant messages.

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
That address refers to the container itself when running in Docker. Choose
`LLM_PROVIDER=ollama` and an installed model with `LLM_MODEL`. In Docker, set
`LLM_ENDPOINT` to a reachable Ollama address, such as
`http://host.docker.internal:11434/` on Docker Desktop.

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
