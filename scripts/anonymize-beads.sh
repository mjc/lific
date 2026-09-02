#!/usr/bin/env bash
set -euo pipefail

: "${LIFIC_BEADS_ANON_INPUT:?set LIFIC_BEADS_ANON_INPUT to an exported issues.jsonl}"
: "${LIFIC_BEADS_ANON_OUTPUT:?set LIFIC_BEADS_ANON_OUTPUT to the sanitized JSONL destination}"

cargo test --quiet \
  import::beads::tests::anonymizer_file_is_atomic_and_can_run_from_explicit_environment_paths \
  -- --exact
