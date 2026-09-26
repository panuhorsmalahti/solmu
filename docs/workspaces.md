# Workspaces

Every new conversation has a saved workspace: the directory on the backend host
where Solmu works. Threads created in the CLI use its current working directory;
Muxer sessions use their selected working directory.

Desktop and web threads use `~/.solmu/workspace` on Linux/macOS or
`%USERPROFILE%\.solmu\workspace` on Windows. Solmu creates that default folder
when needed. Set `SOLMU_WORKSPACE` on the backend to choose another default.
Docker uses `/data/workspace`, so the data volume preserves it across restarts.

The CLI's directory must also exist on the backend host. When using a remote
backend or Docker, mount that workspace at the same path. A conversation keeps
its workspace when opened from another client.

CLI and desktop show the selected thread's workspace. An existing thread created
before workspace support uses the backend default when work is needed.
