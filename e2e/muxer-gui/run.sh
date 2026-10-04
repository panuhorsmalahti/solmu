#!/usr/bin/env bash
set -euo pipefail

root=$(pwd)
state=$(mktemp -d)
gui_pid=''
cleanup() {
  if [ -n "$gui_pid" ]; then kill "$gui_pid" 2>/dev/null || true; fi
  target/debug/muxer server stop --session default >/dev/null 2>&1 || true
  rm -rf "$state"
}
trap cleanup EXIT

export SOLMU_MUXER_DIR="$state/sessions"
export SOLMU_MUXER_CONFIG="$state/config.toml"
cat > "$SOLMU_MUXER_CONFIG" <<'EOF'
[terminal]
new_pane = "shell"
EOF

target/debug/muxer server start --session default --cwd "$root"
pane=''
for _ in $(seq 1 30); do
  pane=$(target/debug/muxer status --session default --json | python3 -c 'import json,sys; panes=json.load(sys.stdin)["result"].get("panes", []); print(panes[0]["id"] if panes else "")')
  [ -n "$pane" ] && break
  sleep 1
done
[ -n "$pane" ] || { echo 'Muxer shell pane did not start' >&2; exit 1; }
target/debug/muxer-gui &
gui_pid=$!
window=''
for _ in $(seq 1 90); do
  window=$(xdotool search --name '^Muxer GUI$' 2>/dev/null | head -n 1 || true)
  [ -n "$window" ] && break
  sleep 1
done
[ -n "$window" ] || { echo 'Muxer GUI window did not appear' >&2; exit 1; }
mkdir -p docs/screenshots
sleep 2
import -window "$window" docs/screenshots/muxer-gui.png

# Send a command through the GUI input and verify it reaches the real shell pane.
xdotool windowfocus --sync "$window"
xdotool getwindowgeometry --shell "$window"
xdotool mousemove --window "$window" 650 720 click 1
xdotool type --clearmodifiers 'echo SOLMU_MUXER_GUI_E2E'
xdotool key Return
seen=''
for _ in $(seq 1 30); do
  seen=$(target/debug/muxer pane read "$pane" --session default 2>/dev/null || true)
  [[ "$seen" == *SOLMU_MUXER_GUI_E2E* ]] && break
  sleep 0.5
done
[[ "$seen" == *SOLMU_MUXER_GUI_E2E* ]] || { echo 'GUI input did not reach the selected Muxer pane' >&2; exit 1; }

sleep 1
mkdir -p docs/screenshots
import -window "$window" docs/screenshots/muxer-gui.png
test -s docs/screenshots/muxer-gui.png
