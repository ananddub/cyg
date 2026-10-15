// Console administration: the operators, the audit trail, the ACL templates and
// the server settings.

import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { DataTable, JsonBlock, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import {
  CONSOLE_ENDPOINTS,
  type ConsoleAclTemplate,
  type ConsoleAuditLog,
  type ConsoleSetting,
  type ConsoleUser,
} from "@/lib/console"
import { dash, formatTime, shortId } from "@/lib/format"

const PAGE_SIZE = 25

/** The role badge tone: the wider the access, the louder the badge. */
const ROLE_VARIANT: Record<string, "default" | "secondary" | "outline" | "destructive"> = {
  admin: "destructive",
  developer: "default",
  maintainer: "secondary",
  readonly: "outline",
}

export function ConsoleUsersView() {
  const [offset, setOffset] = useState(0)
  const list = useConsoleQuery<ConsoleUser[]>("console-users", CONSOLE_ENDPOINTS.users, {
    limit: PAGE_SIZE,
    offset,
  })

  const columns: Column<ConsoleUser>[] = [
    { header: "Username", cell: (row) => <span className="font-medium text-foreground">{row.username}</span> },
    { header: "Email", cell: (row) => <Mono>{dash(row.email)}</Mono> },
    { header: "Role", cell: (row) => <Badge variant={ROLE_VARIANT[row.role] ?? "outline"}>{row.role}</Badge> },
    { header: "MFA", cell: (row) => <Mono>{row.mfa_required ? "required" : "off"}</Mono> },
    {
      header: "Status",
      cell: (row) => (row.disabled ? <Badge variant="destructive">Disabled</Badge> : <Badge variant="secondary">Active</Badge>),
    },
    { header: "Created", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="Console Users"
        description="Operators who may sign in to this console, and the role each one holds."
      />
      <Panel title="Operators">
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          empty="No console users besides the bootstrap account."
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>
    </div>
  )
}

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

export function AclTemplatesView() {
  const list = useConsoleQuery<ConsoleAclTemplate[]>("acl", CONSOLE_ENDPOINTS.aclTemplates)
  const [selected, setSelected] = useState<ConsoleAclTemplate | null>(null)

  const columns: Column<ConsoleAclTemplate>[] = [
    { header: "Name", cell: (row) => <span className="font-medium text-foreground">{row.name}</span> },
    { header: "Description", cell: (row) => dash(row.description) },
    { header: "Updated", cell: (row) => <Mono>{formatTime(row.update_time)}</Mono> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="ACL Templates"
        description="Named permission documents a console role can be created from."
      />
      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_24rem]">
        <Panel title="Templates">
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            onRowClick={(row) => setSelected(row)}
            selectedKey={selected?.id}
            empty="No ACL templates defined."
          />
        </Panel>

        {selected === null ? (
          <Panel title="Permissions" description="No template selected">
            <p className="px-4 py-6 text-sm text-muted-foreground">Select a template to read its permissions.</p>
          </Panel>
        ) : (
          <Panel title={selected.name} description={shortId(selected.id)}>
            <div className="p-3">
              <JsonBlock value={selected.acl} />
            </div>
          </Panel>
        )}
      </div>
    </div>
  )
}

export function SettingsView() {
  const list = useConsoleQuery<ConsoleSetting[]>("settings", CONSOLE_ENDPOINTS.settings)
  const [selected, setSelected] = useState<ConsoleSetting | null>(null)

  const columns: Column<ConsoleSetting>[] = [
    { header: "Name", cell: (row) => <span className="font-mono text-xs font-medium text-foreground">{row.name}</span> },
    { header: "Updated", cell: (row) => <Mono>{formatTime(row.update_time)}</Mono> },
    {
      header: "Value",
      cell: (row) => {
        const text = JSON.stringify(row.value)
        return <span className="text-muted-foreground">{text !== undefined && text.length > 60 ? `${text.slice(0, 60)}…` : text}</span>
      },
    },
  ]

  return (
    <div className="space-y-6">
      <PageHeading title="Settings" description="Server-side key/value settings. Read-only for now." />
      <Panel title="Settings">
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.name}
          onRowClick={(row) => setSelected(row)}
          selectedKey={selected?.name}
          empty="No settings recorded."
        />
      </Panel>

      {selected !== null && (
        <Panel title={selected.name}>
          <div className="p-3">
            <JsonBlock value={selected.value} />
          </div>
        </Panel>
      )}
    </div>
  )
}
