// Games Catalog: every registered game on this server, the two-step registration
// modal, and the engine-aware detail sheet.
//
// The surface is `/v2/console/games`: the list answers `{ games, total }` and a
// creation response carries `api_key` once — the detail route re-reads it on
// demand. Each game now carries a server-owned `backend_mode` (`relay` for
// Unity/Unreal/Godot, `wasm` for Web/Custom); the catalog surfaces it as a badge
// and the detail sheet branches its tabs on it. The write layer is the console's
// own (`useConsoleQuery` / `useConsoleMutation`), so the bearer token, the 401
// drop and the toast come for free.

import { useState } from "react"
import { Gamepad2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { DataTable, Mono, PageHeading, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { num } from "@/lib/format"

import { GAMES_ENDPOINT, gamePath } from "./routes"
import { ApiKeyDialog, CopyValue, EngineBadge, StatusPill } from "./parts"
import { CreateGameDialog } from "./register"
import { GameDetailView } from "./detail"
import type { CreatedGame, Game, GameDetail, GameList } from "./model"

// The shapes live in `model.ts`; re-exported here so the area's door keeps its
// public type surface stable.
export type {
  AbiReport,
  CreatedGame,
  Game,
  GameConfig,
  GameDetail,
  GameList,
  UnitySettingsJson,
  WasmModuleInfo,
} from "./model"

// --------------------------------------------------------------- module stats

/** The list's per-game module figures. It re-reads the same detail route the
 * drawer uses — the query key matches, so an open detail and its row share one
 * request — because the list envelope carries no module columns. Only a wasm-mode
 * game has modules; a relay game shows a dash, since the plane is not used. */
function GameModulesCell({ game }: { game: Game }) {
  const isWasm = game.backend_mode === "wasm"
  // The same query key the detail sheet uses, so an open drawer and its row share
  // one request; disabled for a relay game, whose plane has no modules to read.
  const detail = useConsoleQuery<GameDetail>(`game-${game.id}`, gamePath(game.id), {}, isWasm)
  if (!isWasm) return <Mono>—</Mono>
  const d = detail.data
  if (d === undefined) return <Mono>loading…</Mono>
  const active =
    d.active_module_version === null || d.active_module_version === undefined
      ? "—"
      : `v${d.active_module_version}`
  return (
    <Mono className="text-foreground">
      {num(d.module_count)} · <span className="text-muted-foreground">active {active}</span>
    </Mono>
  )
}

// --------------------------------------------------------------------- view

export function GamesView() {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [registering, setRegistering] = useState(false)
  const [created, setCreated] = useState<CreatedGame | null>(null)

  const list = useConsoleQuery<GameList>("games", GAMES_ENDPOINT)
  const rows = list.data?.games
  // Looked up by id so an archive that drops the row closes the drawer with it.
  const listed = rows?.find((game) => game.id === selectedId) ?? null
  const detail = useConsoleQuery<GameDetail>(
    selectedId === null ? "game-none" : `game-${selectedId}`,
    selectedId === null ? GAMES_ENDPOINT : gamePath(selectedId),
    {},
    selectedId !== null,
  )

  const columns: Column<Game>[] = [
    { header: "Name", cell: (row) => <span className="font-medium text-foreground">{row.name}</span> },
    {
      header: "Key",
      cell: (row) => <CopyValue value={row.game_key} className="max-w-44 font-mono text-xs" />,
    },
    { header: "Engine", cell: (row) => <EngineBadge engine={row.engine_type} mode={row.backend_mode} /> },
    { header: "Status", cell: (row) => <StatusPill status={row.status} /> },
    {
      header: "Sync / Cap",
      cell: (row) => (
        <Mono>
          {num(row.config.tick_rate)} Hz / {num(row.config.max_clients)}
        </Mono>
      ),
    },
    { header: "Modules", cell: (row) => <GameModulesCell game={row} /> },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Games Catalog"
        description="Games registered on this server. Unity/Unreal/Godot run on the C# Relay; Web/Custom run server-side WASM. Select one to configure it, read its credentials or open its studio."
      />

      <Panel
        title="Registered games"
        description={list.data === undefined ? undefined : `${num(list.data.total)} games`}
        actions={
          <Button size="sm" onClick={() => setRegistering(true)}>
            <Gamepad2 />
            Register Game
          </Button>
        }
        fill
      >
        <DataTable
          columns={columns}
          rows={rows}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          onRowClick={(row) => setSelectedId(row.id)}
          selectedKey={selectedId ?? undefined}
          empty="No games registered yet."
          fill
        />
      </Panel>

      {registering && (
        <CreateGameDialog
          onClose={() => setRegistering(false)}
          onCreated={(game) => {
            setRegistering(false)
            setCreated(game)
          }}
        />
      )}

      {created !== null && (
        <ApiKeyDialog
          gameName={created.name}
          apiKey={created.api_key}
          onClose={() => setCreated(null)}
        />
      )}

      <DetailSheet
        open={listed !== null}
        onClose={() => setSelectedId(null)}
        title="Game detail"
        description={listed?.name}
        bodyClassName="space-y-4 px-5"
      >
        {/* Keyed by id so switching rows remounts and refetches; the detail read
            is the only surface that carries the API key and module figures. */}
        {listed !== null && detail.data !== undefined && (
          <GameDetailView key={listed.id} game={detail.data} onArchived={() => setSelectedId(null)} />
        )}
      </DetailSheet>
    </div>
  )
}
