#!/usr/bin/env bash
# Export a cyg-gaming database to a timestamped gzip with a sha256 sidecar.
#
# SURREAL_ENDPOINT  http(s):// server (default http://127.0.0.1:8000) or a
#                   surrealkv:// path. A path endpoint requires the `surreal`
#                   CLI and a STOPPED server — the store cannot be opened twice.
# SURREAL_NS / SURREAL_DB / SURREAL_USER / SURREAL_PASS  coordinates.
# $1  output directory (default: backups)
set -euo pipefail

ENDPOINT="${SURREAL_ENDPOINT:-http://127.0.0.1:8000}"
NS="${SURREAL_NS:-cyg}"
DB="${SURREAL_DB:-cyg}"
OUT_DIR="${1:-backups}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
ARCHIVE="$OUT_DIR/cyg-backup-$STAMP.sql.gz"
PART="$ARCHIVE.part"

mkdir -p "$OUT_DIR"
# A half-written export must not survive as something a later restore would
# accept, so the archive only appears under its final name once it is complete.
trap 'rm -f "$PART"' EXIT

if [[ "$ENDPOINT" == http://* || "$ENDPOINT" == https://* ]]; then
	AUTH=()
	[[ -n "${SURREAL_USER:-}" ]] && AUTH=(-u "${SURREAL_USER}:${SURREAL_PASS:-}")
	curl -fsS -X POST "${AUTH[@]}" "$ENDPOINT/export?ns=$NS&db=$DB" | gzip -n >"$PART"
else
	command -v surreal >/dev/null 2>&1 || {
		echo "the surreal CLI is required to export $ENDPOINT" >&2
		exit 1
	}
	# shellcheck disable=SC2086
	surreal db export --log none --endpoint "$ENDPOINT" --ns "$NS" --db "$DB" \
		${SURREAL_USER:+--user "$SURREAL_USER"} ${SURREAL_PASS:+--pass "$SURREAL_PASS"} - |
		gzip -n >"$PART"
fi

mv "$PART" "$ARCHIVE"
(cd "$OUT_DIR" && sha256sum "$(basename "$ARCHIVE")" >"$(basename "$ARCHIVE").sha256")
echo "$ARCHIVE"
