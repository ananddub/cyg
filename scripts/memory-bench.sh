#!/usr/bin/env bash
# Peak-memory benchmark for the real server under a cgroup v2 memory cap:
# the kernel's memory.peak, never an estimate. See --help for the knobs.

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BIN=${BIN:-$ROOT/target/release/cyg-game}
FIXTURE=$ROOT/crates/engine-wasm/tests/assemblyscript

SCENARIOS=both
LIMIT_MIB=500
SETTLE=15
CPUS=0
GAME_PORT=17777
WEB_PORT=18090
API_PORT=18124
JSON=0
KEEP=0

usage() {
  cat <<'USAGE'
Peak-memory benchmark for the real server under a cgroup v2 memory cap.

The room module is built ahead of time, so the measured boot is the steady
state a deployment actually runs — not the one-off AssemblyScript/AOT compile.
Every figure comes from the kernel (/sys/fs/cgroup/.../memory.peak), never an
estimate. Run from the repo root after `cargo build --release`.

  scripts/memory-bench.sh                        # db off + db on, 500 MiB cap, CPU 0
  scripts/memory-bench.sh --scenario on --json
  scripts/memory-bench.sh --limit-mib 256 --settle 30

Requires cgroup v2 and a user systemd session (systemd-run --user); without it
the script falls back to the process's VmHWM and says so.
USAGE
}

# ---- inner mode: runs inside the cgroup scope -------------------------------
if [[ ${1:-} == --inner ]]; then
  shift
  dir=$1 settle=$2 engine=$3 log=$4
  trap 'kill "$srv" 2>/dev/null; wait "$srv" 2>/dev/null' EXIT

  # Own ports for UDP: an unrelated server on the default 0.0.0.0:7778 would
  # otherwise log a bind error (harmless, but the benchmark log should be clean).
  export OX_UDP_ADDR="${OX_UDP_ADDR:-127.0.0.1:17778}"
  OX_ALLOW_DEFAULT_SECRETS=1 "$BIN" run "$dir" >"$log" 2>&1 &
  srv=$!

  ready=0
  for _ in $(seq 1 150); do
    if grep -q "control plane on" "$log"; then ready=1; break; fi
    if ! kill -0 "$srv" 2>/dev/null; then
      echo "server exited before it was ready:" >&2
      tail -n 20 "$log" >&2
      exit 1
    fi
    sleep 0.2
  done
  [[ $ready == 1 ]] || { echo "server never became ready" >&2; tail -n 20 "$log" >&2; exit 1; }

  sleep "$settle"

  oom=0
  if [[ $engine == cgroup ]]; then
    cg=$(awk -F: '$1=="0"{print $3}' /proc/self/cgroup)
    base=/sys/fs/cgroup$cg
    peak=$(cat "$base/memory.peak")
    anon=$(awk '$1=="anon"{print $2}' "$base/memory.stat")
    oom=$(awk '$1=="oom_kill"{print $2}' "$base/memory.events")
  else
    peak=$(($(awk '$1=="VmHWM:"{print $2}' "/proc/$srv/status") * 1024))
    anon=$(($(awk '$1=="VmRSS:"{print $2}' "/proc/$srv/status") * 1024))
  fi

  # The sim loop's own denominator: a boot that never ticks is not a sample.
  metrics=$(curl -sf --max-time 2 "http://127.0.0.1:${OX_BENCH_API:-18124}/metrics" || true)
  tick=$(awk '$1=="cyg_tick_hz_est"{print $2}' <<<"$metrics" | tail -n 1)
  players=$(awk '$1=="cyg_players"{print $2}' <<<"$metrics" | tail -n 1)
  db=$(grep -E '\[database\] .*memtable' "$log" | tail -n 1 | sed -E 's/^.*: (memtable .*)$/\1/' || true)

  echo "RESULT|${peak}|${anon}|${oom}|${tick:-0}|${players:-0}|${db}"
  exit 0
fi

# ---- argument parsing ------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case $1 in
    --scenario)   SCENARIOS=$2; shift 2 ;;
    --limit-mib)  LIMIT_MIB=$2; shift 2 ;;
    --settle)     SETTLE=$2; shift 2 ;;
    --cpus)       CPUS=$2; shift 2 ;;
    --game-port)  GAME_PORT=$2; shift 2 ;;
    --web-port)   WEB_PORT=$2; shift 2 ;;
    --api-port)   API_PORT=$2; shift 2 ;;
    --json)       JSON=1; shift ;;
    --keep)       KEEP=1; shift ;;
    -h|--help)    usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

case $SCENARIOS in off|on|both) ;; *) echo "--scenario must be off, on or both" >&2; exit 2 ;; esac
[[ -x $BIN ]] || { echo "missing $BIN — run: cargo build --release" >&2; exit 1; }
[[ -r /sys/fs/cgroup/cgroup.controllers ]] || { echo "cgroup v2 is required for memory.peak" >&2; exit 1; }
[[ -f $FIXTURE/game_room.wasm ]] || { echo "missing fixture: $FIXTURE/game_room.wasm" >&2; exit 1; }

ENGINE=cgroup
if ! systemd-run --user --scope -q --collect true 2>/dev/null; then
  ENGINE=proc
  echo "warning: no user systemd session — falling back to VmHWM (not a cgroup figure)" >&2
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/cyg-mem-bench.XXXXXX")
cleanup() {
  [[ $KEEP == 1 ]] && { echo "kept: $WORK"; return; }
  rm -rf "$WORK"
}
trap cleanup EXIT

# ---- fixture project: prebuilt module, fixed ports -------------------------
cp "$FIXTURE/game_room.ts" "$WORK/room.ts"
cp "$FIXTURE/game_room.wasm" "$WORK/room.wasm"
touch "$WORK/room.wasm"   # newer than its source: the build step skips the compile

write_config() {
  local db_on=$1
  cat >"$WORK/config.toml" <<EOF
name = "mem-bench"
runtime = "assemblyscript"
source = "room.ts"
out = "room.wasm"
port = $GAME_PORT
web_port = $WEB_PORT
tick_rate = 30
max_clients = 64

[control]
api_port = $API_PORT
EOF
  if [[ $db_on == 1 ]]; then
    cat >>"$WORK/config.toml" <<EOF

[control.session]
database_endpoint = "surrealkv://$WORK/data/cyg"
EOF
  fi
}

# Build once so every measured boot loads the AOT artifact (<1 ms, no compile
# memory in the sample). The fixture ships a wasm, so this needs no toolchain.
write_config 0
"$BIN" build "$WORK" >"$WORK/build.log" 2>&1 || { cat "$WORK/build.log" >&2; exit 1; }
grep -q "AOT precompiled" "$WORK/build.log" || { echo "AOT precompile did not run:" >&2; cat "$WORK/build.log" >&2; exit 1; }

run_session() {
  local db_on=$1 engine=$2 log=$WORK/run-$3.log
  write_config "$db_on"
  rm -rf "$WORK/data"

  local inner=(bash "$0" --inner "$WORK" "$SETTLE" "$engine" "$log")
  local out
  export OX_UDP_ADDR="127.0.0.1:$((GAME_PORT + 1))" OX_BENCH_API=$API_PORT
  if [[ $engine == cgroup ]]; then
    out=$(systemd-run --user --scope -q --collect \
      --unit="cyg-membench-$$-$3" \
      -p "MemoryMax=${LIMIT_MIB}M" -p MemorySwapMax=0 -p "AllowedCPUs=$CPUS" \
      "${inner[@]}")
  else
    out=$("${inner[@]}" 2>/dev/null || true)
  fi

  local peak anon oom tick players db
  IFS='|' read -r _ peak anon oom tick players db <<<"$(grep '^RESULT|' <<<"$out" | tail -n 1)"
  if [[ -z ${peak:-} ]]; then
    echo "scenario '$3' produced no reading — the run died before it could be" >&2
    echo "measured (cap ${LIMIT_MIB} MiB too small?). log: $log" >&2
    tail -n 20 "$log" >&2
    return 1
  fi

  printf '%s|%s|%s|%s|%s|%s\n' "$peak" "$anon" "$oom" "$tick" "$players" "${db:-}"
}

mib() { awk -v b="$1" 'BEGIN{printf "%.1f MiB", b/1048576}'; }

echo "cyg memory benchmark: $BIN ($(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unversioned))"
echo "kernel $(uname -r) | cgroup v2 | MemoryMax=${LIMIT_MIB}M | cpus=$CPUS | settle=${SETTLE}s | source=$ENGINE"
echo

declare -a rows=() labels=()
for key in off on; do
  [[ $SCENARIOS == both || $SCENARIOS == $key ]] || continue
  label="db $key"
  row=$(run_session "$([[ $key == on ]] && echo 1 || echo 0)" "$ENGINE" "$key") || exit 1
  rows+=("$row")
  labels+=("$label")
done

if [[ $JSON == 1 ]]; then
  printf '{"limitMib":%s,"cpus":"%s","settleSecs":%s,"measurement":"%s","scenarios":[' \
    "$LIMIT_MIB" "$CPUS" "$SETTLE" "$ENGINE"
  sep=
  for i in "${!rows[@]}"; do
    IFS='|' read -r peak anon oom tick players db <<<"${rows[$i]}"
    printf '%s{"name":"%s","peakBytes":%s,"anonBytes":%s,"oomKills":%s,"tickHz":%s,"players":%s,"databaseBudget":"%s"}' \
      "$sep" "${labels[$i]}" "$peak" "$anon" "$oom" "$tick" "$players" "${db//\"/}"
    sep=,
  done
  echo "]}"
else
  printf '%-8s %-12s %-12s %-9s %-8s %-7s %s\n' scenario peak steady limit tick-Hz players oom_kill
  for i in "${!rows[@]}"; do
    IFS='|' read -r peak anon oom tick players db <<<"${rows[$i]}"
    printf '%-8s %-12s %-12s %-9s %-8s %-7s %s\n' \
      "${labels[$i]}" "$(mib "$peak")" "$(mib "$anon")" "${LIMIT_MIB} MiB" "$tick" "$players" "$oom"
  done
  echo
  echo "peak  = kernel memory.peak for the whole run (includes boot)"
  echo "steady= anonymous memory at settle time (the process's own heap)"
  echo "room  = minimal AS module, no clients; a full game adds its own state"
  [[ $ENGINE == proc ]] && echo "note  = no user systemd session: values are VmHWM/VmRSS, not cgroup"
fi

for i in "${!rows[@]}"; do
  IFS='|' read -r _ _ oom _ _ _ <<<"${rows[$i]}"
  [[ ${oom:-0} == 0 ]] || { echo "OOM kill recorded in ${labels[$i]}" >&2; exit 1; }
done
