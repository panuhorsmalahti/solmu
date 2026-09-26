# Solmu agent

Solmu starts each conversation request with the system prompt:

> You are Solmu, an autonomous agent. Use Bash, Edit, Glob, Grep, Read, and Write to work in the conversation's workspace.

The full saved conversation and tool results follow the prompt. Solmu can
manage conversations, stream replies, and use [tools](tools.md) to work in a
[workspace](workspaces.md). Edit the prompt in [Profile](profile.md); upgrades
preserve your saved prompt.

You can stop a reply while waiting for it. The user message remains saved;
an interrupted assistant reply is discarded. Automatic title generation can
still finish after stopping a reply. Completed tool changes and their saved
results remain available; Stop cancels pending tool calls.
