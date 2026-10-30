# Backup and recovery

Data lives in the `./data` volume (docker-compose) or the path behind
`SURREAL_ENDPOINT`. Everything is exported as SurrealQL text, so archives are
portable across storage engines and versions.

## What to run

- HTTP server (compose default):
  `SURREAL_ENDPOINT=http://127.0.0.1:8000 ./scripts/backup.sh /var/backups/cyg`
- Embedded `surrealkv://` path: same script with
  `SURREAL_ENDPOINT=surrealkv://./data`, but only while the server is **stopped**
  (the store cannot be opened twice). The CLI-less `cyg-gaming backup` command
  covers the HTTP case only.
- Restore: `./scripts/restore.sh <archive> [--force]` or
  `cyg-gaming restore <archive> [--force]`.

Every archive ships with a `.sha256` sidecar; restore refuses a target that
already holds user rows unless `--force`.

## RPO / RTO

| Tier | Schedule | RPO | RTO |
|---|---|---|---|
| Nightly full export | cron, 03:15 UTC | 24 h | ~10 min (import + boot) |
| Off-box copy | within 1 h of each export | +1 h | +upload time |

Keep 14 nightly archives locally and the last 8 weekly copies off-box. For an
RPO below one hour, schedule the export more often — exports are read-side and
safe against the running server, but they hold no transaction log (no PITR).

## Retention and off-box copy

`retention: keep 14 daily + 8 weekly`; copy each archive (and its sidecar) to a
second location before deleting the local one. The archives contain record data
— treat them like the database itself (encryption at rest, restricted access).

## Restore drill checklist (run quarterly)

1. Take a fresh export on the staging box.
2. Verify the sidecar: `sha256sum -c <archive>.sha256`.
3. Import into a **fresh** namespace/database, not over production: an import is
   applied statement by statement and is *not* transactional — a failure leaves
   a half-applied target.
4. Boot the server pointed at the restored coordinates.
5. Smoke checks: `/health`, `/status` room counts, one login, one leaderboard
   read, `/v2/account/export` for a fixture user.
6. Record elapsed time; compare with the RTO above.
7. Note in the log: who drilled, which archive, outcome.

## What a dump does not restore

Access-method signing keys and bucket backend credentials are redacted from
exports by design. Redefine them (`DEFINE ACCESS ... TYPE JWT ALGORITHM ...`,
bucket credentials via environment) after any restore, before issuing tokens.
