// Tournaments: the boards an operator writes. `GET/POST /v2/console/tournament`
// and `PUT/DELETE /v2/console/tournament/{id}` — the same rows as leaderboards,
// with category 1 fixed by the server.

import { useState } from "react"
import { Pencil, Plus, Trash2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { TournamentDialog } from "./tournament-dialogs"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleLeaderboard } from "@/lib/console"
import { dash, formatTime, num } from "@/lib/format"

const TOURNAMENTS = CONSOLE_ENDPOINTS.tournaments
const PAGE_SIZE = 25

export function TournamentsView() {
  const [offset, setOffset] = useState(0)
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<ConsoleLeaderboard | null>(null)
  const [deleting, setDeleting] = useState<ConsoleLeaderboard | null>(null)

  const list = useConsoleQuery<ConsoleLeaderboard[]>("tournaments", TOURNAMENTS, {
    limit: PAGE_SIZE,
    offset,
  })

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
      <PageHeading
        title="Tournaments"
        description="Scheduled competitive rounds: create a board, move its definition, end it."
      />

      <Panel
        title="Tournaments"
        fill
        actions={
          <Button size="sm" onClick={() => setCreating(true)}>
            <Plus />
            New tournament
          </Button>
        }
      >
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          empty="No tournaments defined."
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
