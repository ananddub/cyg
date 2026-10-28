# OpenOxide Gaming Server - Justfile
# Use `just` or `just --list` to view all available commands.

# Default game project
game := "examples/sky3d"

# Default action: show all available commands
default:
    @just --list --unsorted

# Start all containers in the background (Postgres, Redis, Game Server)
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

# Follow live container logs (default: server; use `just logs postgres` for db)
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

# Run health check & diagnostics for a game project
doctor target=game:
    #!/usr/bin/env bash
    G="{{target}}"
    if [[ "$G" != examples/* ]] && [ -d "examples/$G" ]; then
        G="examples/$G"
    fi
    docker compose exec server /bin/bash /app/deploy/entrypoint.sh doctor "$G"
