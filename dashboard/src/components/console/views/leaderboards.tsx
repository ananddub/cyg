// Leaderboards and tournaments (the same table, split by `category`) with the
// selected board's records.

import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DataTable, Field, JsonBlock, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleLeaderboard, type ConsoleRecord } from "@/lib/console"
import { dash, formatTime, num } from "@/lib/format"
import { cn } from "@/lib/utils"

const PAGE_SIZE = 25

type Kind = "leaderboard" | "tournament"

export function LeaderboardsView() {
  const [kind, setKind] = useState<Kind>("leaderboard")
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState<ConsoleLeaderboard | null>(null)

  const list = useConsoleQuery<ConsoleLeaderboard[]>(`boards-${kind}`, CONSOLE_ENDPOINTS[kind === "leaderboard" ? "leaderboards" : "tournaments"], {
    limit: PAGE_SIZE,
    offset,
  })

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
      header: kind === "tournament" ? "Window" : "Created",
      cell: (row) =>
        kind === "tournament" ? (
          <Mono>{`${formatTime(row.start_time)} → ${formatTime(row.end_time)}`}</Mono>
        ) : (
          <Mono>{formatTime(row.create_time)}</Mono>
        ),
    },
  ]

  function switchKind(next: Kind) {
    setKind(next)
    setOffset(0)
    setSelected(null)
  }

  return (
    <div className="space-y-6">
      <PageHeading
        title="Leaderboards"
        description="Ranked boards and scheduled tournaments, with each board's records."
      />

      <div className="flex gap-1.5">
        {(["leaderboard", "tournament"] as const).map((option) => (
          <Button
            key={option}
            variant={kind === option ? "default" : "outline"}
            size="sm"
            onClick={() => switchKind(option)}
            className={cn("capitalize")}
          >
            {option}s
          </Button>
        ))}
      </div>

      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_24rem]">
        <Panel title={`${kind === "tournament" ? "Tournament" : "Leaderboard"} definitions`}>
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            onRowClick={(row) => setSelected(row)}
            selectedKey={selected?.id}
            empty={`No ${kind}s defined.`}
          />
          <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
        </Panel>

        {selected === null ? (
          <Panel title="Records" description="No board selected">
            <p className="px-4 py-6 text-sm text-muted-foreground">Select a board to read its records.</p>
          </Panel>
        ) : (
          <RecordsPanel board={selected} />
        )}
      </div>
    </div>
  )
}

function RecordsPanel({ board }: { board: ConsoleLeaderboard }) {
  const records = useConsoleQuery<ConsoleRecord[]>(
    "board-records",
    // A board id is an operator-chosen string, so it is escaped, not assumed safe.
    `${CONSOLE_ENDPOINTS.leaderboards}/${encodeURIComponent(board.id)}/records`,
  )

  return (
    <div className="space-y-4">
      <Panel title="Board" description={board.id}>
        <div className="px-4 py-2">
          <Field label="Title">{board.title}</Field>
          <Field label="Kind">{board.kind}</Field>
          <Field label="Records">{num(board.size)}</Field>
          <Field label="Max score">{board.max_num_score === 0 ? "—" : num(board.max_num_score)}</Field>
          <Field label="Authoritative">{board.authoritative ? "yes" : "no"}</Field>
          <Field label="Created">{formatTime(board.create_time)}</Field>
        </div>
      </Panel>

      <Panel title="Records" description={`${records.data?.length ?? 0} listed`}>
        {queryError(records.error) !== null ? (
          <p className="px-4 py-6 text-sm text-destructive">{queryError(records.error)}</p>
        ) : records.data === undefined || records.data.length === 0 ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">No records.</p>
        ) : (
          <div className="divide-y divide-border/40">
            {records.data.map((record, index) => (
              <div key={`${record.owner_id}-${index}`} className="flex items-center justify-between gap-3 px-4 py-2.5">
                <div className="min-w-0">
                  <p className="truncate text-xs font-medium text-foreground">
                    {index + 1}. {dash(record.username) === "—" ? record.owner_id : record.username}
                  </p>
                  <Mono>{formatTime(record.update_time)}</Mono>
                </div>
                <span className="shrink-0 font-mono text-sm tabular-nums text-primary">{num(record.score)}</span>
              </div>
            ))}
          </div>
        )}
      </Panel>

      <Panel title="Metadata">
        <div className="p-3">
          <JsonBlock value={board.metadata} />
        </div>
      </Panel>
    </div>
  )
}
