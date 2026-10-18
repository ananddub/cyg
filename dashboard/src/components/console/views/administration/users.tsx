// Console operators: who may sign in and the role each one holds. The acts an
// admin performs on one live in the detail pane (./user-detail); the creation form
// is in ./user-create-dialog.

import { useState } from "react"
import { Plus } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  DataTable,
  Mono,
  PageHeading,
  Pager,
  Panel,
} from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleUser } from "@/lib/console"
import { roleVariant } from "@/lib/console/roles"
import { dash, formatTime } from "@/lib/format"

import { CreateOperatorDialog } from "./user-create-dialog"
import { OperatorDetail } from "./user-detail"

const PAGE_SIZE = 25

export function ConsoleUsersView() {
  const [offset, setOffset] = useState(0)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)

  const list = useConsoleQuery<ConsoleUser[]>("console-users", CONSOLE_ENDPOINTS.users, {
    limit: PAGE_SIZE,
    offset,
  })
  // The row is read back from the list rather than held: an edit must show what
  // the server now holds, not the values that were clicked.
  const selected = list.data?.find((user) => user.id === selectedId) ?? null

  const columns: Column<ConsoleUser>[] = [
    { header: "Username", cell: (row) => <span className="font-medium text-foreground">{row.username}</span> },
    { header: "Email", cell: (row) => <Mono>{dash(row.email)}</Mono> },
    { header: "Role", cell: (row) => <Badge variant={roleVariant(row.role)}>{row.role}</Badge> },
    { header: "MFA", cell: (row) => <Mono>{row.mfa_required ? "required" : "off"}</Mono> },
    {
      header: "Status",
      cell: (row) =>
        row.disabled ? <Badge variant="destructive">Disabled</Badge> : <Badge variant="secondary">Active</Badge>,
    },
    { header: "Created", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="Console Users"
        description="Operators who may sign in to this console, and the role each one holds. The config bootstrap account is not listed — it is not a row."
      />

      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_26rem]">
        <Panel
          title="Operators"
          actions={
            <Button size="sm" variant="outline" onClick={() => setCreating(true)}>
              <Plus />
              New operator
            </Button>
          }
        >
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            onRowClick={(row) => setSelectedId(row.id)}
            selectedKey={selectedId ?? undefined}
            empty="No console users besides the bootstrap account."
          />
          <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
        </Panel>

        {selected === null ? (
          <Panel title="Operator" description="No operator selected">
            <p className="px-4 py-6 text-sm text-muted-foreground">Select a row to inspect it.</p>
          </Panel>
        ) : (
          <OperatorDetail key={selected.id} user={selected} onDeleted={() => setSelectedId(null)} />
        )}
      </div>

      <CreateOperatorDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  )
}
