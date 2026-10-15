// A player's notifications. The console cannot list every notification (they
// belong to an account), so the view asks for one account id first.

import { useState, type FormEvent } from "react"
import { Search } from "lucide-react"

import { Button } from "@/components/ui/button"
import { DataTable, JsonBlock, Mono, PageHeading, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleNotification } from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export function NotificationsView() {
  const [draft, setDraft] = useState("")
  const [userId, setUserId] = useState<string | null>(null)

  const list = useConsoleQuery<ConsoleNotification[]>(
    "notifications",
    CONSOLE_ENDPOINTS.notifications,
    { user_id: userId ?? "", limit: 50 },
    userId !== null,
  )

  function submit(event: FormEvent) {
    event.preventDefault()
    setUserId(UUID.test(draft.trim()) ? draft.trim() : null)
  }

  const columns: Column<ConsoleNotification>[] = [
    { header: "Subject", cell: (row) => <span className="font-medium text-foreground">{row.subject}</span> },
    { header: "Code", cell: (row) => <Mono>{row.code}</Mono> },
    { header: "Sender", cell: (row) => <Mono>{shortId(row.sender_id)}</Mono> },
    { header: "Sent", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="Notifications"
        description="Subject, content and delivery time for one account's notifications."
      />

      <form onSubmit={submit} className="flex flex-wrap items-end gap-2">
        <label className="space-y-1.5">
          <span className="block text-xs font-medium text-muted-foreground">Account id</span>
          <input
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder="00000000-0000-0000-0000-000000000000"
            className="h-8 w-80 rounded-md border border-border bg-background px-3 font-mono text-xs outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
          />
        </label>
        <Button type="submit" size="sm" disabled={draft.trim() === ""}>
          <Search />
          Look up
        </Button>
      </form>

      <Panel title="Notifications" description={userId === null ? "Enter an account id" : shortId(userId)}>
        {userId === null ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">
            Notifications are per account. Enter an id to list them.
          </p>
        ) : (
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            empty="No notifications for this account."
          />
        )}
      </Panel>

      {userId !== null && list.data !== undefined && list.data.length > 0 && (
        <Panel title="Content">
          <div className="space-y-3 p-3">
            {list.data.map((row) => (
              <div key={row.id}>
                <p className="mb-1 text-xs font-medium text-foreground">{row.subject}</p>
                <JsonBlock value={row.content} />
              </div>
            ))}
          </div>
        </Panel>
      )}
    </div>
  )
}
