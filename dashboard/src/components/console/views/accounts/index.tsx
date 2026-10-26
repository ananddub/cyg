// Player accounts: the directory, paged, with the selected row's detail in a
// right-side drawer and the import dialog below. Every write on a row flows
// through the detail, not this list.

import { useEffect, useState } from "react"
import { Plus } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Mono } from "@/components/console/data-table"
import {
  DataTable,
  FilterPills,
  PageHeading,
  Pager,
  Panel,
  SearchField,
} from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleAccount } from "@/lib/console"
import { dash, formatTime, num, shortId } from "@/lib/format"

import { AccountDetail } from "./detail"
import { ImportDialog } from "./import-dialog"

const PAGE_SIZE = 25
const ACCOUNT = CONSOLE_ENDPOINTS.accounts

type StatusFilter = "all" | "active" | "disabled"

const FILTERS: { id: StatusFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "active", label: "Active" },
  { id: "disabled", label: "Disabled" },
]

export function AccountsView() {
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState<string | null>(null)
  const [importing, setImporting] = useState(false)
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<StatusFilter>("all")

  const list = useConsoleQuery<ConsoleAccount[]>("accounts", ACCOUNT, {
    limit: PAGE_SIZE,
    offset,
  })

  useEffect(() => {
    setOffset(0)
  }, [query, filter])

  const needle = query.trim().toLowerCase()
  const filtered = (list.data ?? []).filter((row) => {
    const disabled = new Date(row.disable_time).getTime() > 0
    if (filter === "active" && disabled) return false
    if (filter === "disabled" && !disabled) return false
    if (needle === "") return true
    return [row.username, row.display_name ?? "", row.email ?? "", disabled ? "disabled" : "active"].some(
      (value) => value.toLowerCase().includes(needle),
    )
  })

  const empty =
    list.data !== undefined && list.data.length > 0
      ? "No accounts match these filters on this page."
      : "No accounts yet."

  const columns: Column<ConsoleAccount>[] = [
    { header: "Username", cell: (row) => <span className="font-medium text-foreground">{row.username}</span> },
    { header: "Display name", cell: (row) => dash(row.display_name) },
    { header: "Email", cell: (row) => <Mono>{dash(row.email)}</Mono> },
    { header: "Groups", cell: (row) => <Mono>{num(row.edge_count)}</Mono> },
    { header: "Created", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
    {
      header: "Status",
      cell: (row) =>
        new Date(row.disable_time).getTime() > 0 ? (
          <Badge variant="destructive">Disabled</Badge>
        ) : (
          <Badge variant="secondary">Active</Badge>
        ),
    },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <PageHeading
            title="Accounts"
            description="Player accounts. Select one to read its notes, wallet ledger and provider links — or to ban it, note it and adjust its wallet."
          />
        </div>
        <Button className="h-9 shrink-0 gap-1.5" onClick={() => setImporting(true)}>
          <Plus className="size-3.5" />
          Import
        </Button>
      </div>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder="Search accounts by name, display name or email"
          label="Search accounts"
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
          rowKey={(row) => row.id}
          onRowClick={(row) => setSelected(row.id)}
          selectedKey={selected ?? undefined}
          empty={empty}
          fill
          selectable
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>

      <DetailSheet
        open={selected !== null}
        onClose={() => setSelected(null)}
        title="Account detail"
        description={selected ? shortId(selected) : undefined}
        bodyClassName="space-y-4 px-5"
      >
        {selected !== null && (
          <AccountDetail key={selected} id={selected} onDeleted={() => setSelected(null)} />
        )}
      </DetailSheet>

      <ImportDialog open={importing} onClose={() => setImporting(false)} />
    </div>
  )
}
