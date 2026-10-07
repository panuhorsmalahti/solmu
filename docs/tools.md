# Working with tools

Solmu can act on a conversation's [workspace](workspaces.md) while answering.
Ask it to inspect files, make a change, or run a command; the selected model
decides which tools to call. Use a model that supports tool calling.

| Tool | What it does |
| --- | --- |
| Bash | Runs a noninteractive Bash command from the workspace. |
| Read | Reads a UTF-8 file, optionally by line range. |
| Write | Creates or replaces a UTF-8 file, including missing directories. |
| Edit | Replaces exact text; ambiguous matches need more context or `replace_all`. |
| Glob | Finds files with patterns such as `**/*.rs`. |
| Grep | Searches UTF-8 files with a regular expression. |
| Memory | Searches, reads, saves, updates, or deletes shared facts. |

CLI, desktop, web, and Muxer show each tool's name, state, arguments, and result.
In the web client, click a tool row to expand its details. Tool activity is saved
with the conversation and remains available after reopening it or restarting
the backend. Failures are returned to the model so it can adjust its approach.
Saved memories live in SQLite and matching facts are included in later requests.
See [Memories](memories.md) to browse them.
Open [Audit](audit.md) to review all saved calls across conversations in one
ordered list, with expandable arguments and results.

Stop also stops tool execution. Bash and its child processes are terminated;
queued calls are cancelled. A file write that has already started finishes
atomically. Completed changes remain in the workspace. If a connection or
backend stops unexpectedly, unfinished calls are marked interrupted; Solmu
does not automatically repeat them.

## Workspace access

File tools accept relative paths or absolute paths within the workspace. They
reject parent-directory traversal and symlinks that resolve outside it. They
read files up to 1 MB and skip binary or oversized files while searching.
Read defaults to 200 lines; its maximum is 2,000. Searches examine at most
20,000 entries and return up to 500 paths or 200 matching lines.

Bash runs with the backend's OS permissions, including its network access.
Choosing a workspace sets its working directory; it does not sandbox shell
commands. To apply OS restrictions to tools, launch the **backend** through
[Boxer](boxer.md). Running a client through Boxer restricts that client.
On Windows, install Git for Windows to provide Bash. The Docker backend
includes Bash.

A Bash call is limited to 30 seconds and 64 KB of combined output. Tool
arguments are limited to 32 KB per call. A reply allows up to 16 model rounds
and 64 tool calls. Truncated output is marked in the result.
