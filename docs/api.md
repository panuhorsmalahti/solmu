# Conversation API

The default backend address is `http://127.0.0.1:3000`. Endpoints use JSON and the
versioned `/api/v1` prefix. This initial API is intended for local use;
authentication is not implemented.

| Method | Path | Body / result |
| --- | --- | --- |
| POST | `/api/v1/threads` | `{ "title": "Planning", "workspace": "/path/to/project" }`, both optional. New thread (201). |
| GET | `/api/v1/threads` | Threads, most recently updated first. |
| GET | `/api/v1/threads/{id}` | A thread. |
| PATCH | `/api/v1/threads/{id}` | Optional `title` and/or `model`. `model: null` clears the thread override. |
| DELETE | `/api/v1/threads/{id}` | Deletes the thread, messages, and tool history (204). |
| POST | `/api/v1/threads/{id}/messages` | `{ "content": "Hello" }`. Saved user message (201). |
| GET | `/api/v1/threads/{id}/messages` | Saved messages in conversation order. |
| POST | `/api/v1/threads/{id}/responses` | `{ "message_id": "saved-user-message-id" }`. Streams an assistant reply. |
| POST | `/api/v1/threads/{id}/stop` | Cancels the active response (204). Safe to repeat. |
| GET | `/api/v1/events` | WebSocket connection for live conversation updates. |
| GET | `/api/v1/profile` | Current `system_prompt`, nullable `model`, `backend_default_model`, and `edited_at` (UTC timestamp). |
| PUT | `/api/v1/profile` | Required `system_prompt`, optional `model`. Omission preserves the model; null clears it. Returns the saved profile. |
| GET | `/api/v1/models` | Provider, effective `default_model`, and named `models` (`id`, `name`). |
| GET | `/api/v1/tools` | Available tool definitions and their input schemas in `items`. |
| GET | `/api/v1/audit` | Saved tool calls across all conversations, newest first. Cursor paging with `limit` (1–100, default 25) and optional positive `before`; returns `items` and `next_cursor`. See [Audit](audit.md). |
| GET / POST | `/api/v1/tasks` | List tasks or create one with `name`, `prompt`, `schedule_kind` (`once` or `cron`), `schedule`, and optional `workspace`. |
| GET / PATCH / DELETE | `/api/v1/tasks/{id}` | Read, edit (`name`, `prompt`, `schedule_kind`, `schedule`, `enabled`), or remove a task. |
| POST | `/api/v1/tasks/{id}/run` | Start a run now (201). |
| GET | `/api/v1/tasks/{id}/runs` | Saved run history. See [scheduled tasks](tasks.md). |
| GET | `/api/v1/threads/{id}/skills` | Workspace skill catalog: `directory`, `items`, and validation `issues`. See [skills](skills.md). |
| GET | `/api/v1/threads/{id}/mcp` | Workspace MCP servers, protocol versions, connection status, tools, and issues. See [MCP](mcp.md). |
| GET | `/api/v1/threads/{id}/plugins` | Installed Agent Plugins, their components, and loading issues. See [plugins](plugins.md). |
| GET | `/api/v1/threads/{id}/tools` | Saved tool activity in execution order; supports pagination. |

List endpoints accept `limit` (1–100, default 50) and `offset` (default 0).
Results have `{ "items": [], "limit": 50, "offset": 0 }`.
Audit uses cursor paging instead: pass its returned `next_cursor` as `before`
to get the next older page. Audit rows include the conversation title, global
sequence, and creation time in addition to the tool-run fields below.
Threads contain `id`, `title`, nullable `model`, `workspace`, `created_at`, and `updated_at`. Messages contain
`id`, `thread_id`, `role`, `content`, `reply_to_id`, and `created_at`.

## Streaming replies

Save the user message first, then request its response. Only the latest user
message can receive a response. The full saved conversation is sent to the LLM.
The response is `text/event-stream` with JSON payloads:

| Event | Payload |
| --- | --- |
| `start` | `{ "message_id": "..." }` |
| `delta` | `{ "text": "a piece of the reply" }` |
| `reset` | Clear the partial assistant text before a tool round. |
| `tool_start` | A saved tool run with `status: "running"`. |
| `tool_result` | The saved tool run with its result and final status. |
| `done` | The complete, persisted assistant message. |
| `error` | `{ "error": { "code": "...", "message": "..." } }` |
| `stopped` | `{ "message_id": "..." }`; the response was cancelled. |

The assistant message is saved before `done`. Partial replies are not saved on
failure or disconnect. A thread is locked while its reply streams: concurrent
messages, title changes, deletion, and additional replies return 409.

HTTP errors use `{ "error": { "code": "...", "message": "..." } }`.
Missing resources return 404, invalid input 400/422, a missing provider 503,
and provider request failures 502. Streaming failures arrive as `error` events.

Tool runs contain `id`, `thread_id`, `message_id`, `call_id`, `name`,
`arguments` (JSON), `status`, nullable `result`, `started_at`, and `finished_at`.
Results have `{ "success": true, "output": ... }`; failed results set
`success: false`. States are queued, running, completed, failed, cancelled, or
interrupted. A tool-only model turn continues with its results until the model
produces the final text reply. Saved native tool exchanges are included in
future model requests and deleted with their thread. See [tools](tools.md).

## Live updates

`{ "type": "skills_changed", "thread_id": "..." }` means fetch that thread's
workspace skill catalog again. Catalogs update when skill files change.

`{ "type": "mcp_changed", "thread_id": "..." }` means fetch that thread's
[MCP server status](mcp.md) again. Tool lists and connections update automatically.

`{ "type": "plugins_changed", "thread_id": "..." }` means fetch that thread's
[plugin catalog](plugins.md) again.

Connect to `ws://127.0.0.1:3000/api/v1/events` (use `wss` behind HTTPS).
The server sends `{ "type": "ready" }`, then
`{ "type": "conversation_changed", "thread_id": "..." }` for thread changes,
saved messages, completed replies, and generated names. Reload affected data
after these notifications. A null thread ID means reload everything. Reconnect
and reload after a lost connection; events are not replayed.

`{ "type": "profile_changed" }` means fetch the current Profile and model
catalog again. Preserve unsaved edits. System prompt revisions are saved
atomically with the current profile; only changed text creates a revision.
History is not exposed by an API yet. Existing profiles get an initial history
entry when upgrading, with the migration time as their edit timestamp.

Models apply in this order: thread override, Profile override, backend default.
Profile's `backend_default_model` always describes the fallback without either
override. Model IDs must belong to the configured backend provider. Prompts
must contain text and be at most 64000 bytes. Workspaces are absolute folders
on the backend host; they must exist when explicitly supplied. Without a
workspace, the backend creates its [default folder](workspaces.md).
