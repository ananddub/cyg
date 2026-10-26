// Running matches, paged, with the selected match's live state in a right-side
// drawer.
//
// The list is the console's own view (`console.MatchList`): every row is still
// joined and in the plane right now. Our data holds no match creation time, so
// unlike accounts there is no "Created" column to print — inventing one would be
// a lie. What an operator can act on is who is in the match and stopping it.

import { useEffect, useState } from "react"

import { Badge } from "@/components/ui/badge"
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
import { DetailSheet } from "@/components/console/detail-sheet"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleMatch, type ConsoleMatchList } from "@/lib/console"
import { dash, num, shortId } from "@/lib/format"

import { MatchDetail } from "./detail"

const PAGE_SIZE = 25
const MATCHES = CONSOLE_ENDPOINTS.matches

type TypeFilter = "all" | "authoritative" | "relay"

const FILTERS: { id: TypeFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "authoritative", label: "Authoritative" },
  { id: "relay", label: "Relay" },
]

export function MatchesView() {
  const [offset, setOffset] = useState(0)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<TypeFilter>("all")

  const list = useConsoleQuery<ConsoleMatchList>("matches", MATCHES, {
    limit: PAGE_SIZE,
    offset,
  })

  const rows = list.data?.matches
  // Looked back up rather than held, so terminating a match leaves nothing stale:
  // the id stops matching and the detail pane falls to "nothing selected" as soon
  // as the list refreshes.
  const selected = rows?.find((match) => match.api_match.match_id === selectedId) ?? null

  // A search or a type pick is a fresh look at the list, so it starts on page one.
  useEffect(() => {
    setOffset(0)
  }, [query, filter])

  const needle = query.trim().toLowerCase()
  const filtered = (rows ?? []).filter((row) => {
    const kind = row.api_match.authoritative ? "authoritative" : "relay"
    if (filter !== "all" && kind !== filter) return false
    if (needle === "") return true
    return [row.api_match.match_id, row.api_match.label ?? "", kind, row.node ?? ""].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  const empty =
    rows !== undefined && rows.length > 0
      ? "No matches match these filters on this page."
      : "No matches running."

  const columns: Column<ConsoleMatch>[] = [
    {
      header: "Match",
      cell: (row) => (
        <span className="font-medium text-foreground">{shortId(row.api_match.match_id)}</span>
      ),
    },
    { header: "Label", cell: (row) => dash(row.api_match.label) },
    { header: "Players", cell: (row) => <Mono>{num(row.api_match.size)}</Mono> },
    {
      header: "Tick rate",
      cell: (row) =>
        row.api_match.tick_rate === undefined ? <Mono>—</Mono> : <Mono>{num(row.api_match.tick_rate)}</Mono>,
    },
    {
      header: "Type",
      cell: (row) =>
        row.api_match.authoritative ? (
          <Badge variant="default">Authoritative</Badge>
        ) : (
          <Badge variant="outline">Relay</Badge>
        ),
    },
    { header: "Node", cell: (row) => dash(row.node) },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Matches & Matchmaking"
        description="Live realtime matches on this node. Select one to read its members and tick, or terminate it."
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder="Search matches, labels or nodes"
          label="Search matches"
          className="sm:flex-1"
        />
        <FilterPills
          filters={FILTERS}
          value={filter}
          onChange={setFilter}
          className="sm:ml-auto"
        />
      </div>

      <Panel fill>
        <DataTable
          columns={columns}
          rows={filtered}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.api_match.match_id}
          onRowClick={(row) => setSelectedId(row.api_match.match_id)}
          selectedKey={selectedId ?? undefined}
          empty={empty}
          fill
          selectable
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={rows?.length ?? 0} onChange={setOffset} />
      </Panel>

      <DetailSheet
        open={selected !== null}
        onClose={() => setSelectedId(null)}
        title="Match detail"
        description={selected ? shortId(selected.api_match.match_id) : undefined}
        bodyClassName="space-y-4 px-5"
      >
        {selected !== null && (
          <MatchDetail key={selected.api_match.match_id} match={selected} onDeleted={() => setSelectedId(null)} />
        )}
      </DetailSheet>
    </div>
  )
}
