# Secrets

Everything the plane signs with is read from the environment **once, at process
start**. There is no vault and no hot reload: a new key takes effect when a pod
boots, and the only way to keep already-issued credentials working across that
boot is to tell the new process what the old key was. That dual-key window is
what this document is about.

`cyg-game doctor` audits a project against the rules below before you deploy,
and the control plane refuses to boot on a documented default unless
`OX_ALLOW_DEFAULT_SECRETS=1` is set — which the manifests in `deploy/k8s` never
do.

## What each secret guards

| Secret | Who presents it | What it buys an attacker | Who is expected to hold it |
| --- | --- | --- | --- |
| `OX_SIGNING_KEY` | nothing; the server signs player access tokens with it | forge a session for any player id | control plane pods only |
| `OX_REFRESH_SIGNING_KEY` | nothing; signs refresh tokens | mint fresh sessions indefinitely, surviving a password change | control plane pods only |
| `OX_SERVER_KEY` | the **user** half of HTTP Basic (the password is ignored) on server-to-server endpoints | call privileged endpoints from outside the fleet | control plane plus every backend that calls it |
| `OX_SEAT_SECRET` | nothing; HMACs a seat ticket binding a client to one room | claim a seat in any room and inject input there | control plane **and** sim planes — both verify it |
| `OX_CONSOLE_SIGNING_KEY` | nothing; signs console (admin) tokens | full control plane, at the role the token carries | release engineering |
| `OX_CONSOLE_PASSWORD` | console login | an admin account, if the username is known | release engineering |
| `SURREAL_PASS` | the datastore's own Basic auth | every row, byte for byte, bypassing every check above | release engineering and the database client |

A leaked seat or player key is a per-game incident. A leaked console or database
credential is a company-wide one, which is why they live in a different `Secret`
than anything a game server mounts.

## The dual-key window

Player tokens are labelled: every minted JWT carries a `kid` header naming the
key that signed it, and a verifier accepts exactly two ids — the one it signs
with (`OX_SIGNING_KEY_ID`, default `current`) and the one being retired
(`OX_PREVIOUS_SIGNING_KEY_ID`, default `previous`). A `kid` the process was not
given a secret for is refused rather than guessed at, and a token with no `kid`
at all — minted before labelling existed — is verified with the current key, so
the first rotation does not log out sessions that predate the upgrade.

| To rotate | Set alongside the new value | Window must stay open for |
| --- | --- | --- |
| signing key | `OX_PREVIOUS_SIGNING_KEY` | `session.token_expiry_secs` (default 60s) |
| refresh signing key | `OX_PREVIOUS_REFRESH_SIGNING_KEY` | `session.refresh_expiry_secs` (default 1h) |
| server key | `OX_PREVIOUS_SERVER_KEY` | until the slowest backend has redeployed |
| seat secret | `OX_PREVIOUS_SEAT_SECRET` | `control.seat_ttl_secs` (default 5m) |

The window is not a grace period you can shorten after the fact: it is how long
a credential minted with the retired key stays verifiable, so closing it early
is what a mass logout looks like.

## Rotate a player signing key

1. Read the live value out of the cluster so you can hand it to the next pod.

   ```bash
   kubectl -n cyg get secret cyg-secrets -o jsonpath='{.data.OX_SIGNING_KEY}' | base64 -d
   ```

2. Generate the replacement and write **both** into the Secret. The old value
   moves to `OX_PREVIOUS_SIGNING_KEY`; the new one takes `OX_SIGNING_KEY`. Give
   the pair distinct ids so nothing has to be guessed:

   ```bash
   OLD=$(kubectl -n cyg get secret cyg-secrets -o jsonpath='{.data.OX_SIGNING_KEY}' | base64 -d)
   kubectl -n cyg create secret generic cyg-secrets \
     --dry-run=client -o yaml \
     --from-literal=OX_SIGNING_KEY_ID=2026-10 \
     --from-literal=OX_SIGNING_KEY="$(openssl rand -hex 32)" \
     --from-literal=OX_PREVIOUS_SIGNING_KEY_ID=2026-09 \
     --from-literal=OX_PREVIOUS_SIGNING_KEY="$OLD" \
     ... every other key already in the Secret ... \
     | kubectl apply -f -
   ```

   Listing every key in the `create` command is deliberate: `envFrom` takes the
   whole `Secret`, so a partial update that drops a key silently removes it from
   the pod too.

3. Roll the plane. A `Secret` change alone does not move a pod.

   ```bash
   kubectl -n cyg rollout restart deployment/cyg
   kubectl -n cyg rollout status deployment/cyg
   ```

4. Prove it worked before you walk away: `/readyz` returns 200, an already
   logged-in client is still logged in, and a fresh login mints a token whose
   header says the new id.

   ```bash
   curl -fsS http://<control-plane>/readyz
   ```

5. Wait out the window in the table above — refresh tokens, being the longest
   lived, decide the real number. Then delete the `OX_PREVIOUS_*` keys, restart
   again, and confirm a token minted under the retired key is now refused.

## Rotate the server key and the seat secret

Neither is labelled, so both sides of the swap is simply "trust two values for a
while": `OX_SERVER_KEY` is compared constant-time against the current and the
retired key, and both seat registries — the control plane's and the sim plane's
(`crates/engine-server/src/plane/seats.rs`) — verify an HMAC against the current
secret before trying the retired one. The seat secret therefore has to be
rotated in one step that reaches the control plane *and* every fleet pod, or a
seat minted under the new key will be rejected by a pod still holding the old
pair.

The server key is the exception to "let the old ones expire": it has no expiry.
Keep the window open until every backend that presents it has been redeployed,
then close it.

## The console gap

Console tokens are **not** labelled and there is no previous console key.
Rotating `OX_CONSOLE_SIGNING_KEY` invalidates every admin session immediately,
and rotating `OX_CONSOLE_PASSWORD` does not revoke sessions that were already
minted — they stay valid until they expire.

Until the console keys are labelled the same way player keys are:

- lower `OX_CONSOLE_TOKEN_EXPIRES_SEC` from its 3600 default to a few minutes,
  wait one expiry, then rotate the signing key and roll;
- tell admins to expect a re-login;
- if a console account is compromised, rotate the signing key *and* the
  password, and treat the window between the two as open exposure.

## Before you deploy

```bash
cyg-game doctor game/
```

The audit fails, with the reason, when:

- a signing key, server key, seat secret, console key or console password is
  empty, a documented default, or shorter than 16 characters (8 for the
  password) — the same rules the boot guard applies;
- the database is reachable over the network and `session.database_pass` is a
  default or too short;
- a retired secret in the window is itself weak — anything minted with it still
  verifies today, so it has to stay as unguessable as its replacement;
- `OX_IAP_ALLOW_UNVERIFIED` or `OX_SOCIAL_ALLOW_UNVERIFIED` is anything but off.
  Unverified receipts let any client mint currency; unverified social tokens let
  any client pick which account to log in as. A value the parser cannot read is
  a failure too, because "silently ignored" is not a security posture.

Setting `OX_ALLOW_DEFAULT_SECRETS=1` skips the strength checks — that is what it
is for, on a laptop — and does **not** skip the unverified-flag checks. The
manifest gate in CI fails a `deploy/k8s` file that mentions it at all.
