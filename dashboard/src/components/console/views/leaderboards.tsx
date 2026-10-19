// Leaderboards: the ranked boards and the selected board's records. Tournaments
// are the boards an operator writes and live in their own area.

import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { RecordsPanel } from "@/components/console/views/leaderboard-records"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleLeaderboard } from "@/lib/console"
import { dash, formatTime, num, shortId } from "@/lib/format"

const PAGE_SIZE = 25

export function LeaderboardsView() {
  const [offset, setOffset] = useState(0)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  // The row the delete dialog points at: one hook for the whole table, with the
  // row's own id passed in as variables.
  const [deleting, setDeleting] = useState<ConsoleLeaderboard | null>(null)

  const list = useConsoleQuery<ConsoleLeaderboard[]>(
    "leaderboards",
    CONSOLE_ENDPOINTS.leaderboards,
    { limit: PAGE_SIZE, offset },
  )

  // Looked back up from the list rather than held, so deleting a board closes
  // its drawer as soon as the list refreshes.
  const selected = list.data?.find((board) => board.id === selectedId) ?? null

  const columns: Column<ConsoleLeaderboard>[] = [
    { header: "ID", cell: (row) => <Mono>{row.id}</Mono> },
    { header: "Title", cell: (row) => <span className="font-medium text-foreground">{row.title}</span> },
    { header: "Records", cell: (row) => <Mono>{num(row.size)}</Mono> },
    {
      header: "Sort",
      cell: (row) => <Badge variant="outline">{row.sort_order === 0 ? "asc" : "desc"}</Badge>,
    },
    {
      header: "Reset",
      cell: (row) => <Mono>{dash(row.reset_schedule)}</Mono>,
    },
    {
      header: "Created",
      cell: (row) => <Mono>{formatTime(row.create_time)}</Mono>,
    },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Leaderboards"
        description="Ranked boards, with the selected board's records and record-level resets."
      />

      <Panel title="Leaderboard definitions" fill>
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          onRowClick={(row) => setSelectedId(row.id)}
          selectedKey={selectedId ?? undefined}
          empty="No leaderboards defined."
          fill
          selectable
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>

      <DetailSheet
        open={selected !== null}
        onClose={() => setSelectedId(null)}
        title="Records"
        description={selected ? `${selected.title} · ${shortId(selected.id)}` : undefined}
      >
        {selected !== null && (
          <RecordsPanel key={selected.id} board={selected} onRequestDelete={() => setDeleting(selected)} />
        )}
      </DetailSheet>

      <ConfirmAction
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onDone={() => {
          if (deleting?.id === selectedId) setSelectedId(null)
          setDeleting(null)
        }}
        title="Delete leaderboard"
        message={`"${deleting?.id ?? ""}" and every record on it go. Nothing puts them back.`}
        confirmLabel="Delete board"
        destructive
        method="DELETE"
        // DELETE /v2/console/leaderboard/{id} — the answer is an empty `{}`, so
        // the list refresh comes from the shared invalidation, not the reply.
        path={`${CONSOLE_ENDPOINTS.leaderboards}/${encodeURIComponent(deleting?.id ?? "")}`}
        success="Leaderboard deleted."
      />
    </div>
  )
}
