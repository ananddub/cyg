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
