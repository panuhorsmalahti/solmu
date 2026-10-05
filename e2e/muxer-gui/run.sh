#!/usr/bin/env bash
set -euo pipefail

root=$(pwd)
state=$(mktemp -d)
gui_pid=''
cleanup() {
  if [ -n "$gui_pid" ]; then kill "$gui_pid" 2>/dev/null || true; fi
  rm -rf "$state"
}
trap cleanup EXIT

export SOLMU_MUXER_DIR="$state/sessions"
export SOLMU_MUXER_CONFIG="$state/config.toml"
export SOLMU_CLI_PATH="$root/target/debug/solmu"
export PATH="$root/target/debug:$PATH"
export SOLMU_MUXER_PATH="$state/no-muxer-binary"
cat > "$SOLMU_MUXER_CONFIG" <<'EOF'
[terminal]
new_pane = "shell"
EOF

# Launch the GUI with no Muxer executable or daemon. Its embedded engine should
# create and persist the default workspace itself.
target/debug/muxer-gui &
gui_pid=$!
window=''
for _ in $(seq 1 90); do
  window=$(xdotool search --name '^Muxer GUI$' 2>/dev/null | head -n 1 || true)
  [ -n "$window" ] && break
  sleep 1
done
[ -n "$window" ] || { echo 'Muxer GUI window did not appear' >&2; exit 1; }
snapshot="$SOLMU_MUXER_DIR/default.json"
for _ in $(seq 1 30); do [ -s "$snapshot" ] && break; sleep 0.5; done
[ -s "$snapshot" ] || { echo 'Embedded GUI did not create a persisted session' >&2; exit 1; }
python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
assert data["spaces"], "default workspace was not created"
assert data["panes"], "default terminal pane was not created"
PY
mkdir -p docs/screenshots

# Send a shell command through the GUI and verify its file side effect.
marker="$state/gui-command.txt"
xdotool windowfocus --sync "$window"
eval "$(xdotool getwindowgeometry --shell "$window")"
xdotool mousemove --window "$window" "$((WIDTH / 2))" "$((HEIGHT - 75))" click 1
xdotool type --clearmodifiers "echo SOLMU_MUXER_GUI_E2E > '$marker'"
xdotool key Return
for _ in $(seq 1 30); do [ -s "$marker" ] && break; sleep 0.5; done
[ -s "$marker" ] && grep -q SOLMU_MUXER_GUI_E2E "$marker" || { echo 'GUI input did not reach its terminal pane' >&2; exit 1; }

# Create a Terminal space and confirm it is present in the saved shared state.
xdotool mousemove --window "$window" 190 210 click 1
sleep 0.3
xdotool mousemove --window "$window" 165 272 click 1
sleep 0.3
xdotool mousemove --window "$window" 140 160 click 1
created=''
for _ in $(seq 1 30); do
  created=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = next((item for item in data["spaces"] if item["id"] == data["active"]), {})
print(space.get("kind", "empty"))
PY
)
  [ "$created" = terminal ] && break
  sleep 0.5
done
[ "$created" = terminal ] || { echo "GUI did not persist a Terminal space (active=$created)" >&2; exit 1; }
sleep 1
import -window "$window" docs/screenshots/muxer-gui.png

# The embedded GUI can create Solmu spaces too, without a Muxer server.
xdotool mousemove --window "$window" 190 210 click 1
sleep 0.3
xdotool mousemove --window "$window" 165 241 click 1
sleep 0.3
xdotool mousemove --window "$window" 140 160 click 1
created=''
for _ in $(seq 1 30); do
  created=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = next((item for item in data["spaces"] if item["id"] == data["active"]), {})
print(space.get("kind", "empty"))
PY
)
  [ "$created" = solmu ] && break
  sleep 0.5
done
[ "$created" = solmu ] || { echo "GUI did not persist a Solmu space (active=$created)" >&2; exit 1; }

test -s docs/screenshots/muxer-gui.png
