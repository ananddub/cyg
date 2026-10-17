// The audit trail: every administrative action, newest first. Read-only by
// design — a trail an operator could edit would be worth nothing.

import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { DataTable, JsonBlock, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleAuditLog } from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"

const PAGE_SIZE = 25

export function AuditLogView() {
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState<ConsoleAuditLog | null>(null)
  const list = useConsoleQuery<ConsoleAuditLog[]>("audit", CONSOLE_ENDPOINTS.auditLog, {
    limit: PAGE_SIZE,
    offset,
  })

  const columns: Column<ConsoleAuditLog>[] = [
    { header: "When", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
    { header: "Operator", cell: (row) => <span className="text-foreground">{row.console_username}</span> },
    { header: "Action", cell: (row) => <Badge variant="outline">{row.action}</Badge> },
    { header: "Resource", cell: (row) => <Mono>{row.resource}</Mono> },
    { header: "Message", cell: (row) => <span className="text-muted-foreground">{row.message}</span> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading title="Audit Log" description="Every administrative action, newest first." />
      <Panel title="Entries">
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          onRowClick={(row) => setSelected(row)}
          selectedKey={selected?.id}
          empty="No audit entries yet."
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>

      {selected !== null && (
        <Panel title="Entry" description={`${selected.action} · ${shortId(selected.id)}`}>
          <div className="p-3">
            <JsonBlock value={selected.metadata} />
          </div>
        </Panel>
      )}
    </div>
  )
}
