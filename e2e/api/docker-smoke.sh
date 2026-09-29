#!/bin/sh
set -eu

image=${1:?Pass the Solmu backend image}
container=solmu-docker-e2e
base=http://127.0.0.1:3000/api/v1
docker run --detach --name "$container" -p 127.0.0.1:3000:3000 -p 127.0.0.1:3001:3001 "$image" >/dev/null
trap 'docker rm --force "$container" >/dev/null' EXIT HUP INT TERM

wait_for_backend() {
  attempt=0
  until curl --fail --silent "$base/threads" >/dev/null; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 30 ]; then docker logs "$container"; return 1; fi
    sleep 1
  done
}
wait_for_backend
curl --fail --silent --show-error http://127.0.0.1:3000/ | grep -q 'id="root"'
curl --fail --silent --show-error http://127.0.0.1:3000/profile | grep -q 'id="root"'
thread=$(curl --fail --silent --show-error -H 'Content-Type: application/json' \
  -d '{"title":"Docker conversation"}' "$base/threads")
id=$(printf '%s' "$thread" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
curl --fail --silent --show-error -H 'Content-Type: application/json' \
  -d '{"content":"Saved inside Docker"}' "$base/threads/$id/messages" >/dev/null
docker restart "$container" >/dev/null
wait_for_backend
curl --fail --silent --show-error "$base/threads/$id/messages" | \
  python3 -c 'import json,sys; assert json.load(sys.stdin)["items"][0]["content"] == "Saved inside Docker"'
curl --fail --silent --show-error -X PATCH -H 'Content-Type: application/json' \
  -d '{"title":"Renamed in Docker"}' "$base/threads/$id" | \
  python3 -c 'import json,sys; assert json.load(sys.stdin)["title"] == "Renamed in Docker"'
curl --fail --silent --show-error -X DELETE "$base/threads/$id" >/dev/null
curl --fail --silent --show-error "$base/threads" | \
  python3 -c 'import json,sys; assert json.load(sys.stdin)["items"] == []'
echo 'Docker conversation persistence and CRUD passed'
