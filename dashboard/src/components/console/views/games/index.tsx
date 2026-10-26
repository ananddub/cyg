// Games Catalog: the landing's project picker. Each registered game is a card —
// identity, engine/backend, status and the quiet numbers that matter — and the
// whole card opens that project's workspace. The gear is the only way into the
// manage sheet, so opening a game never lands on a configuration form.
//
// The surface is `/v2/console/games`: the list answers `{ games, total }` and a
// creation response carries `api_key` once — the detail route re-reads it on
// demand. The write layer is the console's own (`useConsoleQuery` /
// `useConsoleMutation`), so the bearer token, the 401 drop and the toast come
// for free.

import { useEffect, useRef, useState } from "react"
import { AlertTriangle, Gamepad2, Plus, Search, SearchX, Settings2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { PageHeading } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { num } from "@/lib/format"
import { cn } from "@/lib/utils"

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

type StatusFilter = "all" | "active" | "suspended"

const FILTERS: { id: StatusFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "active", label: "Active" },
  { id: "suspended", label: "Suspended" },
]

/** The card's stamp: a short date reads better than a full locale timestamp. */
function shortDate(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime()) || date.getTime() === 0) return "—"
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })
}

// --------------------------------------------------------------- module count

/** The per-game module figures, read off the detail route the sheet already
 * shares (same query key, so an open sheet and its card cost one request).
 * Only a wasm-mode game has modules; the card skips this line for relay ones. */
function ModuleCount({ game }: { game: Game }) {
  const detail = useConsoleQuery<GameDetail>(`game-${game.id}`, gamePath(game.id))
  const d = detail.data
  return (
    <>
      <span aria-hidden className="text-muted-foreground/40">
        ·
      </span>
      <span>{d === undefined ? "…" : `${num(d.module_count ?? 0)} modules`}</span>
    </>
  )
}

// ------------------------------------------------------------------ the card

function GameCard({
  game,
  selected,
  onOpen,
  onManage,
}: {
  game: Game
  selected: boolean
  onOpen: () => void
  onManage: () => void
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={`Open ${game.name}`}
      onClick={onOpen}
      onKeyDown={(event) => {
        // Keys pressed inside the key-copy or the manage button belong to them.
        if (event.target !== event.currentTarget) return
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault()
          onOpen()
        }
      }}
      className={cn(
        "card-lift group flex min-h-[150px] cursor-pointer flex-col gap-3 rounded-xl border border-border bg-card p-4 text-left",
        "hover:border-foreground/20 hover:bg-accent/40",
        "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none",
        selected && "border-primary/60 bg-primary/5",
      )}
    >
      <div className="flex items-start justify-between gap-2.5">
        <div className="flex min-w-0 items-center gap-3">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-full border border-border bg-background text-muted-foreground transition-transform duration-200 group-hover:scale-105">
            <Gamepad2 className="size-4" />
          </span>
          <div className="flex min-w-0 flex-col">
            <h3 className="truncate text-sm font-semibold text-foreground">{game.name}</h3>
            <CopyValue
              value={game.game_key}
              className="mt-0.5 max-w-52 font-mono text-[11px] text-muted-foreground"
            />
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          <StatusPill status={game.status} />
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Manage ${game.name}`}
            title="Manage game"
            className="text-muted-foreground opacity-70 transition-opacity group-hover:opacity-100"
            onClick={(event) => {
              event.stopPropagation()
              onManage()
            }}
          >
            <Settings2 />
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <EngineBadge engine={game.engine_type} mode={game.backend_mode} />
      </div>

      <div className="mt-auto flex items-center gap-1.5 text-xs text-muted-foreground">
        <span className="font-medium text-foreground">{num(game.config.tick_rate)} Hz</span>
        <span aria-hidden className="text-muted-foreground/40">
          ·
        </span>
        <span className="font-medium text-foreground">{num(game.config.max_clients)}</span>
        <span>players max</span>
        {game.backend_mode === "wasm" && <ModuleCount game={game} />}
        <span className="ml-auto shrink-0">{shortDate(game.created_at)}</span>
      </div>
    </div>
  )
}

// --------------------------------------------------------------------- view

export function GamesView({ onOpenGame }: { onOpenGame?: (game: Game) => void }) {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [registering, setRegistering] = useState(false)
  const [created, setCreated] = useState<CreatedGame | null>(null)
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<StatusFilter>("all")
  const searchRef = useRef<HTMLInputElement>(null)

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

  // "/" focuses the picker, the same gesture the field's badge shows.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return
      const active = document.activeElement as HTMLElement | null
      if (
        active !== null &&
        (active.tagName === "INPUT" ||
          active.tagName === "TEXTAREA" ||
          active.tagName === "SELECT" ||
          active.isContentEditable)
      ) {
        return
      }
      event.preventDefault()
      searchRef.current?.focus()
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [])

  const needle = query.trim().toLowerCase()
  const filtered = (rows ?? []).filter((game) => {
    if (filter !== "all" && game.status !== filter) return false
    if (needle === "") return true
    return [game.name, game.game_key, game.engine_type, game.backend_mode].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  const open = (game: Game) => {
    if (onOpenGame === undefined) setSelectedId(game.id)
    else onOpenGame(game)
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-5">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <PageHeading
            title="Games & Projects"
            description="Pick a game to open its project dashboard — live metrics, rooms, players and game data stay scoped to that game. Use the gear to configure or retire one."
          />
        </div>
        <Button className="h-9 shrink-0 gap-1.5" onClick={() => setRegistering(true)}>
          <Plus className="size-3.5" />
          Register Game
        </Button>
      </div>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="relative w-full sm:flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground/70" />
          <Input
            ref={searchRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") setQuery("")
            }}
            placeholder="Search games, keys or engines"
            aria-label="Search games"
            className="h-9 rounded-lg border-border bg-card pl-9 pr-11 text-sm transition-colors hover:border-foreground/25"
          />
          <kbd
            aria-hidden
            className="pointer-events-none absolute right-3 top-1/2 flex size-5 -translate-y-1/2 items-center justify-center rounded-md border border-border bg-foreground/[0.04] font-mono text-[11px] text-muted-foreground select-none"
          >
            /
          </kbd>
        </div>

        <div className="flex shrink-0 items-center gap-1 rounded-lg border border-border bg-card p-1 sm:ml-auto">
          {FILTERS.map((option) => (
            <button
              key={option.id}
              type="button"
              aria-pressed={filter === option.id}
              onClick={() => setFilter(option.id)}
              className={cn(
                "h-7 cursor-pointer rounded-md px-2.5 text-xs font-medium transition-colors",
                filter === option.id
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      {list.isPending ? (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
          {[1, 2, 3, 4].map((key) => (
            <div key={key} className="h-[150px] animate-pulse rounded-xl border border-border bg-card" />
          ))}
        </div>
      ) : list.error !== null ? (
        <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-border bg-card px-6 py-12 text-center">
          <AlertTriangle className="size-6 text-destructive" />
          <p className="max-w-md text-sm text-destructive">{queryError(list.error)}</p>
          <Button variant="outline" size="sm" onClick={() => void list.refetch()}>
            Try again
          </Button>
        </div>
      ) : rows === undefined || rows.length === 0 ? (
        <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-border bg-card px-6 py-14 text-center">
          <span className="flex size-12 items-center justify-center rounded-full border border-border bg-background text-muted-foreground">
            <Gamepad2 className="size-5" />
          </span>
          <div>
            <h3 className="text-base font-semibold text-foreground">No games registered yet</h3>
            <p className="mt-1 max-w-sm text-sm text-muted-foreground">
              Register a game to give it a key, a runtime config and its own workspace.
            </p>
          </div>
          <Button className="h-9 gap-1.5" onClick={() => setRegistering(true)}>
            <Plus className="size-3.5" />
            Register Game
          </Button>
        </div>
      ) : filtered.length === 0 ? (
        <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-border bg-card px-6 py-14 text-center">
          <SearchX className="size-6 text-muted-foreground/60" />
          <div>
            <h3 className="text-base font-semibold text-foreground">No games match</h3>
            <p className="mt-1 max-w-sm text-sm text-muted-foreground">
              Try another name or key, or clear the status filter.
            </p>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setQuery("")
              setFilter("all")
            }}
          >
            Clear filters
          </Button>
        </div>
      ) : (
        <div className="stagger-in grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
          {filtered.map((game) => (
            <GameCard
              key={game.id}
              game={game}
              selected={selectedId === game.id}
              onOpen={() => open(game)}
              onManage={() => setSelectedId(game.id)}
            />
          ))}
        </div>
      )}

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
        {/* Keyed by id so switching cards remounts and refetches; the detail read
            is the only surface that carries the API key and module figures. */}
        {listed !== null && detail.data !== undefined && (
          <GameDetailView key={listed.id} game={detail.data} onArchived={() => setSelectedId(null)} />
        )}
      </DetailSheet>
    </div>
  )
}
