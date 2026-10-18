// Groups, and the members and roles of the selected one.

import { useState } from "react"
import { Download, Loader2, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Field, JsonBlock, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import {
  CONSOLE_ENDPOINTS,
  consoleGet,
  downloadJson,
  type ConsoleGroup,
  type ConsoleGroupExport,
  type ConsoleGroupMember,
} from "@/lib/console"
import { useSession } from "@/lib/session"
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
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const list = useConsoleQuery<ConsoleGroup[]>("groups", CONSOLE_ENDPOINTS.groups, {
    limit: PAGE_SIZE,
    offset,
  })

  // The row is looked back up rather than held, so a delete leaves nothing stale
  // behind: the id stops matching and the detail pane falls back to "nothing
  // selected" as soon as the list refreshes.
  const selected = list.data?.find((group) => group.id === selectedId) ?? null

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

      <Panel title="All groups">
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          onRowClick={(row) => setSelectedId(row.id)}
          selectedKey={selectedId ?? undefined}
          empty="No groups yet."
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>

      <DetailSheet
        open={selected !== null}
        onClose={() => setSelectedId(null)}
        title={selected?.name ?? "Group"}
        description={selected ? shortId(selected.id) : undefined}
        bodyClassName="space-y-4 px-5"
      >
        {selected !== null && (
          <GroupDetail key={selected.id} group={selected} onDeleted={() => setSelectedId(null)} />
        )}
      </DetailSheet>
    </div>
  )
}

function GroupDetail({ group, onDeleted }: { group: ConsoleGroup; onDeleted: () => void }) {
  const token = useSession()?.token ?? ""
  const [deleting, setDeleting] = useState(false)
  const [exporting, setExporting] = useState(false)
  const members = useConsoleQuery<ConsoleGroupMember[]>("group-members", `${CONSOLE_ENDPOINTS.groups}/${group.id}/member`)

  /** The group export carries the row and its members, saved as a file the operator
   * can carry elsewhere — the same round trip the account export supports. */
  async function exportGroup() {
    setExporting(true)
    try {
      const document_ = await consoleGet<ConsoleGroupExport>(
        `${CONSOLE_ENDPOINTS.groups}/${encodeURIComponent(group.id)}/export`,
        token,
      )
      downloadJson(`group-${group.name || shortId(group.id)}.json`, document_)
      toast.success("Exported.")
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setExporting(false)
    }
  }

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
      <Panel
        title={group.name}
        description={shortId(group.id)}
        actions={
          <div className="flex shrink-0 gap-1.5">
            <Button size="sm" variant="outline" onClick={exportGroup} disabled={exporting}>
              {exporting ? <Loader2 className="animate-spin" /> : <Download />}
              Export
            </Button>
            <Button size="sm" variant="destructive" onClick={() => setDeleting(true)}>
              <Trash2 />
              Delete
            </Button>
          </div>
        }
      >
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

      <ConfirmAction
        open={deleting}
        onClose={() => setDeleting(false)}
        onDone={onDeleted}
        title="Delete group"
        message="The group and every membership go; the members' accounts are untouched."
        confirmLabel="Delete group"
        destructive
        method="DELETE"
        path={`${CONSOLE_ENDPOINTS.groups}/${group.id}`}
        success="Group deleted."
      />
    </div>
  )
}
