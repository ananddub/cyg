// A player's notifications. The console cannot list every notification (they
// belong to an account), so the view asks for one account id first.

import { useState, type FormEvent } from "react"
import { Search, Send, Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, JsonBlock, Mono, PageHeading, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleNotification } from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"

import { SendNotificationDialog } from "./notifications-send"

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export function NotificationsView() {
  const [draft, setDraft] = useState("")
  const [userId, setUserId] = useState<string | null>(null)
  const [sending, setSending] = useState(false)
  const [deleteId, setDeleteId] = useState<string | null>(null)
  const [openId, setOpenId] = useState<string | null>(null)

  const list = useConsoleQuery<ConsoleNotification[]>(
    "notifications",
    CONSOLE_ENDPOINTS.notifications,
    { user_id: userId ?? "", limit: 50 },
    userId !== null,
  )

  // Read back from the list, so deleting or refetching the opened row updates or
  // closes the drawer rather than leaving a captured copy on screen.
  const opened = list.data?.find((row) => row.id === openId) ?? null

  function submit(event: FormEvent) {
    event.preventDefault()
    setUserId(UUID.test(draft.trim()) ? draft.trim() : null)
    setOpenId(null)
  }

  const columns: Column<ConsoleNotification>[] = [
    { header: "Subject", cell: (row) => <span className="font-medium text-foreground">{row.subject}</span> },
    { header: "Code", cell: (row) => <Mono>{row.code}</Mono> },
    { header: "Sender", cell: (row) => <Mono>{shortId(row.sender_id)}</Mono> },
    { header: "Sent", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
    {
      header: "Actions",
      cell: (row) => (
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Delete notification"
          onClick={(event) => {
            event.stopPropagation()
            setDeleteId(row.id)
          }}
        >
          <Trash2 />
        </Button>
      ),
    },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Notifications"
        description="Subject, content and delivery time for one account's notifications."
      />

      <form onSubmit={submit} className="flex shrink-0 flex-wrap items-end gap-2">
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

      <Panel
        title="Notifications"
        description={userId === null ? "Enter an account id" : shortId(userId)}
        fill
        actions={
          <Button size="sm" variant="outline" disabled={userId === null} onClick={() => setSending(true)}>
            <Send />
            Send
          </Button>
        }
      >
        {userId === null ? (
          <p className="flex flex-1 items-center justify-center px-4 py-6 text-center text-sm text-muted-foreground">
            Notifications are per account. Enter an id to list them.
          </p>
        ) : (
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            onRowClick={(row) => setOpenId(row.id)}
            selectedKey={openId ?? undefined}
            empty="No notifications for this account."
            fill
            selectable
          />
        )}
      </Panel>

      <DetailSheet
        open={opened !== null}
        onClose={() => setOpenId(null)}
        title={opened?.subject ?? "Notification"}
        description={opened ? `${opened.code} · ${formatTime(opened.create_time)}` : undefined}
      >
        {opened !== null && <JsonBlock value={opened.content} />}
      </DetailSheet>

      {userId !== null && (
        <SendNotificationDialog userId={userId} open={sending} onClose={() => setSending(false)} />
      )}

      {deleteId !== null && (
        <ConfirmAction
          open
          onClose={() => setDeleteId(null)}
          title="Delete notification"
          message="The notification is removed from the account's inbox. This cannot be undone."
          confirmLabel="Delete"
          destructive
          method="DELETE"
          path={`${CONSOLE_ENDPOINTS.notifications}/${encodeURIComponent(deleteId)}`}
          success="Notification deleted."
        />
      )}
    </div>
  )
}

