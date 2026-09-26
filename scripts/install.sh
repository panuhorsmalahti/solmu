#!/bin/sh
set -eu

repo=panuhorsmalahti/solmu
api=${SOLMU_RELEASE_API:-https://api.github.com/repos/$repo/releases}
base=${SOLMU_RELEASE_BASE_URL:-https://github.com/$repo/releases/download}
destination=${SOLMU_INSTALL_DIR:-"$HOME/.local/bin"}
tag=${SOLMU_VERSION:-}
if [ -z "$tag" ]; then
  metadata=$(curl -fsSL "$api/latest") || {
    echo 'No Solmu release is available. See the GitHub Releases page.' >&2; exit 1;
  }
  tag=$(printf '%s\n' "$metadata" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
fi
case "$tag" in v*) ;; *) tag="v$tag" ;; esac
printf '%s\n' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' || { echo 'Invalid Solmu release version' >&2; exit 1; }
case "$(uname -s)" in Linux) os=linux ;; Darwin) os=macos ;; *) echo 'Use install.ps1 on Windows.' >&2; exit 1 ;; esac
case "$(uname -m)" in x86_64|amd64) arch=x86_64 ;; arm64|aarch64) arch=aarch64 ;; *) echo 'Unsupported CPU architecture' >&2; exit 1 ;; esac
[ "$os-$arch" != linux-aarch64 ] || { echo 'Linux releases currently support x64. Build Solmu from source for another architecture.' >&2; exit 1; }
asset="solmu-$tag-$os-$arch.tar.gz"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
curl -fsSL "$base/$tag/$asset" -o "$temporary/$asset"
curl -fsSL "$base/$tag/SHA256SUMS" -o "$temporary/SHA256SUMS"
expected=$(awk -v file="$asset" '$2 == file {print $1}' "$temporary/SHA256SUMS")
printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' || { echo 'Missing release checksum' >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$temporary/$asset" | awk '{print $1}');
elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 "$temporary/$asset" | awk '{print $1}');
else echo 'A SHA-256 utility is required' >&2; exit 1; fi
[ "$actual" = "$expected" ] || { echo 'Release checksum mismatch' >&2; exit 1; }
tar -tzf "$temporary/$asset" | while IFS= read -r entry; do
  case "$entry" in solmu-backend|solmu-cli|solmu-desktop|boxer|muxer) ;; *) echo 'Unexpected file in release archive' >&2; exit 1 ;; esac
done
tar -xzf "$temporary/$asset" -C "$temporary"
mkdir -p "$destination"
for binary in solmu-backend solmu-cli solmu-desktop boxer muxer; do
  [ -f "$temporary/$binary" ] && [ ! -L "$temporary/$binary" ] || { echo "Missing binary: $binary" >&2; exit 1; }
  cp "$temporary/$binary" "$destination/$binary"
  chmod 755 "$destination/$binary"
done
printf 'Installed Solmu %s to %s\nAdd this directory to PATH, then run solmu-backend and solmu-cli in separate terminals.\n' "$tag" "$destination"
