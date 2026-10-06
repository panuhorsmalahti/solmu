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
sleep 2
mkdir -p docs/screenshots

# The top strip acts like browser tabs: create, switch, then close a tab.
first_tab=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = data["spaces"][0]
print(space["tabs"][0]["id"])
PY
)
xdotool mousemove --window "$window" 390 38 click 1
for _ in $(seq 1 30); do
  count=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = data["spaces"][0]
print(len(space["tabs"]))
PY
)
  [ "$count" = 2 ] && break
  sleep 0.3
done
[ "$count" = 2 ] || { echo 'Top tab strip did not create a tab' >&2; exit 1; }
second_tab=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = data["spaces"][0]
print(space["tabs"][1]["id"])
PY
)
xdotool mousemove --window "$window" 335 38 click 1
sleep 0.3
active_tab=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = data["spaces"][0]
print(space["selected"])
PY
)
[ "$active_tab" = "$first_tab" ] || { echo 'Top tab strip did not switch tabs' >&2; exit 1; }
xdotool mousemove --window "$window" 420 38 click 1
sleep 0.3
active_tab=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = data["spaces"][0]
print(space["selected"])
PY
)
[ "$active_tab" = "$second_tab" ] || { echo 'Top tab strip did not focus the new tab' >&2; exit 1; }
xdotool mousemove --window "$window" 470 38 click 1
for _ in $(seq 1 30); do
  count=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = data["spaces"][0]
print(len(space["tabs"]))
PY
)
  [ "$count" = 1 ] && break
  sleep 0.3
done
[ "$count" = 1 ] || { echo 'Top tab strip did not close the selected tab' >&2; exit 1; }

# Send a shell command through the GUI and verify its file side effect.
marker="$state/gui-command.txt"
xdotool windowfocus --sync "$window"
eval "$(xdotool getwindowgeometry --shell "$window")"
xdotool mousemove --window "$window" "$((WIDTH / 2))" "$((HEIGHT - 45))" click 1
xdotool type --clearmodifiers "echo SOLMU_MUXER_GUI_E2E > '$marker'"
xdotool key Return
for _ in $(seq 1 30); do [ -s "$marker" ] && break; sleep 0.5; done
[ -s "$marker" ] && grep -q SOLMU_MUXER_GUI_E2E "$marker" || { echo 'GUI input did not reach its terminal pane' >&2; exit 1; }

# Open the + menu, capture its choices, and select Terminal.
initial_space=$(python3 - "$snapshot" <<'PY'
import json, sys
print(json.load(open(sys.argv[1]))["spaces"][0]["id"])
PY
)
before=$(python3 - "$snapshot" <<'PY'
import json, sys
print(len(json.load(open(sys.argv[1]))["spaces"]))
PY
)
xdotool mousemove --window "$window" 240 210 click 1
sleep 0.3
import -window "$window" docs/screenshots/muxer-gui.png
xdotool mousemove --window "$window" 75 242 click 1
sleep 0.3
created=''
for _ in $(seq 1 30); do
  created=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = next((item for item in data["spaces"] if item["id"] == data["active"]), {})
pane = next((item for item in data["panes"] if item["id"] == data["active"]), {})
print("{}:{}:{}".format(len(data["spaces"]), space.get("kind", "empty"), pane.get("launch", {}).get("kind", "empty")))
PY
)
  [ "$created" = "$((before + 1)):terminal:shell" ] && break
  sleep 0.5
done
[ "$created" = "$((before + 1)):terminal:shell" ] || { echo "Terminal choice did not open a shell (active=$created)" >&2; exit 1; }

# The embedded GUI can create Solmu spaces too, without a Muxer server.
xdotool mousemove --window "$window" 240 210 click 1
sleep 0.3
xdotool mousemove --window "$window" 75 274 click 1
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

# Right-click a space, choose Delete space, and verify it is removed.
xdotool mousemove --window "$window" 100 254 click 3
sleep 0.3
xdotool mousemove --window "$window" 80 310 click 1
removed=''
for _ in $(seq 1 30); do
  removed=$(python3 - "$snapshot" "$initial_space" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print("removed" if all(space["id"] != int(sys.argv[2]) for space in data["spaces"]) else "present")
PY
)
  [ "$removed" = removed ] && break
  sleep 0.5
done
[ "$removed" = removed ] || { echo 'Right-click Delete space did not remove the selected workspace' >&2; exit 1; }

test -s docs/screenshots/muxer-gui.png
