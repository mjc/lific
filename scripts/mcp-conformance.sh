#!/usr/bin/env bash
set -euo pipefail

# Keep this pinned: the July 2026 scenarios are not available from the
# currently published `latest` conformance tag.
CONFORMANCE_PACKAGE='@modelcontextprotocol/conformance@0.2.0-alpha.11'
SPEC_VERSION='2026-07-28'
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BASELINE="$SCRIPT_DIR/mcp-conformance-baseline.yml"

if (($# != 1)); then
    echo "usage: $0 http://127.0.0.1:3456/mcp" >&2
    exit 2
fi

scenarios=(
    server-stateless
    completion-complete
    tools-list
    server-sse-multiple-streams
    dns-rebinding-protection
    http-header-validation
)

for scenario in "${scenarios[@]}"; do
    npx --yes "$CONFORMANCE_PACKAGE" server \
        --url "$1" \
        --scenario "$scenario" \
        --spec-version "$SPEC_VERSION" \
        --expected-failures "$BASELINE" \
        --verbose
done
