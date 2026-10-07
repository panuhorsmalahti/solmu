# Solmu documentation

Solmu is an autonomous agent with terminal, desktop, mobile, and web clients.
Use your choice of model to work on conversations and files in a workspace.

## Get started

- [Install the complete bundle](bundle.md), or [choose individual modules](releases.md#individual-components).
- [Run the backend as a service](services.md) or [inside Docker](running.md#docker-backend).
- [Configure your provider and model](configuration.md).

## Choose a client

All clients share conversations, streamed replies, model settings, and workspace
tools. The backend must be running before you open a client.

- [Terminal](cli.md): run `solmu` in your project folder.
- [Desktop](desktop.md): open `solmu-desktop`.
- [Web](web.md): open `http://127.0.0.1:3000`.
- [Android](android.md): build the native app with Android Studio or Gradle.
- [iOS](ios.md): build the native SwiftUI app with Xcode on a Mac.
- [Muxer](muxer.md): keep Solmu, shells, and commands together in terminal spaces.
- [Muxer GUI](muxer-gui.md): manage local spaces and terminals in a native window.

See the [client overview](clients.md) for screenshots and shared features.

The [Solmu website](https://panuhorsmalahti.github.io/solmu/) also has a page
for each workspace product: [Muxer](https://panuhorsmalahti.github.io/solmu/muxer/),
[Boxer](https://panuhorsmalahti.github.io/solmu/boxer/), and
[Congregator](https://panuhorsmalahti.github.io/solmu/congregator/).

## Work with Solmu

- [Profile](profile.md): edit the system prompt and default model.
- [Memories](memories.md): save and browse facts recalled across conversations.
- [Audit](audit.md): review tool calls and the prompt cache hit rate.
- [Scheduled tasks](tasks.md): run Solmu once later or on a recurring schedule.
- [Webhooks](webhooks.md): start conversations from GitHub or other authenticated events.
- [Workspaces](workspaces.md): choose the folder Solmu works in.
- [Tools](tools.md): read, search, edit, and write files or run commands.
- [Skills](skills.md): install reusable project instructions in `.agents/skills/`.
- [MCP](mcp.md): connect workspace tools and inspect server status in every client.
- [Plugins](plugins.md): install portable packages with skills and MCP tools.
- [Congregator](congregator.md): manage Solmu agents in Kubernetes sandboxes.
- [Boxer](boxer.md): launch a program with optional OS sandbox controls.

## Make room with Muxer

- [Git worktrees](muxer-worktrees.md)
- [Shells and commands](muxer-commands.md)
- [Settings](muxer-configuration.md)
- [Layout and terminal automation](muxer-automation.md)
- [Native Solmu prompts](muxer-agents.md)
- [Direct terminal attachment](muxer-terminals.md)

## Reference

- [Conversation API](api.md)
- [Build and test](development.md)
- [Installation and releases](releases.md)
- [License](license.md)
