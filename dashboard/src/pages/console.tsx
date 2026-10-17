// The console shell: one live dashboard feed, the grouped sidebar, and a switch
// over the area catalogue. Every area view is self-contained; only the runtime
// ones receive the shared feed, because the metrics socket must be opened once.

import { useState } from "react"

import { ConsoleSidebar } from "@/components/console/shell"
import { AccountsView } from "@/components/console/views/accounts"
import {
  AclTemplatesView,
  AuditLogView,
  ConsoleUsersView,
  SettingsView,
} from "@/components/console/views/administration"
import { GroupsView } from "@/components/console/views/groups"
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
import { CoreDetailsSidebar } from "@/components/core-details-sidebar"
import { DashboardHeader } from "@/components/dashboard-header"
import { OfflineBanner } from "@/components/offline-banner"
import { useDashboardData, type DashboardData } from "@/hooks/use-dashboard-api"
import { useDashboardToasts } from "@/hooks/use-dashboard-toasts"
import type { ConsoleArea } from "@/lib/areas"
import type { ConsoleSession } from "@/lib/console"
import type { WorkerRow } from "@/lib/types"

export function ConsolePage({
  session,
  area,
  onNavigate,
  onLogout,
}: {
  session: ConsoleSession
  area: ConsoleArea
  onNavigate: (id: string) => void
  onLogout: () => void
}) {
  const data = useDashboardData()
  useDashboardToasts(data.ws, data.overview, data.rooms)

  const [worker, setWorker] = useState<WorkerRow | null>(null)
  const offline = data.ws.status !== "open" && data.overview === null
  // Re-resolve against the live list so a poll refreshes the open sidebar.
  const activeWorker =
    worker === null ? null : ((data.workers ?? []).find((candidate) => candidate.name === worker.name) ?? worker)

  return (
    <div className="flex min-h-svh w-full bg-background text-foreground">
      <ConsoleSidebar area={area} onNavigate={onNavigate} session={session} onLogout={onLogout} />

      <div className="flex min-w-0 flex-1 flex-col">
        <div className="sticky top-0 z-10 flex h-16 items-center border-b border-border/60 bg-background/80 px-4 backdrop-blur-md lg:px-8">
          <DashboardHeader
            overview={data.overview ?? undefined}
            source={data.source}
            connected={data.ws.status === "open"}
            lastFrameAtMs={data.ws.lastFrameAtMs ?? undefined}
            sectionLabel={area.label}
          />
        </div>

        <main className="flex-1 space-y-6 p-4 lg:p-8">
          {/* `null`: with no overview at all, there has been no frame since load
              — a stamp of "now" would claim a freshness the feed does not have. */}
          {offline && <OfflineBanner lastGoodAtMs={null} />}
          <AreaView id={area.id} data={data} onSelectWorker={setWorker} />
        </main>
      </div>

      <CoreDetailsSidebar
        worker={activeWorker}
        overview={data.overview ?? undefined}
        onClose={() => setWorker(null)}
      />
    </div>
  )
}

function AreaView({
  id,
  data,
  onSelectWorker,
}: {
  id: string
  data: DashboardData
  onSelectWorker: (worker: WorkerRow) => void
}) {
  switch (id) {
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
    case "leaderboards":
      return <LeaderboardsView />
    case "storage":
      return <StorageView />
    case "purchases":
      return <PurchasesView />
    case "subscriptions":
      return <SubscriptionsView />
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
      return <RuntimeView data={data} />
  }
}
