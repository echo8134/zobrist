#!/usr/bin/env bash
# Runs the end-to-end test: the real app on a virtual display that lives in a container.
# Usage: scripts/check-e2e.sh, after the view is installed and built. Exit 0 when the test passes.
set -u -o pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2

cargo build -p zobrist-app -p zobrist-cli --features webdriver --locked || exit 1

# The tag names the content of the Dockerfile, so the image is built again only when that changes.
image="zobrist-display:$(sha256sum scripts/display.Dockerfile | cut -c 1-12)"
if ! docker image inspect "$image" >/dev/null 2>&1; then
  docker build --quiet --tag "$image" - <scripts/display.Dockerfile >/dev/null || exit 1
fi

# The name holds the process ID, and Docker picks the port, so two runs at once share neither.
container="zobrist-display-$$"
trap 'docker rm --force "$container" >/dev/null 2>&1' EXIT
docker run --detach --rm --name "$container" --publish 127.0.0.1::6000 "$image" >/dev/null || exit 1

# The display server prints its number as soon as it accepts connections.
ready=""
for _ in $(seq 100); do
  ready=$(docker logs "$container" 2>/dev/null)
  [ -n "$ready" ] && break
  sleep 0.1
done
if [ -z "$ready" ]; then
  echo "check-e2e: the display in the container did not start" >&2
  exit 1
fi
port=$(docker port "$container" 6000/tcp) || exit 1
port=${port##*:}

# An X display number is its TCP port minus 6000. The WebDriver server in the app needs a port
# of its own. Docker holds the display port for this run alone, so the port at a fixed distance
# below it belongs to no other run, and it lies below the range that Docker picks from.
env --unset=WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY="127.0.0.1:$((port - 6000))" \
  TAURI_WEBDRIVER_PORT="$((port - 30000))" \
  pnpm --dir crates/app/view run e2e
