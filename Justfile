# OpenOxide Gaming Server - Justfile
# Use `just` or `just --list` to view all available commands.

# Default game project
game := "examples/sky3d"

# Default action: show all available commands
default:
    @just --list --unsorted

# Start the container in the background (embedded SurrealKV, Game Server)
up target=game:
    #!/usr/bin/env bash
    G="{{target}}"
    if [[ "$G" != examples/* ]] && [ -d "examples/$G" ]; then
        G="examples/$G"
    fi
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

# Switch to and launch a specific game (e.g. `just game sky3d` or `just game coin-rush`)
game target:
    #!/usr/bin/env bash
    G="{{target}}"
    if [[ "$G" != examples/* ]] && [ -d "examples/$G" ]; then
        G="examples/$G"
    fi
    echo "🎮 Switching game to: $G"
    GAME_DIR="$G" docker compose up -d server

# Check status and health of all containers
status:
    docker compose ps

# List all playable games in examples/
list-games:
    #!/usr/bin/env bash
    echo "🎮 Available game projects in examples/:"
    for d in examples/*; do
        if [ -f "$d/config.toml" ]; then
            echo "  - $(basename "$d") ($d)"
        fi
    done

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
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Compile everything, tests and examples included
check:
    cargo check --workspace --all-targets

# Full test suite (embedded mem:// stores; no SurrealDB needed)
test:
    cargo test --workspace

# Everything .github/workflows/ci.yml runs, in order (`just fmt` for the rest)
ci: clippy check test

# Run health check & diagnostics for a game project
doctor target=game:
    #!/usr/bin/env bash
    G="{{target}}"
    if [[ "$G" != examples/* ]] && [ -d "examples/$G" ]; then
        G="examples/$G"
    fi
    docker compose exec server /bin/bash /app/deploy/entrypoint.sh doctor "$G"

# Run every SDK test this machine can run: web (node), Godot unit + live e2e.
# The Godot e2e spawns a real coin-rush server on scratch ports, drives the
# real SDK through join -> schema -> input -> leave, and tears it down.
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
        echo "== Godot: binary not installed, skipping (unit + live e2e)"
    else
        echo "== Godot unit suites"
        (cd sdk/client/godot && godot --headless --path . -s res://test/schema_decoder_test.gd) || fail=1
        (cd sdk/client/godot && godot --headless --path . -s res://test/cyg_relay_client_test.gd) || fail=1
        echo "== Godot live e2e (real server, coin-rush, WS lane)"
        work=$(mktemp -d /tmp/cyg-sdk-e2e.XXXX)
        cp examples/coin-rush/src/room.ts "$work/room.ts"
        cp examples/coin-rush/out/room_logic.wasm "$work/room.wasm"
        touch "$work/room.wasm"
        mkdir -p "$work/out"
        cp examples/coin-rush/out/cyg.schema.bin "$work/out/cyg.schema.bin"
        {
            echo 'name = "coin-rush"'
            echo 'runtime = "assemblyscript"'
            echo 'source = "room.ts"'
            echo 'out = "room.wasm"'
            echo 'port = 17779'
            echo 'web_port = 17790'
            echo 'ws_port = 18779'
            echo 'tick_rate = 60'
            echo 'max_clients = 16'
            echo 'width = 800.0'
            echo 'height = 500.0'
            echo 'speed = 240.0'
        } > "$work/config.toml"
        BIN="target/release/cyg-game"
        if [[ ! -x "$BIN" ]]; then echo "server binary missing: cargo build --release --bin cyg-game"; exit 1; fi
        "$BIN" build "$work" >/dev/null
        OX_ALLOW_DEFAULT_SECRETS=1 "$BIN" run "$work" >/tmp/cyg-sdk-e2e-server.log 2>&1 &
        srv=$!
        sleep 4
        (cd sdk/client/godot && CYG_E2E_ENDPOINT=http://127.0.0.1:18779 godot --headless --path . -s res://test/live_server_e2e.gd) || fail=1
        kill "$srv" 2>/dev/null || true
        rm -rf "$work"
        echo "== Godot live e2e (real server, sky3d — 3D, batched Rapier3D ABI)"
        work=$(mktemp -d /tmp/cyg-sdk-e2e.XXXX)
        cp examples/sky3d/src/room.ts "$work/room.ts"
        cp examples/sky3d/out/room_logic.wasm "$work/room.wasm"
        touch "$work/room.wasm"
        mkdir -p "$work/out"
        cp examples/sky3d/out/cyg.schema.bin "$work/out/cyg.schema.bin"
        {
            echo 'name = "sky3d"'
            echo 'runtime = "assemblyscript"'
            echo 'source = "room.ts"'
            echo 'out = "room.wasm"'
            echo 'port = 17779'
            echo 'web_port = 17790'
            echo 'ws_port = 18779'
            echo 'tick_rate = 60'
            echo 'max_clients = 16'
        } > "$work/config.toml"
        "$BIN" build "$work" >/dev/null
        OX_ALLOW_DEFAULT_SECRETS=1 "$BIN" run "$work" >/tmp/cyg-sdk-e2e-server.log 2>&1 &
        srv=$!
        sleep 4
        (cd sdk/client/godot && CYG_E2E_ENDPOINT=http://127.0.0.1:18779 CYG_E2E_ROOM=sky3d godot --headless --path . -s res://test/live_server_e2e.gd) || fail=1
        kill "$srv" 2>/dev/null || true
        rm -rf "$work"
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
    cargo build --release --bin cyg-game 2>&1 | tail -1
    pkill -9 -x cyg-game 2>/dev/null || true
    docker rm -f cyg-bench >/dev/null 2>&1 || true
    echo "==> starting docker container (1 CPU / 2GB)"
    docker run -d --name cyg-bench --cpus=1.0 --memory=2g --memory-swap=2g \
        --ulimit nofile=65536:65536 -e OX_ALLOW_DEFAULT_SECRETS=1 \
        -p 7796:7796 -p 8297:8297 \
        -v "$PWD/bench-game-none:/game" \
        -v "$PWD/target/release/cyg-game:/cyg-game:ro" \
        -v /usr/lib64:/hostlib:ro \
        ubuntu:24.04 /hostlib/ld-linux-x86-64.so.2 --library-path /hostlib /cyg-game run /game >/dev/null
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
