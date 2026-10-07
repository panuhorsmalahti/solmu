# Memories

Memories are durable facts shared across conversations and stored in Solmu's
SQLite database. The agent can search, read, write, update, and delete them with
its `Memory` tool. It saves a memory when you ask or clearly expect something to
be remembered, and checks for duplicates before writing.

Before a reply, Solmu compares words in your latest message with saved memory
text. Up to five matching entries are added to the request's system context,
ordered by relevance. This lightweight matching helps with direct topics such
as asking about cats when a saved fact mentions your cat. It does not send every
memory with every request. Memory text is treated as user-provided facts, not
instructions.

## Browse memories

The **Memories** page is a read-only list, newest first. It loads more entries as
you scroll and shows when each one was saved. The list updates when Solmu adds
or changes a memory during a conversation.

Only the agent manages memories through its `Memory` tool. Ask Solmu to remember
a fact; it can then search, read, update, or delete memories when appropriate.

- **Web:** open **Memories** in the sidebar or visit `/memories`.
- **Desktop:** choose **Memories** in the sidebar, or enter `/memories`.
- **CLI:** enter `/memories`; PageDown loads older entries.
- **Android and iOS:** open **More → Memories**.
- **Muxer:** enter `/memories` in a Solmu pane.

Memories are shared for the backend installation. When Solmu deletes or updates
a memory, future prompts use the new memory state. Existing conversation
messages and summaries are not rewritten.

## Screenshots

### CLI

![CLI Memories](screenshots/cli-memories.png)

### Desktop

![Desktop Memories](screenshots/desktop-memories.png)

### Web

![Web Memories](screenshots/web-memories.png)
