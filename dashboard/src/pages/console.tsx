// The console shell, split by scope.
//
//   landing (`#/games`)  → the games picker plus console administration
//   game (`#/g/<id>/…`)  → one project's workspace: metrics, rooms, clients,
//                          players, game data and monetization
//
// Only the game workspace opens the metrics feed, because only it shows live
// telemetry. Picking a game mounts that workspace fresh, so switching projects
// never carries a worker drawer or a socket frame across the boundary.

import { useState } from "react"
import { ArrowLeft } from "lucide-react"

import { ConsoleSidebar } from "@/components/console/shell"
import { ThemeToggle } from "@/components/console/theme-toggle"
import { Button } from "@/components/ui/button"
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar"
import { AccountsView } from "@/components/console/views/accounts"
import {
  AclTemplatesView,
  AuditLogView,
  ConsoleUsersView,
  SettingsView,
} from "@/components/console/views/administration"
import { GroupsView } from "@/components/console/views/groups"
import { GamesView, GAMES_ENDPOINT, type Game, type GameList } from "@/components/console/views/games"
import { GameProjectView } from "@/components/console/views/games/project"
import { LeaderboardsView } from "@/components/console/views/leaderboards"
import { MatchesView } from "@/components/console/views/matches"
import {
  ClientsView,
  CoresView,
  LimitsView,
  RoomsView,
  RuntimeView,
} from "@/components/console/views/monitoring"
import { PurchasesView, SubscriptionsView } from "@/components/console/views/monetization"
import { NotificationsView } from "@/components/console/views/notifications"
import { SecurityView } from "@/components/console/views/security"
import { StorageView } from "@/components/console/views/storage"
import { TournamentsView } from "@/components/console/views/tournaments"
import { WasmUploadView } from "@/components/console/views/wasm-upload"
import { CoreDetailsSidebar } from "@/components/core-details-sidebar"
import { DashboardHeader } from "@/components/dashboard-header"
import { OfflineBanner } from "@/components/offline-banner"
import { useConsoleQuery } from "@/hooks/use-console"
import { useDashboardData, useIdentityQuery, type DashboardData } from "@/hooks/use-dashboard-api"
import { useDashboardToasts } from "@/hooks/use-dashboard-toasts"
import { Skeleton } from "@/components/ui/skeleton"
import {
  defaultGameArea,
  defaultGlobalArea,
  findArea,
  gameAreas,
  globalAreas,
  isGameArea,
  isGlobalArea,
} from "@/lib/areas"
import type { ConsoleSession } from "@/lib/console"
import { GameScopeProvider } from "@/lib/console/scope"
import type { ConsoleRoute } from "@/lib/routes"
import { GAMES_ROUTE, gameRoute } from "@/lib/routes"
import type { WorkerRow } from "@/lib/types"

export function ConsolePage({
  session,
  route,
  onNavigate,
  onLogout,
}: {
  session: ConsoleSession
  route: ConsoleRoute
  onNavigate: (to: ConsoleRoute) => void
  onLogout: () => void
}) {
  if (route.gameId === null) {
    return (
      <GlobalConsole
        session={session}
        route={route}
        onNavigate={onNavigate}
        onLogout={onLogout}
      />
    )
  }
  // Keyed by game: a different project is a different workspace, not the same
  // one with new labels. The scope provider makes every console read and write
  // under it carry this tenant, so no view can fall back to install-wide rows.
  return (
    <GameScopeProvider gameId={route.gameId}>
      <GameConsole
        key={route.gameId}
        session={session}
        route={route}
        gameId={route.gameId}
        onNavigate={onNavigate}
        onLogout={onLogout}
      />
    </GameScopeProvider>
  )
}

// ------------------------------------------------------------------ landing

function GlobalConsole({
  session,
  route,
  onNavigate,
  onLogout,
}: {
  session: ConsoleSession
  route: ConsoleRoute
  onNavigate: (to: ConsoleRoute) => void
  onLogout: () => void
}) {
  const area =
    isGlobalArea(route.areaId)
      ? (findArea(route.areaId) ?? defaultGlobalArea())
      : defaultGlobalArea()

  const navigateArea = (id: string) => onNavigate({ gameId: null, areaId: id })
  const openGame = (game: Game) => onNavigate(gameRoute(game.id))

  return (
    <SidebarProvider className="h-svh w-full text-foreground">
      <ConsoleSidebar
        area={area}
        areas={globalAreas()}
        onNavigate={navigateArea}
        session={session}
        onLogout={onLogout}
        flatNav
      />

      <SidebarInset className="min-h-0 min-w-0 flex-1 overflow-hidden">
        <div className="flex h-16 shrink-0 items-center border-b border-border/60 bg-background/80 px-4 backdrop-blur-md lg:px-8">
          <SidebarTrigger className="-ml-2 mr-2" />
          <DashboardHeader
            source="poll"
            connected={false}
            sectionLabel={area.label}
            showFeed={false}
          />
          <ThemeToggle />
        </div>

        <div key={area.id} className="page-in flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-4 lg:p-8">
          <GlobalAreaView area={area.id} onOpenGame={openGame} />
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}

function GlobalAreaView({
  area,
  onOpenGame,
}: {
  area: string
  onOpenGame: (game: Game) => void
}) {
  switch (area) {
    case "games":
      return <GamesView onOpenGame={onOpenGame} />
    case "users":
      return <ConsoleUsersView />
    case "audit":
      return <AuditLogView />
    case "acl":
      return <AclTemplatesView />
    case "settings":
      return <SettingsView />
    case "security":
      return <SecurityView />
    default:
      return <GamesView onOpenGame={onOpenGame} />
  }
}

// -------------------------------------------------------------------- game

function GameConsole({
  session,
  route,
  gameId,
  onNavigate,
  onLogout,
}: {
  session: ConsoleSession
  route: ConsoleRoute
  gameId: string
  onNavigate: (to: ConsoleRoute) => void
  onLogout: () => void
}) {
  // Telemetry is a property of the process, not of the database: only the game
  // this process hosts may show it. A workspace for another game keeps its
  // scoped database views and is told plainly that the live plane is elsewhere.
  const identity = useIdentityQuery()
  const hosted = identity.data?.game_id === gameId
  const data = useDashboardData(gameId, hosted)
  useDashboardToasts(data.ws, data.overview, data.rooms)

  const [worker, setWorker] = useState<WorkerRow | null>(null)
  const offline = hosted && data.ws.status !== "open" && data.overview === null

  // The workspace header needs the game's identity, and the same list the
  // picker read is already cached under this key.
  const games = useConsoleQuery<GameList>("games", GAMES_ENDPOINT)
  const game = games.data?.games.find((candidate) => candidate.id === gameId) ?? null
  const missing = games.data !== undefined && game === null

  const area =
    isGameArea(route.areaId)
      ? (findArea(route.areaId) ?? defaultGameArea())
      : defaultGameArea()

  // Re-resolve against the live list so a poll refreshes the open sidebar.
  const activeWorker =
    worker === null ? null : ((data.workers ?? []).find((candidate) => candidate.name === worker.name) ?? worker)

  const back = () => onNavigate(GAMES_ROUTE)
  const navigateArea = (id: string) => onNavigate(gameRoute(gameId, id))

  return (
    <SidebarProvider className="h-svh w-full text-foreground">
      <ConsoleSidebar
        area={area}
        areas={gameAreas()}
        game={game}
        onBack={back}
        onNavigate={navigateArea}
        session={session}
        onLogout={onLogout}
      />

      <SidebarInset className="min-h-0 min-w-0 flex-1 overflow-hidden">
        <div className="flex h-16 shrink-0 items-center border-b border-border/60 bg-background/80 px-4 backdrop-blur-md lg:px-8">
          <SidebarTrigger className="-ml-2 mr-2" />
          <DashboardHeader
            overview={data.overview ?? undefined}
            source={data.source}
            connected={hosted && data.ws.status === "open"}
            lastFrameAtMs={data.ws.lastFrameAtMs ?? undefined}
            sectionLabel={area.label}
            gameName={game?.name}
          />
          <ThemeToggle />
        </div>

        <div
          key={`${gameId}:${area.id}`}
          className="page-in flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-4 lg:p-8"
        >
          {/* `null`: with no overview at all, there has been no frame since load
              — a stamp of "now" would claim a freshness the feed does not have. */}
          {offline && <OfflineBanner lastGoodAtMs={null} />}
          {missing ? (
            <GameMissing gameId={gameId} onBack={back} />
          ) : (
            <GameAreaView
              area={area.id}
              data={data}
              gameId={gameId}
              hosted={hosted}
              verifying={identity.isPending}
              hostedName={identity.data?.game_name ?? null}
              onSelectWorker={setWorker}
              onBack={back}
            />
          )}
        </div>
      </SidebarInset>

      <CoreDetailsSidebar
        worker={activeWorker}
        overview={data.overview ?? undefined}
        onClose={() => setWorker(null)}
      />
    </SidebarProvider>
  )
}

function GameMissing({ gameId, onBack }: { gameId: string; onBack: () => void }) {
  return (
    <div className="rounded-lg border border-border bg-card p-6 shadow-xs">
      <h2 className="font-heading text-base font-semibold text-foreground">Game not found</h2>
      <p className="mt-1 max-w-prose text-sm text-muted-foreground">
        No game with id <span className="font-mono text-xs">{gameId}</span> is registered on
        this server, or it has been removed. Pick another project from the games list.
      </p>
      <Button size="sm" className="mt-4 gap-1.5" onClick={onBack}>
        <ArrowLeft />
        Back to games
      </Button>
    </div>
  )
}

/** Areas whose data is the running process itself, not a table in the shared
 * database. They are only meaningful in the hosted game's workspace. */
const PROCESS_AREAS = new Set(["runtime", "rooms", "clients", "matches", "cores", "limits"])

function NotHosted({
  gameId,
  hostedName,
  onBack,
}: {
  gameId: string
  hostedName: string | null
  onBack: () => void
}) {
  return (
    <div className="rounded-lg border border-border bg-card p-6 shadow-xs">
      <h2 className="font-heading text-base font-semibold text-foreground">
        Live telemetry is not hosted here
      </h2>
      <p className="mt-1 max-w-prose text-sm text-muted-foreground">
        This server runs{" "}
        <span className="font-medium text-foreground">{hostedName ?? "no registered game"}</span>
        , so it has no rooms, clients or tick data for{" "}
        <span className="font-mono text-xs">{gameId}</span>. Showing its numbers would be
        another game&apos;s numbers. Player data, leaderboards and storage below are still
        scoped to this game.
      </p>
      <Button size="sm" variant="outline" className="mt-4 gap-1.5" onClick={onBack}>
        <ArrowLeft />
        Back to games
      </Button>
    </div>
  )
}

function GameAreaView({
  area,
  data,
  gameId,
  hosted,
  verifying,
  hostedName,
  onSelectWorker,
  onBack,
}: {
  area: string
  data: DashboardData
  gameId: string
  hosted: boolean
  verifying: boolean
  hostedName: string | null
  onSelectWorker: (worker: WorkerRow) => void
  onBack: () => void
}) {
  if (PROCESS_AREAS.has(area) && !hosted) {
    return verifying ? (
      <div className="flex flex-col gap-3">
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-40 w-full" />
      </div>
    ) : (
      <NotHosted gameId={gameId} hostedName={hostedName} onBack={onBack} />
    )
  }

  switch (area) {
    case "project":
      return <GameProjectView gameId={gameId} onArchived={onBack} />
    case "rooms":
      return <RoomsView data={data} />
    case "clients":
      return <ClientsView data={data} />
    case "matches":
      return <MatchesView />
    case "cores":
      return <CoresView data={data} onSelectWorker={onSelectWorker} />
    case "limits":
      return <LimitsView data={data} />
    case "accounts":
      return <AccountsView />
    case "groups":
      return <GroupsView />
    case "notifications":
      return <NotificationsView />
    case "wasm":
      return <WasmUploadView gameId={gameId} />
    case "leaderboards":
      return <LeaderboardsView />
    case "tournaments":
      return <TournamentsView />
    case "storage":
      return <StorageView />
    case "purchases":
      return <PurchasesView />
    case "subscriptions":
      return <SubscriptionsView />
    default:
      return <RuntimeView data={data} />
  }
}
