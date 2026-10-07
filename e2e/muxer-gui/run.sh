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
cat > "$SOLMU_MUXER_CONFIG" <<EOF
[terminal]
new_pane = "shell"
[worktrees]
directory = "$state/worktrees"
EOF

source="$state/source"
mkdir -p "$source"
git init -q "$source"
printf 'Muxer GUI worktree fixture\n' > "$source/README.md"
git -C "$source" add README.md
git -C "$source" -c user.name='Solmu E2E' -c user.email=e2e@example.invalid commit -qm initial

# Launch the GUI with no Muxer executable or daemon. Its embedded engine should
# create and persist the default workspace itself.
(cd "$source" && "$root/target/debug/muxer-gui") &
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
first_space=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print(data["spaces"][0]["id"])
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
xdotool mousemove --window "$window" 420 38 click 3
sleep 0.3
import -window "$window" docs/screenshots/muxer-gui-tab-context-menu.png
xdotool mousemove --window "$window" 450 58 click 1
for _ in $(seq 1 30); do
  count=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = data["spaces"][0]
print(f'{len(space["tabs"])}:{space["selected"]}:{space["id"]}')
PY
)
  [ "$count" = "1:$first_tab:$first_space" ] && break
  sleep 0.3
done
[ "$count" = "1:$first_tab:$first_space" ] || { echo "Closing a tab did not select the previous tab in the same space (state=$count)" >&2; exit 1; }

# Right-click the focused pane for layout actions, then close the split pane.
xdotool mousemove --window "$window" 500 300
sleep 0.2
xdotool click 3
sleep 0.3
import -window "$window" docs/screenshots/muxer-gui-pane-context-menu.png
xdotool mousemove --window "$window" 550 320
sleep 0.2
xdotool click 1
for _ in $(seq 1 30); do
  pane_count=$(python3 - "$snapshot" <<'PY'
import json, sys
print(len(json.load(open(sys.argv[1]))["panes"]))
PY
)
  [ "$pane_count" = 2 ] && break
  sleep 0.3
done
[ "$pane_count" = 2 ] || { echo 'Pane context menu did not split the terminal right' >&2; exit 1; }
sleep 0.3
xdotool mousemove --window "$window" 500 300
sleep 0.2
xdotool click 3
sleep 0.3
xdotool mousemove --window "$window" 550 388
sleep 0.2
xdotool click 1
for _ in $(seq 1 30); do
  pane_count=$(python3 - "$snapshot" <<'PY'
import json, sys
print(len(json.load(open(sys.argv[1]))["panes"]))
PY
)
  [ "$pane_count" = 1 ] && break
  sleep 0.3
done
[ "$pane_count" = 1 ] || { echo 'Pane context menu did not close the focused pane' >&2; exit 1; }

# Focus the terminal surface, type like a normal terminal, and verify the shell
# command's file side effect.
marker="$state/gui-command.txt"
xdotool windowfocus --sync "$window"
eval "$(xdotool getwindowgeometry --shell "$window")"
xdotool mousemove --window "$window" "$((WIDTH / 2))" "$((HEIGHT / 2))" click 1
sleep 0.2
xdotool type --clearmodifiers "echo SOLMU_MUXER_GUI_E2E > '$marker'"
xdotool key Return
for _ in $(seq 1 30); do [ -s "$marker" ] && break; sleep 0.5; done
[ -s "$marker" ] && grep -q SOLMU_MUXER_GUI_E2E "$marker" || { echo 'GUI input did not reach its terminal pane' >&2; exit 1; }
import -window "$window" docs/screenshots/muxer-gui.png
single_pane_color=$(convert docs/screenshots/muxer-gui.png -format '%[pixel:p{100,310}]' info:)
[ "$single_pane_color" != 'srgb(224,233,223)' ] || { echo 'A single pane should not appear as a selectable sidebar item' >&2; exit 1; }

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
xdotool mousemove --window "$window" 240 131 click 1
sleep 0.3
import -window "$window" docs/screenshots/muxer-gui.png
import -window "$window" docs/screenshots/muxer-gui-worktrees.png

# Create a Git worktree from the selected project through the + menu.
xdotool mousemove --window "$window" 95 232 click 1
xdotool type --clearmodifiers 'gui-e2e-worktree'
xdotool key Return
worktree=''
for _ in $(seq 1 30); do
  worktree=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
space = next((item for item in data["spaces"] if (item.get("worktree") or {}).get("branch") == "gui-e2e-worktree"), {})
print(f'{len(data["spaces"])}:{space.get("kind", "empty")}:{(space.get("worktree") or {}).get("primary", "empty")}')
PY
)
  [ "$worktree" = "$((before + 1)):terminal:False" ] && break
  sleep 0.5
done
[ "$worktree" = "$((before + 1)):terminal:False" ] || { echo "GUI did not create a worktree space (state=$worktree)" >&2; exit 1; }
before=$((before + 1))
worktree_space=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print(next(item["id"] for item in data["spaces"] if (item.get("worktree") or {}).get("branch") == "gui-e2e-worktree"))
PY
)
xdotool mousemove --window "$window" 240 131 click 1
sleep 0.3
xdotool mousemove --window "$window" 95 232 click 1
xdotool type --clearmodifiers 'gui-e2e-worktree'
xdotool mousemove --window "$window" 90 318 click 1
for _ in $(seq 1 30); do
  opened=$(python3 - "$snapshot" "$worktree_space" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
matches = [item for item in data["spaces"] if (item.get("worktree") or {}).get("branch") == "gui-e2e-worktree"]
print(f'{len(data["spaces"])}:{len(matches)}:{data["active"]}:{sys.argv[2]}')
PY
)
  [ "$opened" = "$before:1:$worktree_space:$worktree_space" ] && break
  sleep 0.5
done
[ "$opened" = "$before:1:$worktree_space:$worktree_space" ] || { echo "GUI did not reopen the existing worktree space (state=$opened)" >&2; exit 1; }
# A space context menu can remove the checkout while preserving its branch.
worktree_path="$state/worktrees/source/gui-e2e-worktree"
xdotool mousemove --window "$window" 100 245 click 3
sleep 0.3
import -window "$window" docs/screenshots/muxer-gui-remove-worktree.png
xdotool mousemove --window "$window" 150 300 click 1
for _ in $(seq 1 30); do
  removed=$(python3 - "$snapshot" "$worktree_space" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print("removed" if all(item["id"] != int(sys.argv[2]) for item in data["spaces"]) else "present")
PY
)
  [ "$removed" = removed ] && break
  sleep 0.5
done
[ "$removed" = removed ] && [ ! -e "$worktree_path" ] || { echo 'GUI did not remove the worktree checkout and its space' >&2; exit 1; }
before=$((before - 1))
xdotool mousemove --window "$window" 240 131 click 1
sleep 0.3
xdotool mousemove --window "$window" 75 168 click 1
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

# Ctrl+C closes the focused Terminal pane.
eval "$(xdotool getwindowgeometry --shell "$window")"
xdotool mousemove --window "$window" "$((WIDTH / 2))" "$((HEIGHT / 2))" click 1
sleep 0.2
xdotool key ctrl+c
remaining_spaces=''
for _ in $(seq 1 30); do
  remaining_spaces=$(python3 - "$snapshot" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print(len(data["spaces"]))
PY
)
  [ "$remaining_spaces" = "$before" ] && break
  sleep 0.3
done
[ "$remaining_spaces" = "$before" ] || { echo 'Ctrl+C did not close the focused Terminal pane' >&2; exit 1; }

# The embedded GUI can create Solmu spaces too, without a Muxer server.
xdotool mousemove --window "$window" 240 131 click 1
sleep 0.3
xdotool mousemove --window "$window" 75 200 click 1
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
sleep 2
# Capture the embedded desktop view, where management is available from the
# chat command menu instead of a row of extra header buttons.
import -window "$window" docs/screenshots/muxer-gui.png

# The Spaces sidebar stays visible while Solmu is active. Right-click the
# original space, delete it, and verify it is removed.
xdotool mousemove --window "$window" 100 174 click 3
sleep 0.3
xdotool mousemove --window "$window" 80 224 click 1
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
test -s docs/screenshots/muxer-gui-worktrees.png
test -s docs/screenshots/muxer-gui-remove-worktree.png
