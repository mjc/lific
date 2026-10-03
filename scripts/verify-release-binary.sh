#!/usr/bin/env bash
# Smoke-test a release binary: it starts on its own, serves the API, and
# carries the Topcoat runtime and frontend assets inside it.
#
# A partial bundle can return index.html while its missing assets also return
# HTML through the SPA fallback. Check the bundles as well as the document,
# from a scratch directory with an explicit config to exclude caller state.
set -euo pipefail

binary_arg="${1:?usage: verify-release-binary.sh PATH_TO_BINARY}"

# Resolve before changing directory, and without `readlink -f`, which is a GNU
# extension that macOS does not have.
binary_dir="$(cd -- "$(dirname -- "$binary_arg")" >/dev/null 2>&1 && pwd)" || {
  echo "no such directory for binary '$binary_arg'" >&2
  exit 1
}
binary="$binary_dir/$(basename -- "$binary_arg")"
if [[ ! -x $binary ]]; then
  echo "'$binary' is not an executable file" >&2
  exit 1
fi

scratch="$(mktemp -d)"
server_pid=""
cleanup() {
  if [[ -n $server_pid ]]; then
    kill "$server_pid" 2>/dev/null || true
    for _ in {1..50}; do
      if ! kill -0 "$server_pid" 2>/dev/null; then
        break
      fi
      sleep 0.1
    done
    if kill -0 "$server_pid" 2>/dev/null; then
      kill -KILL "$server_pid" 2>/dev/null || true
    fi
    wait "$server_pid" 2>/dev/null || true
  fi
  cd / || true
  rm -rf "$scratch"
}
trap cleanup EXIT

# Everything below runs here, so a binary that reads frontend assets, migrations, or a
# lific.toml out of the caller's working directory fails instead of passing on
# borrowed files.
cd "$scratch"

"$binary" --version
"$binary" --help >/dev/null

startup_timeout="${LIFIC_VERIFY_STARTUP_TIMEOUT:-60}"
if ! [[ $startup_timeout =~ ^[1-9][0-9]*$ ]]; then
  echo "LIFIC_VERIFY_STARTUP_TIMEOUT must be a positive integer" >&2
  exit 1
fi
startup_deadline=$((SECONDS + startup_timeout))

# Never pass 0 to curl: it means "no timeout", rather than an expired budget.
request_budget() {
  local remaining=$((startup_deadline - SECONDS)) limit="${1:-5}"
  if ((remaining <= 0)); then
    return 1
  fi
  if ((remaining > limit)); then
    remaining=$limit
  fi
  printf '%s\n' "$remaining"
}

port="${LIFIC_VERIFY_PORT:-}"
automatic_port=true
attempted_ports=()

choose_port() {
  local candidate budget
  for _ in {1..20}; do
    if ! budget="$(request_budget 2)"; then
      echo "server did not report a healthy start within ${startup_timeout}s" >&2
      return 1
    fi
    candidate="$((34567 + RANDOM % 1000))"
    case " ${attempted_ports[*]} " in
      *" $candidate "*) continue ;;
    esac
    if ! curl --connect-timeout 1 --max-time "$budget" --silent --output /dev/null "http://127.0.0.1:$candidate/"; then
      port="$candidate"
      return
    fi
  done
  echo "could not find an available verification port" >&2
  return 1
}

# An override is useful for CI and local debugging. For the automatic case,
# retry a collision so an unrelated local server does not make this test flaky.
if [[ -n $port ]]; then
  automatic_port=false
  budget="$(request_budget 2)"
  if curl --connect-timeout 1 --max-time "$budget" --silent --output /dev/null "http://127.0.0.1:$port/"; then
    echo "verification port $port is already serving HTTP" >&2
    exit 1
  fi
else
  choose_port
fi

config="$scratch/verify-lific.toml"
start_server() {
  attempted_ports+=("$port")
  cat >"$config" <<TOML
# Written by verify-release-binary.sh. Passed with --config so the run cannot
# inherit a project-local, user, or system lific.toml.
[server]
host = "127.0.0.1"
port = $port

[database]
path = "$scratch/lific.db"

[backup]
enabled = false
dir = "$scratch/backups"

[auth]
required = true
allow_signup = false

[log]
level = "info"
TOML

  LIFIC_INIT_ADMIN_NAME=Release \
    LIFIC_INIT_ADMIN_PASSWORD=release-smoke-password-123 \
    "$binary" \
    --config "$config" \
    start --init-if-missing --host 127.0.0.1 --port "$port" \
    >"$scratch/server.log" 2>&1 &
  server_pid=$!
}

started=false
healthy=false
startup_attempt=0
while ((startup_attempt < 20 && SECONDS < startup_deadline)); do
  startup_attempt=$((startup_attempt + 1))
  start_server
  started=false
  healthy=false
  retry=false
  while ((SECONDS < startup_deadline)); do
    if ! kill -0 "$server_pid" 2>/dev/null; then
      if [[ $automatic_port == true ]] &&
        ! grep -q 'lific server started' "$scratch/server.log" &&
        grep -Eqi 'EADDRINUSE|address already in use' "$scratch/server.log"; then
        wait "$server_pid" 2>/dev/null || true
        server_pid=""
        echo "verification port $port was claimed before startup; trying another port" >&2
        retry=true
        break
      fi
      cat "$scratch/server.log" >&2
      echo "server exited before it served a request" >&2
      exit 1
    fi

    # The production server logs this only after TcpListener::bind succeeds.
    # Waiting for that event ties the HTTP probe to this process rather than
    # accepting the first response from an unrelated listener.
    if grep -q 'lific server started' "$scratch/server.log"; then
      started=true
      if budget="$(request_budget)"; then
        if curl --fail --silent --show-error \
          --connect-timeout 1 --max-time "$budget" \
          "http://127.0.0.1:$port/api/health" >"$scratch/health.txt"; then
          healthy=true
          break
        fi
      fi
    fi
    sleep 1
  done
  if [[ $retry == true ]] && ((startup_attempt < 20 && SECONDS < startup_deadline)); then
    choose_port
    continue
  fi
  break
done

if [[ $started != true || $healthy != true ]] || ! kill -0 "$server_pid" 2>/dev/null; then
  cat "$scratch/server.log" >&2
  echo "server did not report a healthy start within ${startup_timeout}s" >&2
  exit 1
fi

if [[ ! -s $scratch/health.txt ]]; then
  echo "/api/health returned an empty body" >&2
  exit 1
fi

if ! curl --fail --silent --show-error --connect-timeout 2 --max-time 15 \
  "http://127.0.0.1:$port/" >"$scratch/index.html"; then
  cat "$scratch/server.log" >&2
  echo "the server did not serve its web root" >&2
  exit 1
fi

if [[ ! -s $scratch/index.html ]]; then
  echo "the web root is empty" >&2
  exit 1
fi
if ! grep -qi '<html' "$scratch/index.html"; then
  echo "the web root is not an HTML document" >&2
  exit 1
fi

# Every asset referenced by the Topcoat document must ship in the binary.
# Legacy Vite bundles must not remain in a production release.
if grep -Eq '(src|href)="/assets/' "$scratch/index.html"; then
  echo "the web root still references a retired frontend bundle" >&2
  exit 1
fi
assets="$(grep -Eo '/__topcoat-[A-Za-z0-9._~%+-]+\.(js|css)' "$scratch/index.html" | sort -u || true)"
js_assets="$(printf '%s\n' "$assets" | grep -E '\.js$' || true)"
css_assets="$(printf '%s\n' "$assets" | grep -E '\.css$' || true)"
if ! printf '%s\n' "$js_assets" | grep -Fxq '/__topcoat-runtime.js' || [[ -z ${css_assets//[[:space:]]/} ]]; then
  echo "the web root is missing the Topcoat runtime or stylesheet" >&2
  exit 1
fi
retired_status="$(curl --silent --show-error --connect-timeout 2 --max-time 15 \
  --output /dev/null --write-out '%{http_code}' \
  "http://127.0.0.1:$port/assets/index-retired.js")"
if [[ $retired_status != 404 ]]; then
  echo "retired frontend assets must return 404, got $retired_status" >&2
  exit 1
fi

# Fetch one asset and prove it is the real file: a 200 alone means nothing
# here, because an asset the binary does not carry falls through to the SPA
# handler, which answers 200 text/html with index.html.
check_asset() {
  local path="$1" kind="$2" body="$scratch/asset-body" content_type
  if ! content_type="$(curl --fail --silent --show-error \
    --connect-timeout 2 --max-time 15 \
    --output "$body" --write-out '%{content_type}' \
    "http://127.0.0.1:$port$path")"; then
    echo "$kind asset $path was not served" >&2
    return 1
  fi
  if [[ ! -s $body ]]; then
    echo "$kind asset $path is empty" >&2
    return 1
  fi

  content_type="$(printf '%s' "$content_type" | tr '[:upper:]' '[:lower:]')"
  case "$kind:$content_type" in
    js:*javascript* | js:*ecmascript*) ;;
    css:*css*) ;;
    manifest:application/manifest+json | manifest:application/manifest+json\;*) ;;
    png:image/png | png:image/png\;*) ;;
    *)
      echo "$kind asset $path came back as '$content_type', which means the" >&2
      echo "binary does not contain it and served the SPA fallback instead" >&2
      return 1
      ;;
  esac

  if [[ $kind == png ]]; then
    if [[ $(od -An -tx1 -N8 "$body" | tr -d '[:space:]') != 89504e470d0a1a0a ]]; then
      echo "PNG asset $path does not have a PNG signature" >&2
      return 1
    fi
    return
  fi
  if head -c 200 "$body" | grep -qi '<!doctype html\|<html'; then
    echo "$kind asset $path is an HTML document" >&2
    return 1
  fi
}

while read -r asset; do
  [[ -n $asset ]] || continue
  check_asset "$asset" js
done <<<"$js_assets"

while read -r asset; do
  [[ -n $asset ]] || continue
  check_asset "$asset" css
done <<<"$css_assets"

check_asset /manifest.webmanifest manifest
# These URLs resolve relative to the manifest so installs retain a proxy mount.
for field in id start_url scope; do
  if ! grep -Eq "\"$field\"[[:space:]]*:[[:space:]]*\"\./\"" "$scratch/asset-body"; then
    echo "install manifest $field must be ./" >&2
    exit 1
  fi
done
for icon in icon-192.png icon-512.png icon-maskable-512.png; do
  if ! grep -Eq "\"src\"[[:space:]]*:[[:space:]]*\"${icon%.png}\\.png\"" "$scratch/asset-body"; then
    echo "install manifest is missing the relative icon URL $icon" >&2
    exit 1
  fi
done
for icon in favicon.png apple-touch-icon.png icon-192.png icon-512.png icon-maskable-512.png; do
  check_asset "/$icon" png
done

echo "release binary serves its API and the embedded web UI"
