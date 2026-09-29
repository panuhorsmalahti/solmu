#!/bin/sh
# Sourced by the installer after all release files have been verified.
solmu_stop_service() {
  case "$(uname -s)" in
    Linux) command -v systemctl >/dev/null || { echo 'systemd user services are required (or set SOLMU_NO_SERVICE=1)' >&2; exit 1; }; systemctl --user stop solmu-backend.service 2>/dev/null || true ;;
    Darwin) launchctl bootout "gui/$(id -u)/dev.solmu.backend" 2>/dev/null || true ;;
  esac
}

solmu_start_service() {
  mkdir -p "$service_dir"
  chmod 700 "$service_dir"
  cat > "$service_dir/auto-update.sh.new" <<'EOF'
#!/bin/sh
set -eu
state_dir=${SOLMU_SERVICE_DIR:-$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)}
enabled=true
if [ -f "$state_dir/.env" ]; then
  while IFS= read -r line || [ -n "$line" ]; do
    line=${line%%#*}
    case "$line" in SOLMU_AUTO_UPDATE=*) enabled=${line#*=}; break ;; esac
  done < "$state_dir/.env"
fi
enabled=$(printf '%s' "$enabled" | tr -d '[:space:]')
case "$enabled" in \"*\") enabled=${enabled#\"}; enabled=${enabled%\"} ;; \'*\') enabled=${enabled#\'}; enabled=${enabled%\'} ;; esac
case "$enabled" in 0|false|FALSE|no|NO|off|OFF) exit 0 ;; esac
api=${SOLMU_RELEASE_API:-https://api.github.com/repos/panuhorsmalahti/solmu/releases}
latest=$(curl -fsSL "$api/latest" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
case "$latest" in v[0-9]*.[0-9]*.[0-9]*) ;; *) exit 0 ;; esac
[ "$(cat "$state_dir/.solmu-backend-version" 2>/dev/null || true)" != "$latest" ] || exit 0
temporary=$(mktemp)
trap 'rm -f "$temporary"' EXIT HUP INT TERM
base=${SOLMU_INSTALLER_BASE_URL:-https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts}
curl -fsSL "$base/install.sh" -o "$temporary"
SOLMU_COMPONENT=backend SOLMU_NO_PATH=1 SOLMU_VERSION="$latest" sh "$temporary"
EOF
  chmod 700 "$service_dir/auto-update.sh.new"
  mv "$service_dir/auto-update.sh.new" "$service_dir/auto-update.sh"
  if [ ! -e "$service_dir/.env" ]; then
    (umask 077; printf 'LLM_PROVIDER=openai\nOPENAI_API_KEY=\nLLM_MODEL=gpt-6-sol\nSOLMU_WEB_DIR=web\nSOLMU_AUTO_UPDATE=true\n' > "$service_dir/.env")
  fi
  case "$(uname -s)" in
    Linux)
      unit_dir=${XDG_CONFIG_HOME:-"$HOME/.config"}/systemd/user
      mkdir -p "$unit_dir"
      # systemd expands percent specifiers even inside quoted paths.
      executable=$(printf '%s' "$destination/solmu-backend" | sed 's/\\/\\\\/g; s/"/\\"/g; s/%/%%/g')
      working=$(printf '%s' "$service_dir" | sed 's/\\/\\\\/g; s/"/\\"/g; s/%/%%/g')
      cat > "$unit_dir/solmu-backend.service" <<EOF
[Unit]
Description=Solmu backend
After=network.target

[Service]
ExecStart="$executable"
WorkingDirectory="$working"
Restart=on-failure
RestartSec=3
UMask=0077

[Install]
WantedBy=default.target
EOF
      systemctl --user daemon-reload
      systemctl --user enable --now solmu-backend.service
      cat > "$unit_dir/solmu-auto-update.service" <<EOF
[Unit]
Description=Update Solmu backend

[Service]
Type=oneshot
ExecStart=/bin/sh "$service_dir/auto-update.sh"
WorkingDirectory="$service_dir"
EOF
      cat > "$unit_dir/solmu-auto-update.timer" <<'EOF'
[Unit]
Description=Check for Solmu backend updates daily

[Timer]
OnCalendar=daily
RandomizedDelaySec=2h
Persistent=true

[Install]
WantedBy=timers.target
EOF
      systemctl --user daemon-reload
      systemctl --user enable --now solmu-auto-update.timer
      ;;
    Darwin)
      agents="$HOME/Library/LaunchAgents"
      mkdir -p "$agents"
      executable=$(printf '%s' "$destination/solmu-backend" | sed 's/\&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g; s/"/\&quot;/g')
      working=$(printf '%s' "$service_dir" | sed 's/\&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g; s/"/\&quot;/g')
      cat > "$agents/dev.solmu.backend.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>dev.solmu.backend</string>
<key>ProgramArguments</key><array><string>$executable</string></array>
<key>WorkingDirectory</key><string>$working</string>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><true/>
<key>ThrottleInterval</key><integer>3</integer>
<key>StandardOutPath</key><string>$working/backend.log</string>
<key>StandardErrorPath</key><string>$working/backend-error.log</string>
      </dict></plist>
EOF
      if [ ! -e "$agents/dev.solmu.autoupdate.plist" ]; then
      cat > "$agents/dev.solmu.autoupdate.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>dev.solmu.autoupdate</string>
<key>ProgramArguments</key><array><string>/bin/sh</string><string>$service_dir/auto-update.sh</string></array>
<key>WorkingDirectory</key><string>$service_dir</string>
<key>RunAtLoad</key><false/>
<key>StartInterval</key><integer>86400</integer>
</dict></plist>
EOF
      launchctl bootstrap "gui/$(id -u)" "$agents/dev.solmu.autoupdate.plist"
      launchctl enable "gui/$(id -u)/dev.solmu.autoupdate"
      fi
      launchctl bootstrap "gui/$(id -u)" "$agents/dev.solmu.backend.plist"
      launchctl enable "gui/$(id -u)/dev.solmu.backend"
      launchctl kickstart "gui/$(id -u)/dev.solmu.backend"
      ;;
  esac
  printf 'Backend runs in the background. Set your provider key in %s/.env and restart the service.\n' "$service_dir"
}
