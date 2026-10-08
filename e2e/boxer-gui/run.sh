#!/usr/bin/env bash
set -euo pipefail

root=$(pwd)
state=$(mktemp -d)
gui_pid=''
launch_pid=''
launch_id=''
gui_launch_id=''
cleanup() {
  if [ -n "$launch_pid" ]; then "$root/target/debug/boxer" stop "$launch_id" --force >/dev/null 2>&1 || true; wait "$launch_pid" 2>/dev/null || true; fi
  if [ -n "$gui_launch_id" ]; then "$root/target/debug/boxer" stop "$gui_launch_id" --force >/dev/null 2>&1 || true; fi
  if [ -n "$gui_pid" ]; then kill "$gui_pid" 2>/dev/null || true; fi
  rm -rf "$state"
}
trap cleanup EXIT

export BOXER_SESSIONS_DIR="$state/sessions"
(cd "$root" && "$root/target/debug/boxer-gui") &
gui_pid=$!
window=''
for _ in $(seq 1 60); do
  window=$(xdotool search --name '^Boxer$' 2>/dev/null | head -n 1 || true)
  [ -n "$window" ] && break
  sleep 0.5
done
[ -n "$window" ] || { echo 'Boxer GUI window did not appear' >&2; exit 1; }
mkdir -p docs/screenshots
sleep 1
import -window "$window" "$state/empty.png"

# Launch from the CLI while Boxer GUI is open. The directory watcher must make
# the new record appear in the rendered list without a timer-based refresh.
"$root/target/debug/boxer" --detached --cwd "$root" -- /bin/sh -c 'sleep 30' >/dev/null 2>&1 &
launch_pid=$!
launch_id=''
for _ in $(seq 1 40); do
  launch_id=$("$root/target/debug/boxer" ps --json | python3 -c 'import json,sys; sessions=json.load(sys.stdin); print(sessions[0]["id"] if sessions else "")')
  [ -n "$launch_id" ] && break
  sleep 0.1
done
[ -n "$launch_id" ] || { echo 'CLI Boxer launch did not create a session record' >&2; exit 1; }
sleep 1
import -window "$window" docs/screenshots/boxer-gui.png
cmp -s "$state/empty.png" docs/screenshots/boxer-gui.png && { echo 'Boxer GUI did not react to the session record change' >&2; exit 1; }

"$root/target/debug/boxer" stop "$launch_id" --force >/dev/null
launch_pid=''
sleep 1
import -window "$window" "$state/stopped.png"
cmp -s "$state/empty.png" "$state/stopped.png" && { echo 'Boxer GUI did not react to the launch stopping' >&2; exit 1; }

# Launch and stop a process from the GUI itself.
xdotool windowfocus --sync "$window"
xdotool mousemove --sync --window "$window" 450 198
sleep 0.2
xdotool click 1
sleep 0.3
xdotool key --clearmodifiers ctrl+a
xdotool key BackSpace
xdotool type --delay 40 --clearmodifiers '/bin/cat'
sleep 0.3
xdotool mousemove --window "$window" 900 200 click 1
for _ in $(seq 1 40); do
  gui_launch_id=$("$root/target/debug/boxer" ps --json | python3 -c 'import json,sys; sessions=json.load(sys.stdin); print(next((item["id"] for item in sessions if item["command"] == "/bin/cat"), ""))')
  [ -n "$gui_launch_id" ] && break
  sleep 0.1
done
if [ -z "$gui_launch_id" ]; then
  import -window "$window" docs/screenshots/boxer-gui.png
  echo 'Boxer GUI did not launch the selected program' >&2
  exit 1
fi
sleep 0.5
xdotool mousemove --window "$window" 900 368 click 1
for _ in $(seq 1 40); do
  stopped=$("$root/target/debug/boxer" ps --all --json | python3 -c 'import json,sys; sessions=json.load(sys.stdin); item=next((item for item in sessions if item["id"] == sys.argv[1]), {}); print(item.get("status", "missing"))' "$gui_launch_id")
  [[ "$stopped" == stopped || "$stopped" == finished ]] && break
  sleep 0.1
done
if [[ "$stopped" != stopped && "$stopped" != finished ]]; then
  import -window "$window" docs/screenshots/boxer-gui.png
  "$root/target/debug/boxer" ps --all --json >&2
  echo "Boxer GUI Stop did not stop the selected process (status=$stopped, id=$gui_launch_id)" >&2
  exit 1
fi
gui_launch_id=''
test -s docs/screenshots/boxer-gui.png
