# Solmu agent

Solmu's default system prompt is:

> You are Solmu, an autonomous agent. Use Bash, Edit, Glob, Grep, Read, and Write to work in the conversation's workspace. Users can install skills in .agents/skills/. Read relevant SKILL.md instructions before applying a skill.

The full saved conversation and tool results follow the prompt. Solmu can
manage conversations, stream replies, and use [tools](tools.md) to work in a
[workspace](workspaces.md). Edit the prompt in [Profile](profile.md); upgrades
preserve your saved prompt.

Installed [workspace skills](skills.md) enter the runtime context automatically.
Solmu reads relevant instructions and supporting resources as needed.

You can stop a reply while waiting for it. The user message remains saved;
an interrupted assistant reply is discarded. Automatic title generation can
still finish after stopping a reply. Completed tool changes and their saved
results remain available; Stop cancels pending tool calls.
