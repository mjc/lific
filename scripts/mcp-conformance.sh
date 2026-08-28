#!/usr/bin/env bash
set -euo pipefail

# Keep this pinned with the July 2026 requirements selection. The requirements
# flag is important: a hand-picked scenario list can silently omit a new
# mandatory check when the conformance package grows.
CONFORMANCE_PACKAGE='@modelcontextprotocol/conformance@0.2.0-alpha.11'
SPEC_VERSION='2026-07-28'
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BASELINE="$SCRIPT_DIR/mcp-conformance-baseline.yml"

if (($# != 1)); then
    echo "usage: $0 http://127.0.0.1:3456/mcp[/path-token]" >&2
    echo "authenticated /mcp: set MCP_CONFORMANCE_BEARER_TOKEN_ENV to the name of an exported token variable" >&2
    exit 2
fi

SERVER_URL="$1"
if [[ -n ${MCP_CONFORMANCE_BEARER_TOKEN_ENV:-} ]]; then
    BEARER_ENV="$MCP_CONFORMANCE_BEARER_TOKEN_ENV"
    if [[ ! $BEARER_ENV =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
        echo "MCP_CONFORMANCE_BEARER_TOKEN_ENV must name a shell environment variable" >&2
        exit 2
    fi
    BEARER_TOKEN="${!BEARER_ENV:-}"
    if [[ -z $BEARER_TOKEN ]]; then
        echo "MCP_CONFORMANCE_BEARER_TOKEN_ENV names an unset or empty variable" >&2
        exit 2
    fi

    export MCP_CONFORMANCE_TARGET_URL="$SERVER_URL"
    export MCP_CONFORMANCE_BEARER_TOKEN="$BEARER_TOKEN"
    coproc AUTH_PROXY { exec node "$SCRIPT_DIR/mcp-conformance-auth-proxy.mjs"; }
    AUTH_PROXY_PID_VALUE="$AUTH_PROXY_PID"
    unset MCP_CONFORMANCE_TARGET_URL MCP_CONFORMANCE_BEARER_TOKEN BEARER_TOKEN
    cleanup_proxy() {
        kill "$AUTH_PROXY_PID_VALUE" 2>/dev/null || true
        wait "$AUTH_PROXY_PID_VALUE" 2>/dev/null || true
    }
    trap cleanup_proxy EXIT INT TERM
    if ! IFS= read -r -t 5 SERVER_URL <&"${AUTH_PROXY[0]}"; then
        echo "authenticated MCP conformance proxy failed to start" >&2
        exit 1
    fi
fi

npx --yes "$CONFORMANCE_PACKAGE" server \
    --url "$SERVER_URL" \
    --requirements "$SPEC_VERSION" \
    --expected-failures "$BASELINE" \
    --verbose
