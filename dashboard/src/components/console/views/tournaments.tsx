// Tournaments: the boards an operator writes. `GET/POST /v2/console/tournament`
// and `PUT/DELETE /v2/console/tournament/{id}` — the same rows as leaderboards,
// with category 1 fixed by the server.

import { useEffect, useState } from "react"
import { Pencil, Plus, Trash2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import {
  DataTable,
  FilterPills,
  Mono,
  PageHeading,
  Pager,
  Panel,
  SearchField,
} from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { TournamentDialog } from "./tournament-dialogs"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleLeaderboard } from "@/lib/console"
import { dash, formatTime, num } from "@/lib/format"

const TOURNAMENTS = CONSOLE_ENDPOINTS.tournaments
const PAGE_SIZE = 25

type AuthFilter = "all" | "authoritative" | "relay"

const FILTERS: { id: AuthFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "authoritative", label: "Authoritative" },
  { id: "relay", label: "Relay" },
]

export function TournamentsView() {
  const [offset, setOffset] = useState(0)
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<ConsoleLeaderboard | null>(null)
  const [deleting, setDeleting] = useState<ConsoleLeaderboard | null>(null)
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<AuthFilter>("all")

  const list = useConsoleQuery<ConsoleLeaderboard[]>("tournaments", TOURNAMENTS, {
    limit: PAGE_SIZE,
    offset,
  })

  useEffect(() => {
    setOffset(0)
  }, [query, filter])

  const needle = query.trim().toLowerCase()
  const filtered = (list.data ?? []).filter((row) => {
    const kind = row.authoritative ? "authoritative" : "relay"
    if (filter !== "all" && kind !== filter) return false
    if (needle === "") return true
    return [row.id, row.title, kind, row.reset_schedule ?? ""].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  const empty =
    list.data !== undefined && list.data.length > 0
      ? "No tournaments match these filters on this page."
      : "No tournaments defined."

  const columns: Column<ConsoleLeaderboard>[] = [
    { header: "ID", cell: (row) => <Mono>{row.id}</Mono> },
    { header: "Title", cell: (row) => <span className="font-medium text-foreground">{row.title}</span> },
    { header: "Records", cell: (row) => <Mono>{num(row.size)}</Mono> },
    {
      header: "Window",
      cell: (row) => <Mono>{`${formatTime(row.start_time)} → ${formatTime(row.end_time)}`}</Mono>,
    },
    { header: "Round", cell: (row) => <Mono>{dash(row.reset_schedule)}</Mono> },
    {
      header: "Auth",
      cell: (row) =>
        row.authoritative ? <Badge variant="secondary">yes</Badge> : <Badge variant="outline">no</Badge>,
    },
    {
      header: "Actions",
      cell: (row) => (
        <div className="flex gap-1.5">
          <Button variant="ghost" size="icon-xs" aria-label={`Edit ${row.id}`} onClick={() => setEditing(row)}>
            <Pencil />
          </Button>
          <Button variant="ghost" size="icon-xs" aria-label={`Delete ${row.id}`} onClick={() => setDeleting(row)}>
            <Trash2 />
          </Button>
        </div>
      ),
    },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <PageHeading
            title="Tournaments"
            description="Scheduled competitive rounds: create a board, move its definition, end it."
          />
        </div>
        <Button className="h-9 shrink-0 gap-1.5" onClick={() => setCreating(true)}>
          <Plus className="size-3.5" />
          New tournament
        </Button>
      </div>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder="Search tournaments by id or title"
          label="Search tournaments"
          className="sm:flex-1"
        />
        <FilterPills
          filters={FILTERS}
          value={filter}
          onChange={setFilter}
          className="sm:ml-auto"
        />
      </div>

      <Panel title="Tournaments" fill>
        <DataTable
          columns={columns}
          rows={filtered}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          empty={empty}
          fill
          selectable
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>

      {/* Keyed so opening a different row re-seeds the form instead of keeping
          the last board's values. */}
      <TournamentDialog key="create" board={null} open={creating} onClose={() => setCreating(false)} />
      <TournamentDialog
        key={`edit-${editing?.id ?? ""}`}
        board={editing}
        open={editing !== null}
        onClose={() => setEditing(null)}
      />

      <ConfirmAction
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        title="Delete tournament"
        message={`"${deleting?.id ?? ""}" and every record on it go. Nothing puts them back.`}
        confirmLabel="Delete tournament"
        destructive
        method="DELETE"
        // A delete answers Empty ({}): the refresh is the shared invalidation,
        // never a read of the reply.
        path={`${TOURNAMENTS}/${encodeURIComponent(deleting?.id ?? "")}`}
        success="Tournament deleted."
      />
    </div>
  )
}
