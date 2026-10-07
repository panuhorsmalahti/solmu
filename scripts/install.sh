#!/bin/sh
set -eu

repo=panuhorsmalahti/solmu
api=${SOLMU_RELEASE_API:-https://api.github.com/repos/$repo/releases}
base=${SOLMU_RELEASE_BASE_URL:-https://github.com/$repo/releases/download}
component=${SOLMU_COMPONENT:-all}
case "$component" in
  all) selected='solmu-backend solmu solmu-desktop boxer boxer-gui muxer muxer-gui' ;;
  backend) selected=solmu-backend ;;
  cli) selected=solmu ;;
  desktop) selected=solmu-desktop ;;
  muxer) selected='solmu muxer' ;;
  muxer-gui) selected='solmu muxer-gui' ;;
  boxer) selected=boxer ;;
  boxer-gui) selected=boxer-gui ;;
  web) selected='' ;;
  *) echo 'Unknown Solmu component' >&2; exit 1 ;;
esac
service_dir=${SOLMU_SERVICE_DIR:-"$HOME/.solmu"}
web_destination=${SOLMU_WEB_INSTALL_DIR:-"$service_dir/web"}
if [ "$component" = web ]; then
  destination=${SOLMU_INSTALL_DIR:-"$web_destination"}
  command -v unzip >/dev/null 2>&1 || { echo 'Install unzip to extract the web client' >&2; exit 1; }
else
  destination=${SOLMU_INSTALL_DIR:-"$HOME/.local/bin"}
fi
destination=$(mkdir -p "$(dirname "$destination")" && cd "$(dirname "$destination")" && printf '%s/%s' "$PWD" "$(basename "$destination")")
case "$destination$service_dir$web_destination" in *'
'*) echo 'Installer paths must not contain newlines' >&2; exit 1 ;; esac
check_web_destination() {
  [ ! -L "$1" ] || { echo 'Web destination is a link' >&2; exit 1; }
  if [ -e "$1" ]; then
    [ -f "$1/.solmu-web" ] || { echo 'Web destination already exists and is not managed by Solmu' >&2; exit 1; }
    [ -z "$(find "$1" -type l -print)" ] || { echo 'Web destination contains links' >&2; exit 1; }
  fi
}
case "$component" in web) check_web_destination "$destination" ;; all) command -v unzip >/dev/null; check_web_destination "$web_destination" ;; esac
tag=${SOLMU_VERSION:-}
if [ -z "$tag" ]; then
  metadata=$(curl -fsSL "$api/latest") || {
    echo 'No Solmu release is available. See the GitHub Releases page.' >&2; exit 1;
  }
  tag=$(printf '%s\n' "$metadata" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
fi
case "$tag" in v*) ;; *) tag="v$tag" ;; esac
printf '%s\n' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' || { echo 'Invalid Solmu release version' >&2; exit 1; }
if [ "$component" = web ]; then
  asset="solmu-$tag-web.zip"
else
  case "$(uname -s)" in Linux) os=linux ;; Darwin) os=macos ;; *) echo 'Use install.ps1 on Windows.' >&2; exit 1 ;; esac
  case "$(uname -m)" in x86_64|amd64) arch=x86_64 ;; arm64|aarch64) arch=aarch64 ;; *) echo 'Unsupported CPU architecture' >&2; exit 1 ;; esac
  [ "$os-$arch" != linux-aarch64 ] || { echo 'Linux releases currently support x64. Build Solmu from source for another architecture.' >&2; exit 1; }
  asset="solmu-$tag-$os-$arch.tar.gz"
fi
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
curl -fsSL "$base/$tag/SHA256SUMS" -o "$temporary/SHA256SUMS"
verify_download() {
  cache_file=
  if [ -n "${SOLMU_INSTALL_CACHE_DIR:-}" ]; then
    mkdir -p "$SOLMU_INSTALL_CACHE_DIR"
    cache_file="$SOLMU_INSTALL_CACHE_DIR/$1"
  fi
  if [ -n "$cache_file" ] && [ -f "$cache_file" ] && [ ! -L "$cache_file" ]; then
    cp "$cache_file" "$temporary/$1"
  else
    curl -fsSL "$base/$tag/$1" -o "$temporary/$1"
  fi
  expected=$(awk -v file="$1" '$2 == file {print $1}' "$temporary/SHA256SUMS")
  printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' || { echo 'Missing release checksum' >&2; exit 1; }
  if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$temporary/$1" | awk '{print $1}');
  elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 "$temporary/$1" | awk '{print $1}');
  else echo 'A SHA-256 utility is required' >&2; exit 1; fi
  if [ "$actual" != "$expected" ]; then
    [ -z "$cache_file" ] || rm -f "$cache_file"
    echo 'Release checksum mismatch' >&2
    exit 1
  fi
  [ -z "$cache_file" ] || cp "$temporary/$1" "$cache_file"
}
verify_download "$asset"
if [ "$component" = all ]; then verify_download "solmu-$tag-web.zip"; fi
if [ "$component" = web ] || [ "$component" = all ]; then
  web_asset="solmu-$tag-web.zip"
  unzip -Z -1 "$temporary/$web_asset" > "$temporary/entries"
  while IFS= read -r entry; do
    case "$entry" in ''|/*|*[!A-Za-z0-9_./-]*) echo 'Unsafe path in web archive' >&2; exit 1 ;; esac
    case "/$entry" in */../*|*/./*|*//*) echo 'Unsafe path in web archive' >&2; exit 1 ;; esac
    case "$entry" in ..|.|*/..|*/.) echo 'Unsafe path in web archive' >&2; exit 1 ;; esac
  done < "$temporary/entries"
  unzip -Z -l "$temporary/$web_asset" | awk '/^[lbcps]/ {bad=1} END {exit bad}' || { echo 'Links and special files are not allowed in web archives' >&2; exit 1; }
  [ "$(sort "$temporary/entries" | uniq -d | wc -l | tr -d ' ')" = 0 ] || { echo 'Duplicate web archive entry' >&2; exit 1; }
  mkdir "$temporary/web"
  unzip -q "$temporary/$web_asset" -d "$temporary/web"
  [ -f "$temporary/web/index.html" ] && [ ! -L "$temporary/web/index.html" ] || { echo 'Missing web index.html' >&2; exit 1; }
fi
install_web() {
  mkdir -p "$1"
  # Keep old hashed assets for already-open browser tabs; switch the index last.
  if [ -d "$temporary/web/assets" ]; then mkdir -p "$1/assets"; cp -R "$temporary/web/assets/." "$1/assets/"; fi
  cp "$temporary/web/index.html" "$1/.index.html.new"
  mv "$1/.index.html.new" "$1/index.html"
  printf '%s\n' "$tag" > "$1/.solmu-web"
  printf 'Installed Solmu web %s to %s\nOpen http://127.0.0.1:3000 with the backend running.\n' "$tag" "$1"
}
register_component() {
  name=$1
  path=$2
  printf '%s\n' "$path" > "$service_dir/update-components/$name.new"
  mv "$service_dir/update-components/$name.new" "$service_dir/update-components/$name"
  printf '%s\n' "$tag" > "$service_dir/.solmu-version-$name"
}
if [ "$component" = web ]; then
  install_web "$destination"
  mkdir -p "$service_dir/update-components"
  chmod 700 "$service_dir" "$service_dir/update-components"
  register_component web "$destination"
  if [ "${SOLMU_NO_AUTO_UPDATE:-0}" != 1 ]; then
    installer_base=${SOLMU_INSTALLER_BASE_URL:-https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts}
    curl -fsSL "$installer_base/service.sh" -o "$temporary/service.sh"
    . "$temporary/service.sh"
    solmu_setup_autoupdate
  fi
  exit 0
fi

tar -tzf "$temporary/$asset" > "$temporary/entries"
while IFS= read -r entry; do
  case "$entry" in solmu-backend|solmu|solmu-cli|solmu-desktop|boxer|boxer-gui|muxer|muxer-gui) ;; *) echo 'Unexpected file in release archive' >&2; exit 1 ;; esac
done < "$temporary/entries"
[ "$(sort "$temporary/entries" | uniq -d | wc -l | tr -d ' ')" = 0 ] || { echo 'Duplicate native archive entry' >&2; exit 1; }
tar -tvzf "$temporary/$asset" | awk 'substr($0,1,1) != "-" {bad=1} END {exit bad}' || { echo 'Only regular binaries are allowed in release archives' >&2; exit 1; }
tar -xzf "$temporary/$asset" -C "$temporary"
legacy_cli=0
if [ -f "$temporary/solmu-cli" ]; then
  [ ! -e "$temporary/solmu" ] || { echo 'Ambiguous CLI binaries in archive' >&2; exit 1; }
  cp "$temporary/solmu-cli" "$temporary/solmu"
  legacy_cli=1
fi
for binary in solmu-backend solmu solmu-desktop boxer boxer-gui muxer muxer-gui; do
  [ -f "$temporary/$binary" ] && [ ! -L "$temporary/$binary" ] || { echo "Missing binary: $binary" >&2; exit 1; }
done
installer_base=${SOLMU_INSTALLER_BASE_URL:-https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts}
curl -fsSL "$installer_base/service.sh" -o "$temporary/service.sh"
. "$temporary/service.sh"
case "$component" in backend|all)
  if [ "${SOLMU_NO_SERVICE:-0}" != 1 ]; then solmu_stop_service; fi ;;
esac
mkdir -p "$destination"
for binary in $selected; do
  cp "$temporary/$binary" "$destination/$binary"
  chmod 755 "$destination/$binary"
done
mkdir -p "$service_dir/update-components"
chmod 700 "$service_dir" "$service_dir/update-components"
case "$component" in
  backend) if [ "${SOLMU_NO_SERVICE:-0}" != 1 ]; then register_component backend "$destination"; printf '%s\n' "$tag" > "$service_dir/.solmu-backend-version"; fi ;;
  cli) register_component cli "$destination" ;;
  desktop) register_component desktop "$destination" ;;
  boxer) register_component boxer "$destination" ;;
  boxer-gui) register_component boxer-gui "$destination" ;;
  muxer) register_component cli "$destination"; register_component muxer "$destination" ;;
  muxer-gui) register_component cli "$destination"; register_component muxer-gui "$destination" ;;
  web) register_component web "$destination" ;;
  all)
    for name in cli desktop boxer boxer-gui muxer muxer-gui; do register_component "$name" "$destination"; done
    if [ "${SOLMU_NO_SERVICE:-0}" != 1 ]; then register_component backend "$destination"; printf '%s\n' "$tag" > "$service_dir/.solmu-backend-version"; fi
    register_component web "$web_destination"
    ;;
esac
if [ "$legacy_cli" = 1 ]; then
  case " $selected " in *' solmu '*) cp "$temporary/solmu-cli" "$destination/solmu-cli"; chmod 755 "$destination/solmu-cli" ;; esac
fi
printf 'Installed Solmu %s (%s) to %s\nAdd this directory to PATH. Clients connect to a separately running solmu-backend.\n' "$tag" "$component" "$destination"
if [ "$component" = all ]; then install_web "$web_destination"; fi
case "$component" in backend|all)
  if [ "${SOLMU_NO_SERVICE:-0}" != 1 ]; then solmu_start_service; fi ;;
esac
if [ "${SOLMU_NO_AUTO_UPDATE:-0}" != 1 ]; then solmu_setup_autoupdate; fi
if [ "${SOLMU_NO_PATH:-0}" != 1 ]; then
  case ":$PATH:" in *":$destination:"*) ;; *)
    case "${SHELL:-}" in
      */zsh) profile="$HOME/.zprofile" ;;
      */bash)
        if [ -f "$HOME/.bash_profile" ]; then profile="$HOME/.bash_profile";
        elif [ -f "$HOME/.bash_login" ]; then profile="$HOME/.bash_login";
        else profile="$HOME/.profile"; fi ;;
      *) profile="$HOME/.profile" ;;
    esac
    escaped=$(printf '%s' "$destination" | sed "s/'/'\\''/g")
    line="export PATH='$escaped':\"\$PATH\""
    touch "$profile"
    grep -F -x "$line" "$profile" >/dev/null 2>&1 || printf '\n# Solmu installed programs\n%s\n' "$line" >> "$profile"
    printf 'Open a new login terminal to use the updated PATH.\n'
  esac
fi
