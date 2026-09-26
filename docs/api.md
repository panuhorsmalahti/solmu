# Conversation API

The default backend address is `http://127.0.0.1:3000`. Endpoints use JSON and the
versioned `/api/v1` prefix. This initial API is intended for local use;
authentication is not implemented.

| Method | Path | Body / result |
| --- | --- | --- |
| POST | `/api/v1/threads` | `{ "title": "Planning" }`, optional title. New thread (201). |
| GET | `/api/v1/threads` | Threads, most recently updated first. |
| GET | `/api/v1/threads/{id}` | A thread. |
| PATCH | `/api/v1/threads/{id}` | `{ "title": "New title" }`. |
| DELETE | `/api/v1/threads/{id}` | Deletes the thread and its messages (204). |
| POST | `/api/v1/threads/{id}/messages` | `{ "content": "Hello" }`. Saved user message (201). |
| GET | `/api/v1/threads/{id}/messages` | Saved messages in conversation order. |
| POST | `/api/v1/threads/{id}/responses` | `{ "message_id": "saved-user-message-id" }`. Streams an assistant reply. |

List endpoints accept `limit` (1–100, default 50) and `offset` (default 0).
Results have `{ "items": [], "limit": 50, "offset": 0 }`.
Threads contain `id`, `title`, `created_at`, and `updated_at`. Messages contain
`id`, `thread_id`, `role`, `content`, `reply_to_id`, and `created_at`.

## Streaming replies

Save the user message first, then request its response. Only the latest user
message can receive a response. The full saved conversation is sent to the LLM.
The response is `text/event-stream` with JSON payloads:

| Event | Payload |
| --- | --- |
| `start` | `{ "message_id": "..." }` |
| `delta` | `{ "text": "a piece of the reply" }` |
| `done` | The complete, persisted assistant message. |
| `error` | `{ "error": { "code": "...", "message": "..." } }` |

The assistant message is saved before `done`. Partial replies are not saved on
failure or disconnect. A thread is locked while its reply streams: concurrent
messages, title changes, deletion, and additional replies return 409.

HTTP errors use `{ "error": { "code": "...", "message": "..." } }`.
Missing resources return 404, invalid input 400/422, a missing provider 503,
and provider request failures 502. Streaming failures arrive as `error` events.
