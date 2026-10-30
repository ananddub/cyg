# Kubernetes + Agones deployment

These manifests run cyg-gaming as a control plane (axum API + status pump), a
SurrealDB StatefulSet, and Agones-managed game-server pods. Apply them in the
order below; every step is reversible with `kubectl delete -f`.

The images are placeholders (`ghcr.io/REPLACE_ME/cyg-gaming:REPLACE_ME_TAG`).
Build one with `deploy/Dockerfile` and push it to your registry first.

## Prerequisites

- Kubernetes ≥ 1.29 and a default StorageClass (the SurrealDB PVC needs one).
- [Agones](https://agones.dev) installed (CRDs + controller) before applying
  `agones-fleet.yaml`.
- `kubectl` with cluster-admin.

## 1. Namespace

    kubectl apply -f namespace.yaml

## 2. Config

`configmap.yaml` holds the non-secret `game.toml`. Ports match the defaults:
control API 8124, game WS 7777, website 8090, native UDP 7778, WebRTC UDP 7779.

    kubectl apply -f configmap.yaml

## 3. Secrets

Never commit real secrets. Copy the template, replace every `REPLACE_ME`, or
create the Secret imperatively (the recipe is inside `secret.template.yaml`):

    kubectl create secret generic cyg-secrets -n cyg \
      --from-literal=OX_SIGNING_KEY="$(openssl rand -hex 32)" \
      --from-literal=OX_REFRESH_SIGNING_KEY="$(openssl rand -hex 32)" \
      --from-literal=OX_SEAT_SECRET="$(openssl rand -hex 32)" \
      --from-literal=OX_SERVER_KEY="$(openssl rand -hex 32)" \
      --from-literal=OX_CONSOLE_SIGNING_KEY="$(openssl rand -hex 32)" \
      --from-literal=OX_CONSOLE_USERNAME=admin \
      --from-literal=OX_CONSOLE_PASSWORD="$(openssl rand -hex 16)" \
      --from-literal=SURREAL_USER=root \
      --from-literal=SURREAL_PASS="$(openssl rand -hex 16)"

No manifest sets `OX_ALLOW_DEFAULT_SECRETS`: the boot guard
(`refuse_default_secrets`) refuses to start on development defaults, and the
deployment relies on that rather than overriding it. See `docs/SECRETS.md` for
the rotation runbook.

## 4. SurrealDB

    kubectl apply -f surrealdb.yaml

Wait for `surrealdb-0` to be Ready before continuing; the control plane's
`/readyz` check reports `database: "down"` until it can connect.

## 5. Control plane

    kubectl apply -f control-plane-deployment.yaml
    kubectl apply -f service-ingress.yaml
    kubectl apply -f monitor-pdb-netpol.yaml

- `readinessProbe` hits `/readyz` (database live, schema migrated, status pump
  running); `livenessProbe` hits `/health` (process-up only, so a datastore
  blip restarts nothing).
- `terminationGracePeriodSeconds: 60` gives the SIGTERM drain room to close
  rooms cleanly.
- The Ingress exposes `/` only. `/metrics` is reachable through the headless
  `cyg-metrics` Service for in-cluster scrapers, never publicly.
- Edit the `Ingress` host/TLS secret and the NetworkPolicy CIDRs for your
  cluster.

## 6. Game servers (Agones)

    kubectl apply -f agones-fleet.yaml

The Fleet schedules 4 `cyg-room` GameServers (Packed) behind the same image.
Direct ports: 7777 tcp+udp (WS), 7778 udp (native), 7779 udp (WebRTC). Each pod
sets `OX_AGONES=1` so the server talks to the `agones-sdk` sidecar on
127.0.0.1:9358. Allocate one for a player:

    kubectl create -f - <<'EOF'
    apiVersion: gameserver.agones.dev/v1
    kind: GameServerAllocation
    spec:
      required:
        gameServerSelector:
          matchLabels: { agones.dev/fleet: cyg-room }
        ports:
          - portPolicy: Dynamic
            protocol: UDP
            port: 7778
    EOF

The response carries `address`/`port`; the `FleetAutoscaler` (Buffer 2) grows
and shrinks the Fleet from demand.

## Room ownership across nodes

Each pod names itself with `OX_NODE_ID` from the downward API (`metadata.name`);
without it every container would be pid 1 and every node would be `cyg_1`, so a
registry row would say nothing about who owns the room. A host pushes one
ownership row per status beat over the bus and the control plane writes it to
`room_registry` (`room_id`, `node_id`, `game_id`, `state`, `peers`, `tick`,
`snapshot_seq`, `update_time`); closing or draining a room deletes the row.

- A row is trusted for 10 s past `update_time`. A node that dies stops
  refreshing, the lease lapses, and the room name becomes claimable again —
  there is no cleanup job.
- `POST /api/matchmake/{method}/{room}` on a room leased to another node answers
  `409 room '<name>' is running on node '<id>'; attach there`. `reconnect` and
  `rejoin` are exempt: the seat ticket is HMAC'd with the shared
  `OX_SEAT_SECRET`, so it is portable and the player lands on the owning node.
- `GET /v2/console/room` (console auth) lists every row with a `stale` flag for
  a lease that has lapsed, which is how you tell a dead host from a quiet one.

## 7. Verify

    kubectl -n cyg get pods,svc,ingress,fleet,fleetautoscaler
    kubectl -n cyg exec deploy/cyg -- wget -qO- localhost:8124/readyz
    curl -k https://<ingress-host>/health

`/readyz` returning `{"status": "ready"}` means the pod receives traffic; the
Prometheus scrape surface is documented in `docs/MONITORING.md`.

## Backup / restore

`scripts/backup.sh` and `scripts/restore.sh` (and the `cyg-game backup|restore`
subcommands) speak the same HTTP endpoint the StatefulSet exposes; see
`docs/BACKUP.md`.
