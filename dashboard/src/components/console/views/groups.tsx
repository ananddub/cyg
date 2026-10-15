// Groups, and the members and roles of the selected one.

import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { DataTable, Field, JsonBlock, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleGroup, type ConsoleGroupMember } from "@/lib/console"
import { dash, formatTime, num, shortId } from "@/lib/format"

const PAGE_SIZE = 25

const STATE_VARIANT: Record<string, "default" | "secondary" | "outline" | "destructive"> = {
  superadmin: "default",
  admin: "default",
  member: "secondary",
  join_request: "outline",
  banned: "destructive",
}

export function GroupsView() {
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState<ConsoleGroup | null>(null)

  const list = useConsoleQuery<ConsoleGroup[]>("groups", CONSOLE_ENDPOINTS.groups, {
    limit: PAGE_SIZE,
    offset,
  })

  const columns: Column<ConsoleGroup>[] = [
    { header: "Name", cell: (row) => <span className="font-medium text-foreground">{row.name}</span> },
    { header: "Description", cell: (row) => dash(row.description) },
    { header: "Lang", cell: (row) => <Mono>{dash(row.lang_tag)}</Mono> },
    { header: "Members", cell: (row) => <Mono>{num(row.edge_count)}</Mono> },
    {
      header: "Visibility",
      cell: (row) =>
        row.open ? <Badge variant="secondary">Open</Badge> : <Badge variant="outline">Closed</Badge>,
    },
    { header: "Updated", cell: (row) => <Mono>{formatTime(row.update_time)}</Mono> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="Groups"
        description="Player groups and clans. Select one to list its members and their roles."
      />

      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_24rem]">
        <Panel title="All groups">
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            onRowClick={(row) => setSelected(row)}
            selectedKey={selected?.id}
            empty="No groups yet."
          />
          <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
        </Panel>

        {selected === null ? (
          <Panel title="Group detail" description="No group selected">
            <p className="px-4 py-6 text-sm text-muted-foreground">Select a row to inspect it.</p>
          </Panel>
        ) : (
          <GroupDetail group={selected} />
        )}
      </div>
    </div>
  )
}

function GroupDetail({ group }: { group: ConsoleGroup }) {
  const members = useConsoleQuery<ConsoleGroupMember[]>("group-members", `${CONSOLE_ENDPOINTS.groups}/${group.id}/member`)

  const columns: Column<ConsoleGroupMember>[] = [
    { header: "Member", cell: (row) => <span className="text-foreground">{row.username}</span> },
    {
      header: "Role",
      cell: (row) => <Badge variant={STATE_VARIANT[row.state] ?? "outline"}>{row.state}</Badge>,
    },
    { header: "Since", cell: (row) => <Mono>{formatTime(row.edge_update_time)}</Mono> },
  ]

  return (
    <div className="space-y-4">
      <Panel title={group.name} description={shortId(group.id)}>
        <div className="px-4 py-2">
          <Field label="Creator">{shortId(group.creator_id)}</Field>
          <Field label="Members">{`${num(group.edge_count)} / ${num(group.max_count)}`}</Field>
          <Field label="Language">{dash(group.lang_tag)}</Field>
          <Field label="Created">{formatTime(group.create_time)}</Field>
        </div>
      </Panel>

      <Panel title="Members" description={`${members.data?.length ?? 0} listed`}>
        <DataTable
          columns={columns}
          rows={members.data}
          loading={members.isPending}
          error={queryError(members.error)}
          rowKey={(row) => row.id}
          empty="No members."
        />
      </Panel>

      <Panel title="Metadata">
        <div className="p-3">
          <JsonBlock value={group.metadata} />
        </div>
      </Panel>
    </div>
  )
}
