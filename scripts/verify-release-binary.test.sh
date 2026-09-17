#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d)"
occupied_pid=""
cleanup() {
  if [[ -n $occupied_pid ]]; then
    kill "$occupied_pid" 2>/dev/null || true
    wait "$occupied_pid" 2>/dev/null || true
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT

fake_binary="$scratch/failing-lific"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'case " $* " in' \
  '  *" --version "*) echo "lific test" ;;' \
  '  *" --help "*) exit 0 ;;' \
  '  *" start "*) exit 1 ;;' \
  '  *) exit 0 ;;' \
  'esac' >"$fake_binary"
chmod +x "$fake_binary"

port="$((35000 + RANDOM % 1000))"
PORT="$port" bun -e 'Bun.serve({ port: Number(process.env.PORT), fetch: () => new Response("<html>occupied</html>", { headers: { "content-type": "text/html" } }) });' &
occupied_pid=$!

for _ in {1..20}; do
  if curl --fail --silent --max-time 1 "http://127.0.0.1:$port/" >/dev/null; then
    break
  fi
  sleep 0.1
done
curl --fail --silent --max-time 1 "http://127.0.0.1:$port/" >/dev/null

if LIFIC_VERIFY_PORT="$port" bash "$script_dir/verify-release-binary.sh" "$fake_binary"; then
  echo "verifier accepted an occupied port" >&2
  exit 1
fi

echo "occupied port with failing child is rejected"
