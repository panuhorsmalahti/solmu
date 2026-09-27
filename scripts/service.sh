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
  if [ ! -e "$service_dir/.env" ]; then
    (umask 077; printf 'LLM_PROVIDER=openai\nOPENAI_API_KEY=\nSOLMU_WEB_DIR=web\n# LLM_MODEL=gpt-6-sol\n' > "$service_dir/.env")
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
      launchctl bootstrap "gui/$(id -u)" "$agents/dev.solmu.backend.plist"
      launchctl enable "gui/$(id -u)/dev.solmu.backend"
      launchctl kickstart "gui/$(id -u)/dev.solmu.backend"
      ;;
  esac
  printf 'Backend runs in the background. Set your provider key in %s/.env and restart the service.\n' "$service_dir"
}
