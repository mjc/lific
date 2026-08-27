#!/usr/bin/env bash
set -euo pipefail

# Keep this pinned: the July 2026 scenarios are not available from the
# currently published `latest` conformance tag.
CONFORMANCE_PACKAGE='@modelcontextprotocol/conformance@0.2.0-alpha.9'
SPEC_VERSION='2026-07-28'

if (($# != 1)); then
    echo "usage: $0 http://127.0.0.1:3456/mcp" >&2
    exit 2
fi

exec npx --yes "$CONFORMANCE_PACKAGE" server \
    --url "$1" \
    --suite all \
    --spec-version "$SPEC_VERSION" \
    --verbose
