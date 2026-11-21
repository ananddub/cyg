#!/usr/bin/env bash
# Run the Verus proofs for the engine's verified-mirror crate.
#
#   ./scripts/verify-verus.sh
#
# Requires the Verus toolchain on PATH (see ~/.verus and ~/.cargo/bin/verus).
# Exit code 0 = every proof passed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="$ROOT/verus/ox-verified/src/proofs/verify.rs"

if ! command -v verus >/dev/null 2>&1; then
    echo "error: verus not on PATH (expected in ~/.cargo/bin)" >&2
    exit 2
fi

echo "verifying: verus/ox-verified (all engine mirrors)"
cd "$(dirname "$HARNESS")"
exec verus --cfg verus --crate-type lib verify.rs --multiple-errors 20 --rlimit 240 "$@"
