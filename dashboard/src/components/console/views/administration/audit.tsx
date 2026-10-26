// The audit trail: every administrative action, newest first. Read-only by
// design — a trail an operator could edit would be worth nothing.

import { useEffect, useState } from "react"

import { Badge } from "@/components/ui/badge"
import {
  DataTable,
  JsonBlock,
  Mono,
  PageHeading,
  Pager,
  Panel,
  SearchField,
} from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { Select } from "@/components/console/form"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import {
  CONSOLE_ENDPOINTS,
  type ConsoleAuditLog,
  type ConsoleAuditLogUsers,
} from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"

const PAGE_SIZE = 25

export function AuditLogView() {
  const [offset, setOffset] = useState(0)
  const [username, setUsername] = useState("")
  const [query, setQuery] = useState("")
  const [selectedId, setSelectedId] = useState<string | null>(null)

  // The trail, narrowed by the operator filter. A new filter is a new first page —
  // otherwise the offset would carry into a different result set.
  const list = useConsoleQuery<ConsoleAuditLog[]>("audit", CONSOLE_ENDPOINTS.auditLog, {
    limit: PAGE_SIZE,
    offset,
    username,
  })
  // The names come from the trail itself, so the filter only offers operators who
  // actually have entries — never a name that would filter to nothing.
  const users = useConsoleQuery<ConsoleAuditLogUsers>(
    "audit-users",
    `${CONSOLE_ENDPOINTS.auditLog}/users`,
  )

  // The row is read back from the list rather than held: a refetch that no longer
  // carries the id closes the drawer instead of showing a stale entry.
  const selected = list.data?.find((entry) => entry.id === selectedId) ?? null

  const operatorOptions = [
    { value: "", label: "All operators" },
    ...(users.data?.usernames ?? []).map((name) => ({ value: name, label: name })),
  ]

  // A new search is a fresh look at the trail, so it starts at the first page.
  useEffect(() => {
    setOffset(0)
  }, [query])

  function filterByName(name: string) {
    setUsername(name)
    setOffset(0)
    setSelectedId(null)
  }

  const needle = query.trim().toLowerCase()
  const filtered = (list.data ?? []).filter((entry) => {
    if (needle === "") return true
    return [entry.console_username, entry.action, entry.resource, entry.message].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  const empty =
    list.data !== undefined && list.data.length > 0
      ? "No entries match this search on this page."
      : username === ""
        ? "No audit entries yet."
        : `No audit entries for ${username}.`

  const columns: Column<ConsoleAuditLog>[] = [
    { header: "When", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
    { header: "Operator", cell: (row) => <span className="text-foreground">{row.console_username}</span> },
    { header: "Action", cell: (row) => <Badge variant="outline">{row.action}</Badge> },
    { header: "Resource", cell: (row) => <Mono>{row.resource}</Mono> },
    { header: "Message", cell: (row) => <span className="text-muted-foreground">{row.message}</span> },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading title="Audit Log" description="Every administrative action, newest first." />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder="Search operator, action, resource or message"
          label="Search audit entries"
          className="sm:flex-1"
        />
        <div className="w-full shrink-0 sm:max-w-xs">
          <Select
            label="Operator"
            hideLabel
            value={username}
            onChange={filterByName}
            options={operatorOptions}
            searchable
          />
        </div>
      </div>

      <Panel fill>
        <DataTable
          columns={columns}
          rows={filtered}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          onRowClick={(row) => setSelectedId(row.id)}
          selectedKey={selectedId ?? undefined}
          empty={empty}
          fill
          selectable
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>

      <DetailSheet
        open={selected !== null}
        onClose={() => setSelectedId(null)}
        title={selected ? `${selected.action} · ${shortId(selected.id)}` : "Entry"}
        description={selected ? `${selected.console_username} · ${formatTime(selected.create_time)}` : undefined}
      >
        {selected !== null && <JsonBlock value={selected.metadata} />}
      </DetailSheet>
    </div>
  )
}
