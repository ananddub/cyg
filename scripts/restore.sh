#!/usr/bin/env bash
# Restore an archive produced by backup.sh.
#
#   ./scripts/restore.sh backups/cyg-backup-<stamp>.sql.gz [--force]
#
# An import is applied statement by statement with no rollback, so the safest
# recovery is to import into a fresh namespace/database and point the server at
# it (see docs/BACKUP.md). Without --force this refuses a target that already
# holds user rows.
set -euo pipefail

ENDPOINT="${SURREAL_ENDPOINT:-http://127.0.0.1:8000}"
NS="${SURREAL_NS:-cyg}"
DB="${SURREAL_DB:-cyg}"
FORCE=0
ARCHIVE=""
for arg in "$@"; do
	case "$arg" in
		--force) FORCE=1 ;;
		*) ARCHIVE="$arg" ;;
	esac
done
[[ -n "$ARCHIVE" && -f "$ARCHIVE" ]] || {
	echo "usage: restore.sh <archive.sql.gz> [--force]" >&2
	exit 1
}

if [[ -f "$ARCHIVE.sha256" ]]; then
	(cd "$(dirname "$ARCHIVE")" && sha256sum -c "$(basename "$ARCHIVE").sha256")
fi

SQL="$(mktemp)"
trap 'rm -f "$SQL"' EXIT
gunzip -c "$ARCHIVE" >"$SQL"

if [[ "$ENDPOINT" == http://* || "$ENDPOINT" == https://* ]]; then
	AUTH=()
	[[ -n "${SURREAL_USER:-}" ]] && AUTH=(-u "${SURREAL_USER}:${SURREAL_PASS:-}")
	if [[ "$FORCE" == 0 ]]; then
		ROWS="$(curl -fsS -X POST "${AUTH[@]}" -H 'Accept: application/json' \
			--data 'SELECT VALUE id FROM users LIMIT 1;' "$ENDPOINT/sql?ns=$NS&db=$DB")"
		echo "$ROWS" | grep -Eq '"result":[[:space:]]*\[\]' || {
			echo "$NS/$DB already holds user rows; pass --force to import anyway" >&2
			exit 1
		}
	fi
	curl -fsS -X POST "${AUTH[@]}" --data-binary "@$SQL" "$ENDPOINT/import?ns=$NS&db=$DB" >/dev/null
else
	command -v surreal >/dev/null 2>&1 || {
		echo "the surreal CLI is required to import into $ENDPOINT" >&2
		exit 1
	}
	PATH_ARG="${ENDPOINT#*://}"
	if [[ "$FORCE" == 0 && -e "$PATH_ARG" && -n "$PATH_ARG" ]]; then
		echo "$PATH_ARG already exists; pass --force to import anyway" >&2
		exit 1
	fi
	# shellcheck disable=SC2086
	surreal db import --log none --endpoint "$ENDPOINT" --ns "$NS" --db "$DB" \
		${SURREAL_USER:+--user "$SURREAL_USER"} ${SURREAL_PASS:+--pass "$SURREAL_PASS"} "$SQL"
fi

echo "restored $ARCHIVE into $ENDPOINT ($NS/$DB)"
