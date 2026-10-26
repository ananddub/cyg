// Console operators: who may sign in and the role each one holds. The acts an
// admin performs on one live in the detail drawer (./user-detail); the creation
// form is in ./user-create-dialog.

import { useEffect, useState } from "react"
import { Plus } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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
import { CONSOLE_ENDPOINTS, type ConsoleUser } from "@/lib/console"
import { roleVariant } from "@/lib/console/roles"
import { dash, formatTime } from "@/lib/format"

import { CreateOperatorDialog } from "./user-create-dialog"
import { OperatorDetail } from "./user-detail"

const PAGE_SIZE = 25

type StatusFilter = "all" | "active" | "disabled"

const FILTERS: { id: StatusFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "active", label: "Active" },
  { id: "disabled", label: "Disabled" },
]

export function ConsoleUsersView() {
  const [offset, setOffset] = useState(0)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<StatusFilter>("all")

  const list = useConsoleQuery<ConsoleUser[]>("console-users", CONSOLE_ENDPOINTS.users, {
    limit: PAGE_SIZE,
    offset,
  })
  // The row is read back from the list rather than held: an edit must show what
  // the server now holds, not the values that were clicked.
  const selected = list.data?.find((user) => user.id === selectedId) ?? null

  // A search or a status pick is a fresh look at the list, so it starts at the
  // first page instead of carrying an offset into a different result set.
  useEffect(() => {
    setOffset(0)
  }, [query, filter])

  const needle = query.trim().toLowerCase()
  const filtered = (list.data ?? []).filter((user) => {
    if (filter === "active" && user.disabled) return false
    if (filter === "disabled" && !user.disabled) return false
    if (needle === "") return true
    const status = user.disabled ? "disabled" : "active"
    const mfa = user.mfa_required ? "required" : "off"
    return [user.username, user.email, user.role, status, mfa].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  const empty =
    list.data !== undefined && list.data.length > 0
      ? "No operators match these filters on this page."
      : "No console users besides the bootstrap account."

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
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <PageHeading
            title="Console Users"
            description="Operators who may sign in to this console, and the role each one holds. The config bootstrap account is not listed — it is not a row."
          />
        </div>
        <Button className="h-9 shrink-0 gap-1.5" onClick={() => setCreating(true)}>
          <Plus className="size-3.5" />
          New operator
        </Button>
      </div>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder="Search operators by name, email or role"
          label="Search operators"
          className="sm:flex-1"
        />
        <FilterPills
          filters={FILTERS}
          value={filter}
          onChange={setFilter}
          className="sm:ml-auto"
        />
      </div>

      <Panel title="Operators" fill>
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
        title={selected?.username ?? "Operator"}
        bodyClassName="space-y-4 px-5"
      >
        {selected !== null && (
          <OperatorDetail key={selected.id} user={selected} onDeleted={() => setSelectedId(null)} />
        )}
      </DetailSheet>

      <CreateOperatorDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  )
}
