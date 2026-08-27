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
    echo "usage: $0 http://127.0.0.1:3456/mcp" >&2
    exit 2
fi

npx --yes "$CONFORMANCE_PACKAGE" server \
    --url "$1" \
    --requirements "$SPEC_VERSION" \
    --expected-failures "$BASELINE" \
    --verbose
