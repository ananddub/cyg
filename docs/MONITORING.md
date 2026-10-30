# Monitoring

The control plane exposes Prometheus text at **`GET /metrics`**
(`crates/engine-api/src/controllers/status.rs`). It is hand-rendered — there is
no exporter crate in the binary. The endpoint is behind
`server_key_or_console`: authenticate with either the
`[session] server_key` as HTTP Basic (`Basic base64(server_key:)`) or a Bearer
console token. An unauthenticated scrape gets `401`, so the scrape config below
must carry the credential.

Values come from the single `StatusSnapshot` the sim host publishes every
window (`crates/engine-bus/src/telemetry.rs`, consumed by
`crates/engine-api/src/infra/pump.rs`). No snapshot yet (fresh boot, no host)
⇒ every family is emitted with value `0`, so `# TYPE` lines always parse.

## Metric families

| Metric | Type | Meaning |
| --- | --- | --- |
| `cyg_tick_hz_est` | gauge | Measured tick rate (Hz) over the publish window |
| `cyg_nominal_tick_us` | gauge | Configured tick period (µs) |
| `cyg_tick_budget_p99_us` | gauge | Tick p99 wall time (µs) |
| `cyg_tick_overruns_total` | gauge | Ticks past budget in the window (resets each publish) |
| `cyg_desyncs_total` | gauge | Client checksum reports that disagreed with the host checkpoint |
| `cyg_stage_sim_us` | gauge | Simulation-stage average µs/tick |
| `cyg_tick_duration_us` | histogram | Tick wall-time distribution; bucket `i` covers `[2^i, 2^(i+1))` µs, 12 buckets + `+Inf`; count/sum are window-scoped (`le` is an upper bound in µs) |
| `cyg_kernel_to_sim_p50_us` / `_p99_us` | gauge | Latency from kernel receive to sim apply |
| `cyg_players` | gauge | Players attached to rooms |
| `cyg_rooms{phase="active\|hibernating\|created"}` | gauge | Live rooms by lifecycle phase (`created` = never seated anybody, `hibernating` = emptied with its idle window running) |
| `cyg_ws_sessions` / `cyg_webrtc_sessions` / `cyg_udp_sessions` | gauge | Sessions per transport |
| `cyg_deltas_sent` | gauge | Deltas sent in the window |
| `cyg_deltas_dropped_full` | gauge | Deltas dropped because a session queue was full |
| `cyg_inputs_dropped` | gauge | Inputs dropped (queue full / unknown room) |

The window-scoped gauges (`tick_overruns_total`, `deltas_*`, `inputs_dropped`,
histogram counts) are *not* monotonic process counters — treat them as gauges
and alert on the current value, not `rate()`.

## Logs

Default format is `tracing`'s compact line on stderr. Set
`OX_LOG_FORMAT=json` for one JSON object per line (fields `timestamp`, `level`,
`fields.message`, and span fields). Every HTTP request runs inside an
`http` span carrying `request_id` and `method`, so JSON logs are greppable by
request id. `RUST_LOG` still controls the filter (default `info`).

## Request ids

`x-request-id` is added to **every** response (including 401s and 404s). A
printable-ASCII incoming header of ≤128 chars is echoed unchanged — which is
how gateway request ids propagate through the control plane; anything else is
replaced with a fresh UUID v4.

## Prometheus scrape config

```yaml
scrape_configs:
  - job_name: cyg-game
    metrics_path: /metrics
    scheme: http                       # or https behind the ingress
    basic_auth:
      username: cyg                    # ignored; only the server key is checked
      password_file: /etc/prometheus/secrets/cyg-server-key
    static_configs:
      - targets: ["game-server:8124"]   # [control] api_port, default 8124
```

Alert rules:

```yaml
groups:
  - name: cyg-game
    rules:
      - alert: CygTickP99OverBudget
        expr: cyg_tick_budget_p99_us > cyg_nominal_tick_us
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: "Tick p99 ({{ $value }} µs) exceeds the configured period"
      - alert: CygTickStalled
        expr: cyg_tick_hz_est < 0.5 * (1000000 / cyg_nominal_tick_us)
        for: 2m
        labels: { severity: critical }
        annotations: { summary: "Measured tick rate collapsed" }
      - alert: CygInputDrops
        expr: cyg_inputs_dropped > 0
        for: 10m
        labels: { severity: warning }
        annotations: { summary: "Inputs are being dropped continuously" }
      - alert: CygDeltaBackpressure
        expr: cyg_deltas_dropped_full > 0
        for: 10m
        labels: { severity: warning }
        annotations: { summary: "Delta queues are full — clients desync" }
      - alert: CygNoRooms
        expr: cyg_rooms == 0
        for: 30m
        labels: { severity: info }
        annotations: { summary: "Nothing scheduled; check drain/lifecycle" }
```

## Grafana

`docs/site/grafana-cyg.json` is a four-panel start: tick rate, tick p99 vs
budget, sessions by transport, and the tick duration histogram as a heatmap
(format the `le` buckets with the "heatmap" panel and `cyg_tick_duration_us_bucket`).
Import it against the Prometheus data source; no templating beyond `$job`.

## Probes

`/health` answers as soon as the process exists, so it is the liveness probe:
restarting a pod that cannot reach its datastore would only lose more rooms.

`/readyz` is the readiness probe and answers 503 until all three checks pass:

| check    | live when                                        |
| -------- | ------------------------------------------------ |
| database | `RETURN 1` succeeds (or no endpoint is configured) |
| schema   | `migrations::pending` returns nothing              |
| pump     | the status pump owns the snapshot queue            |

```json
{"status": "unready", "checks": {"database": "live", "schema": "1 pending", "pump": "running"}}
```

A pod whose schema has not been applied is deliberately kept out of rotation
rather than serving reads from a half-migrated store. The Kubernetes manifests
in `deploy/k8s/` wire both probes.
