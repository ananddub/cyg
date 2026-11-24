# OpenOxide Gaming Server - Justfile
# Use `just` or `just --list` to view all available commands.

# Game project directory (a folder with a config.toml)
game := "games/my-game"

# Default action: show all available commands
default:
    @just --list --unsorted

# Start the container in the background (embedded SurrealKV, Game Server)
up target=game:
    #!/usr/bin/env bash
    G="{{target}}"
    echo "🚀 Starting OpenOxide stack with game: $G"
    GAME_DIR="$G" docker compose up -d

# Stop all running containers
down:
    @echo "🛑 Stopping OpenOxide stack..."
    docker compose down

# Fast restart the game server container (~1.2s)
restart:
    @echo "🔄 Restarting OpenOxide game server..."
    docker compose restart server

# Follow live container logs (the stack is one service)
logs service="server":
    docker compose logs -f {{service}}

# Switch to and launch a specific game project directory
game target:
    #!/usr/bin/env bash
    G="{{target}}"
    echo "🎮 Switching game to: $G"
    GAME_DIR="$G" docker compose up -d server

# Check status and health of all containers
status:
    docker compose ps

# Open an interactive shell inside the game server container
sh:
    docker compose exec -it server /bin/bash

# Recompile Rust server binary on the host
build:
    @echo "🦀 Compiling engine release binary on host..."
    cargo build --release
    @echo "💡 Done! Run 'just restart' to load the new binary in the container."

# Release build with OTLP trace export; inert unless OTEL_EXPORTER_OTLP_ENDPOINT is set.
build-otel:
    @echo "🦀 Compiling engine with the OpenTelemetry bridge..."
    cargo build --release --features otel
    @echo "💡 Set OTEL_EXPORTER_OTLP_ENDPOINT (e.g. http://collector:4318) before running."

# Rebuild both dashboard UI + Rust binary and restart the game server container
rebuild: build-dashboard build restart

# Local release build — GitHub CI nahi, tumhara system (build + archive + ready command)
release-local version="v0.1.0-local":
    @bash scripts/release-local.sh {{version}}

# Rebuild Web Console Dashboard on the host
build-dashboard:
    @echo "🎨 Building console dashboard dist..."
    cd dashboard && pnpm build
    @echo "💡 Done! Changes are immediately live at http://localhost:19224/dashboard/"

# Install dashboard deps then build dist (cargo embeds dashboard/dist at compile time)
dashboard:
    cd dashboard && pnpm install --frozen-lockfile && pnpm build

# Format the whole tree (CI gates only the files a change touches)
fmt:
    cargo fmt --all

# The same check, without writing
fmt-check:
    cargo fmt --all --check

# Lints, as CI runs them
# Gate: clippy must not report an *error*. Warnings are a backlog, not a gate —
# the tree carries hundreds of them and `-D warnings` would make this recipe,
# and `just ci`, permanently red.
clippy:
    taskset -c 0-7 nice -n 15 cargo clippy --workspace --all-targets

# The aspiration, not the gate: what the tree looks like with no warnings left.
clippy-strict:
    taskset -c 0-7 nice -n 15 cargo clippy --workspace --all-targets -- -D warnings

# Compile everything, tests and examples included
check:
    taskset -c 0-7 nice -n 15 cargo check --workspace --all-targets

# Full test suite (embedded mem:// stores; no SurrealDB needed)
# RUST_MIN_STACK: SurrealDB 4's query executor recurses deeper than the 2 MiB
# default test-thread stack — the boot-migration tests overflow without it.
# taskset pins everything to the first 8 cores and nice keeps the build polite:
# an unconstrained cargo test on a 32-core box starves the desktop/OS.
test:
    RUST_MIN_STACK=16777216 taskset -c 0-7 nice -n 15 cargo test --workspace -j 6

# The API plane on its own: fast enough to run before every commit, and what
# the `api` job in CI gates on.
api-test:
    RUST_MIN_STACK=16777216 cargo test -p engine-api --lib --tests --no-fail-fast

# Unconstrained (CI box, or when you actually want the whole machine)
test-full:
    RUST_MIN_STACK=16777216 cargo test --workspace

# Tests inside a Docker sandbox with HARD limits (taskset alone does not cap
# RAM — an out-of-control test loop can still stall the host OS). 8 CPUs,
# 16 GiB RAM, no swap, 512 processes. Host cargo registry is mounted so crate
# downloads are reused. Same recipe shape as `test`: extra args pass through.
#   just test-docker                     # workspace tests
#   just test-docker -p engine-api       # one crate
test-docker *args="":
    docker run --rm --name cygtest --cpuset-cpus=0-7 --memory=16g --memory-swap=16g --pids-limit=512 \
        -v "$PWD":"$PWD" -w "$PWD" \
        -v "$PWD"/../../rust/testing_dep:"$PWD"/../../rust/testing_dep \
        -v ~/.cargo/registry:/usr/local/cargo/registry \
        -v cyg-test-target:/tmp/target \
        -e RUST_MIN_STACK=16777216 -e CARGO_TARGET_DIR=/tmp/target -e RUST_TEST_THREADS=8 \
        cyg-test \
        sh -c 'mold -run cargo test {{args}} -j 8'

# Everything .github/workflows/ci.yml runs, in order (`just fmt` for the rest)
ci: fmt-check clippy check test api-test core-ts-test

# Typesafe TS client core: build + typecheck + test (sdk/client/core-ts).
# dist/cyg.cjs is the UMD drop-in; sdk/client/web/cyg.js is built from here.
core-ts-build:
    cd sdk/client/core-ts && npm install --no-fund --no-audit && npm run build

core-ts-test: core-ts-build
    cd sdk/client/core-ts && npm test && node --test ../web/test/*.test.mjs

# Schema -> TS types: emit cyg_schema_generated.d.ts for a game directory.
codegen-ts target=game:
    cargo run -p cyg-gaming -- schema-codegen {{target}} --lang ts

# Build the shared Rust client core to wasm for the web SDK.
core-wasm:
    cargo build -p cyg-core-rs --target wasm32-unknown-unknown --features wasm
    cp target/wasm32-unknown-unknown/debug/cyg_core_rs.wasm sdk/client/web/

# Run health check & diagnostics for a game project
doctor target=game:
    #!/usr/bin/env bash
    G="{{target}}"
    docker compose exec server /bin/bash /app/deploy/entrypoint.sh doctor "$G"

# Rebuild the AssemblyScript test guest (fixture for prediction_parity.rs).
#   just guest-as                       # server-mode module
#   just guest-as client                # client-mode module (CYG_CLIENT_MODE=1)
guest-as mode="server":
    #!/usr/bin/env bash
    set -e
    cd tests/guest_as
    if [ "{{mode}}" = client ]; then
        asc room.ts -o room.wasm --runtime stub -Ospeed --noAssert --use CYG_CLIENT_MODE=1
    else
        asc room.ts -o room.wasm --runtime stub -Ospeed --noAssert
    fi
    echo "==> built tests/guest_as/room.wasm ({{mode}} mode)"

# Run every SDK test this machine can run: web (node), Unity, Godot unit suites.
test-sdks:
    #!/usr/bin/env bash
    set -e
    fail=0
    echo "== web SDK (node)"
    (cd sdk/client/web && node test/schema.test.mjs) || fail=1
    (cd sdk/client/web && node test/signal.test.mjs) || fail=1
    (cd sdk/client/web && node test/extras.test.mjs) || fail=1
    (cd sdk/client/web && node test/input.test.mjs) || fail=1
    echo "== schema input lane (live e2e: scaffold + server + real SDK)"
    node tests/e2e/input_lane_e2e.mjs || fail=1
    echo "== Unity runtime (dotnet)"
    if command -v dotnet >/dev/null; then
        (cd "sdk/client/unity/Tools~/CygRuntime.Tests" && dotnet run) || fail=1
    else
        echo "dotnet not installed, skipping"
    fi
    if ! command -v godot >/dev/null; then
        echo "== Godot: binary not installed, skipping (unit suites)"
    else
        echo "== Godot unit suites"
        (cd sdk/client/godot && godot --headless --path . -s res://test/schema_decoder_test.gd) || fail=1
        (cd sdk/client/godot && godot --headless --path . -s res://test/cyg_relay_client_test.gd) || fail=1
    fi
    exit $fail

# Full docker 1-core/2GB capacity benchmark: builds, runs 5k + 10k CCU stages,
# prints a summary table, cleans up.
#   just cyg-bench            (default: 5k + 10k)
#   just cyg-bench 2000       (single stage)
cyg-bench stages="5k 10k":
    #!/usr/bin/env bash
    set -e
    echo "==> building release binary"
    cargo build --release --bin cyg 2>&1 | tail -1
    pkill -9 -x cyg 2>/dev/null || true
    docker rm -f cyg-bench >/dev/null 2>&1 || true
    echo "==> starting docker container (1 CPU / 2GB)"
    docker run -d --name cyg-bench --cpus=1.0 --memory=2g --memory-swap=2g \
        --ulimit nofile=65536:65536 -e CYG_ALLOW_DEFAULT_SECRETS=1 \
        -p 7796:7796 -p 8297:8297 \
        -v "$PWD/bench-game-none:/game" \
        -v "$PWD/target/release/cyg:/cyg:ro" \
        -v /usr/lib64:/hostlib:ro \
        ubuntu:24.04 /hostlib/ld-linux-x86-64.so.2 --library-path /hostlib /cyg run /game >/dev/null
    sleep 3
    for stage in {{stages}}; do
        case "$stage" in
            2k) procs=4; rooms=128; stagger=6 ;;
            5k) procs=10; rooms=320; stagger=6 ;;
            10k) procs=20; rooms=640; stagger=8 ;;
            *) echo "unknown stage: $stage"; exit 1 ;;
        esac
        node bench/spawn_mt.mjs --engine cyg --container cyg-bench \
            --procs "$procs" --clients 500 --rooms "$rooms" --seconds 30 --hz 20 \
            --ramp 60 --stagger "$stagger" --stage "$stage" | grep STAGE
    done
    echo
    echo "================ 10k BENCH SUMMARY ================"
    printf "%-6s %8s %8s %8s %10s %s\n" stage joined errors cpu mem oom
    python3 bench/summary.py {{stages}}
    docker rm -f cyg-bench >/dev/null 2>&1 || true
    echo "===================================================="

# Profiled capacity bench: who eats CPU / RAM under real load.
#   just cyg-bench-prof 10k cpu    # SIGPROF sampling -> cpu-flamegraph.svg
#   just cyg-bench-prof 10k heap   # dhat per-alloc backtraces -> dhat-heap.json
# Artifacts land in /tmp/cyg-prof/. Server self-exits after the stage so the
# profiler flushes; heap mode gets 3GB (dhat's own tables) — cpu mode keeps
# the real 1 CPU / 2GB capacity envelope.
cyg-bench-prof stage="10k" mode="cpu":
    #!/usr/bin/env bash
    set -e
    case "{{mode}}" in
        cpu)  feat=cpu-prof; envname=CYG_CPU_PROF_SECS ;;
        heap) feat=dhat;     envname=CYG_DHAT_SECS ;;
        *) echo "mode must be cpu|heap"; exit 1 ;;
    esac
    case "{{stage}}" in
        2k)  procs=4;  rooms=128; stagger=6; profsecs=150 ;;
        5k)  procs=10; rooms=320; stagger=6; profsecs=220 ;;
        10k) procs=20; rooms=640; stagger=8; profsecs=340 ;;
        *) echo "unknown stage: {{stage}}"; exit 1 ;;
    esac
    mem=2g
    [ "{{mode}}" = heap ] && mem=3g
    # Symbol-ful builds are huge; keep them off the repo SSD (often near-full).
    proftarget="$HOME/.cache/cyg-prof-target"
    echo "==> building profiling binary ($feat, symbols kept, target: $proftarget)"
    cargo build --profile profiling --target-dir "$proftarget" --features "$feat" --bin cyg 2>&1 | tail -1
    pkill -9 -x cyg 2>/dev/null || true
    docker rm -f cyg-bench >/dev/null 2>&1 || true
    echo "==> starting docker container (1 CPU / $mem, {{mode}} profile)"
    docker run -d --name cyg-bench --cpus=1.0 --memory=$mem --memory-swap=$mem \
        --ulimit nofile=65536:65536 -e CYG_ALLOW_DEFAULT_SECRETS=1 -e "$envname=$profsecs" \
        -w /game -p 7796:7796 -p 8297:8297 \
        -v "$PWD/bench-game-none:/game" \
        -v "$proftarget/profiling/cyg:/cyg:ro" \
        -v /usr/lib64:/hostlib:ro \
        ubuntu:24.04 /hostlib/ld-linux-x86-64.so.2 --library-path /hostlib /cyg run /game >/dev/null
    sleep 3
    node bench/spawn_mt.mjs --engine cyg --container cyg-bench \
        --procs "$procs" --clients 500 --rooms "$rooms" --seconds 30 --hz 20 \
        --ramp 60 --stagger "$stagger" --stage "{{stage}}" | grep STAGE
    echo "==> waiting for profiler clean exit (deadline ${profsecs}s from server boot)"
    docker wait cyg-bench >/dev/null
    docker logs cyg-bench 2>&1 | grep -E "\[dhat\]|\[cpu-prof\]" | tail -4 || true
    mkdir -p /tmp/cyg-prof
    if [ "{{mode}}" = heap ]; then
        cp bench-game-none/dhat-heap.json "/tmp/cyg-prof/dhat-heap-{{stage}}.json"
        echo "==> artifact: /tmp/cyg-prof/dhat-heap-{{stage}}.json"
    else
        cp bench-game-none/cpu-flamegraph.svg "/tmp/cyg-prof/cpu-{{stage}}.svg"
        echo "==> artifact: /tmp/cyg-prof/cpu-{{stage}}.svg"
    fi
    docker rm -f cyg-bench >/dev/null 2>&1 || true
