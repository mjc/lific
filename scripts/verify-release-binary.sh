#!/usr/bin/env bash
set -euo pipefail

binary="${1:?usage: verify-release-binary.sh PATH_TO_BINARY}"
if [[ $binary != /* ]]; then
  binary="./$binary"
fi

"$binary" --version
"$binary" --help >/dev/null

scratch="$(mktemp -d)"
server_pid=""
cleanup() {
  if [[ -n $server_pid ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT

port="${LIFIC_VERIFY_PORT:-$((34567 + RANDOM % 1000))}"
touch "$scratch/lific.toml"

# An override is useful for CI and local debugging, but never let an existing
# HTTP server make a failed artifact look healthy.
if curl --connect-timeout 1 --max-time 2 --silent --output /dev/null "http://127.0.0.1:$port/"; then
  echo "verification port $port is already serving HTTP" >&2
  exit 1
fi

LIFIC_INIT_ADMIN_NAME=Release \
  LIFIC_INIT_ADMIN_PASSWORD=release-smoke-password-123 \
  "$binary" \
  --config "$scratch/lific.toml" \
  --db "$scratch/lific.db" \
  start --init-if-missing --host 127.0.0.1 --port "$port" \
  >"$scratch/server.log" 2>&1 &
server_pid=$!

started=false
for _ in {1..60}; do
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$scratch/server.log" >&2
    exit 1
  fi

  # The production server logs this only after TcpListener::bind succeeds.
  # Waiting for that event ties the HTTP probe to this process rather than
  # accepting the first response from an unrelated listener.
  if grep -q 'lific server started' "$scratch/server.log"; then
    started=true
    if curl --fail --silent "http://127.0.0.1:$port/" >"$scratch/index.html"; then
      break
    fi
  fi
  sleep 1
done

if [[ $started != true ]] || ! kill -0 "$server_pid" 2>/dev/null; then
  cat "$scratch/server.log" >&2
  exit 1
fi

test -s "$scratch/index.html"
grep -qi '<html' "$scratch/index.html"
