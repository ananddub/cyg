// WASM Studio & Deployments: upload a game module for one of the catalog's
// server-authoritative games, watch the server's ABI validation as a checklist,
// and manage the deployed versions — activate/roll back through the same route,
// delete only versions the runtime is not holding.
//
// The studio applies to `backend_mode === "wasm"` games only. Unity/Unreal/Godot
// games run on the C# Relay and the server refuses their WASM routes with a 400,
// so the selector lists only the wasm games; the relay ones simply are not here.
// The one path that can still land on a relay game is a stale deep link from a
// hand-off, and that renders the server's refusal as an explanatory card rather
// than a raw error.

import { useState } from "react"
import { ShieldBan } from "lucide-react"

import { Mono, PageHeading, Panel } from "@/components/console/data-table"
import { Select } from "@/components/console/form"
import { queryError, useConsoleQuery } from "@/hooks/use-console"
import {
  GAMES_ENDPOINT,
  RELAY_WASM_REJECTED,
  takePendingStudioGame,
  type Game,
  type GameList,
} from "@/components/console/views/games"

import { DeploymentHistory, WasmUploadPanel } from "./studio"

/** The card a relay game gets if a stale deep link points the studio at it: the
 * server's own 400 message, worded as an explanation, plus the way back to the
 * plane that does apply (the Unity SDK tab in the game's own detail). */
function RelayNotice({ game }: { game: Game }) {
  return (
    <Panel title="This game does not use WASM" description={game.game_key}>
      <div className="space-y-3 px-4 py-4">
        <div className="flex items-start gap-2 rounded-md border border-border/60 bg-muted/40 p-3 text-sm">
          <ShieldBan className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <span className="text-foreground">{RELAY_WASM_REJECTED}</span>
        </div>
        <p className="text-xs text-muted-foreground">
          <Mono>{game.name}</Mono> runs on the Unity C# Relay plane ({game.engine_type}). Its
          rooms are driven from the client over the relay, so there is no server module to upload.
          Open its detail sheet&rsquo;s <span className="font-medium text-foreground">Unity SDK</span>{" "}
          tab instead.
        </p>
      </div>
    </Panel>
  )
}

export function WasmUploadView() {
  // The hand-off can name any game; validated against the list during render so
  // no effect has to chase the selection.
  const [selectedId, setSelectedId] = useState<string>(() => takePendingStudioGame() ?? "")

  const games = useConsoleQuery<GameList>("games", GAMES_ENDPOINT)
  const all = games.data?.games ?? []
  // Only Web/Custom (server-authoritative) games reach the WASM plane; the relay
  // ones are dropped from the selector entirely.
  const wasmGames = all.filter((game) => game.backend_mode === "wasm")
  const byId = (id: string): Game | undefined => all.find((game) => game.id === id)

  // The game to act on: the selected hand-off when it is a wasm game; otherwise
  // the first wasm game. A hand-off that names a relay game is shown, not dropped.
  const handed = selectedId === "" ? undefined : byId(selectedId)
  const handedIsRelay = handed !== undefined && handed.backend_mode === "relay"
  const activeGameId =
    handed !== undefined && handed.backend_mode === "wasm" ? handed.id : (wasmGames[0]?.id ?? "")

  const gameOptions = wasmGames.map((game) => ({ value: game.id, label: game.name }))

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="WASM Studio & Deployments"
        description="Ship a server-authoritative (Web/Custom) game module, watch the ABI validation the server ran, and roll the deployed versions back and forth."
      />

      {handedIsRelay && handed !== undefined ? (
        <RelayNotice game={handed} />
      ) : (
        <>
          <Panel
            title="Target game"
            description="Only Web / Custom games use the WASM plane; modules are deployed per game."
          >
            <div className="space-y-3 px-4 py-3">
              {gameOptions.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  {games.isPending
                    ? "Loading the catalog…"
                    : queryError(games.error) ??
                      "No Web / Custom games registered — add one in the Games Catalog first."}
                </p>
              ) : (
                <Select
                  label="Game"
                  value={activeGameId}
                  onChange={setSelectedId}
                  options={gameOptions}
                  searchable
                />
              )}
            </div>
          </Panel>

          {gameOptions.length > 0 && (
            <>
              <WasmUploadPanel gameId={activeGameId} disabled={activeGameId === ""} />
              <DeploymentHistory gameId={activeGameId} disabled={activeGameId === ""} />
            </>
          )}
        </>
      )}
    </div>
  )
}
