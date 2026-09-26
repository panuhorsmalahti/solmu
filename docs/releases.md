# Releases and installation

Solmu releases are stored in [GitHub Releases](https://github.com/panuhorsmalahti/solmu/releases).
They contain backend, CLI, desktop, sandbox, and muxer binaries, the web build, and
`SHA256SUMS`. Linux x64, Windows x64, and Intel/Apple Silicon macOS are packaged.
Linux desktop binaries require the usual X11/Wayland runtime libraries.
macOS and Windows binaries are currently unsigned.

## Install

Linux/macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install.sh | sh
```

Windows PowerShell:

Use Windows PowerShell 5.1 or PowerShell 7:

```powershell
irm https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts/install.ps1 | iex
```

The installers download the latest production release and verify checksums.
They need an existing release; before the first release, build from source.
Unix installs into `~/.local/bin` (add it to PATH); Windows installs into
`%LOCALAPPDATA%\Solmu\bin` and adds that directory to your user PATH.
Set `SOLMU_VERSION=0.1.0` to choose a specific version, or
`SOLMU_INSTALL_DIR` to choose another directory. PowerShell also supports
`-Version`, `-InstallDir`, and `-NoPath` when running a downloaded script.

Create `.env` in your working directory with `LLM_PROVIDER=openai`,
`OPENAI_API_KEY`, and optionally `LLM_MODEL=gpt-6-sol`. Start `solmu-backend`,
then start `solmu-cli` or `solmu-desktop` in another terminal.
See [client usage](clients.md) and [configuration](configuration.md).

## Publish a production release

Commit the desired version in the native package manifests, push to `main`,
then open **Actions → Production release → Run workflow**. Choose `main` and
enter a version such as `0.1.0`. The workflow validates the version and checks
that the tag is new, runs all CI checks, builds the platform archives and web
assets, publishes the versioned Docker image, then creates the GitHub Release
and tag pointing to the selected commit. It never releases automatically.

## Docker images

The **Publish Docker image** workflow publishes the backend to
`ghcr.io/panuhorsmalahti/solmu`. Relevant pushes to `main` update `latest` and a
`sha-<commit>` tag; production releases add `v<version>`.

```sh
docker run --rm --env-file .env -p 127.0.0.1:3000:3000 \
  -v solmu-data:/data ghcr.io/panuhorsmalahti/solmu:latest
```

The image is public and can be pulled without a GitHub registry login.
