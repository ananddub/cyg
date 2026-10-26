// A player's notifications. The console cannot list every notification (they
// belong to an account), so the view asks for one account id first.

import { useState, type FormEvent } from "react"
import { Search, Send, Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import {
  DataTable,
  JsonBlock,
  Mono,
  PageHeading,
  Panel,
  SearchField,
} from "@/components/console/data-table"
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
  const [query, setQuery] = useState("")

  const list = useConsoleQuery<ConsoleNotification[]>(
    "notifications",
    CONSOLE_ENDPOINTS.notifications,
    { user_id: userId ?? "", limit: 50 },
    userId !== null,
  )

  // Read back from the list, so deleting or refetching the opened row updates or
  // closes the drawer rather than leaving a captured copy on screen.
  const opened = list.data?.find((row) => row.id === openId) ?? null

  const needle = query.trim().toLowerCase()
  const filtered = (list.data ?? []).filter((row) => {
    if (needle === "") return true
    return [row.subject, String(row.code), row.sender_id].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  const empty =
    list.data !== undefined && list.data.length > 0
      ? "No notifications match this search."
      : "No notifications for this account."

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
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <PageHeading
            title="Notifications"
            description="Subject, content and delivery time for one account's notifications."
          />
        </div>
        <Button
          className="h-9 shrink-0 gap-1.5"
          disabled={userId === null}
          onClick={() => setSending(true)}
        >
          <Send className="size-3.5" />
          Send
        </Button>
      </div>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <form onSubmit={submit} className="flex w-full shrink-0 items-center gap-2 sm:max-w-md">
          <input
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder="Account id (UUID)"
            aria-label="Account id"
            spellCheck={false}
            autoComplete="off"
            className="h-9 w-full rounded-lg border border-border bg-card px-3 font-mono text-xs outline-none transition-colors hover:border-foreground/25 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
          />
          <Button type="submit" className="h-9 shrink-0 gap-1.5" disabled={draft.trim() === ""}>
            <Search className="size-3.5" />
            Look up
          </Button>
        </form>

        {userId !== null && (
          <SearchField
            value={query}
            onChange={setQuery}
            placeholder="Search notifications by subject, code or sender"
            label="Search notifications"
            className="sm:flex-1"
          />
        )}
      </div>

      <Panel
        title="Notifications"
        description={userId === null ? "Enter an account id" : shortId(userId)}
        fill
      >
        {userId === null ? (
          <p className="flex flex-1 items-center justify-center px-4 py-6 text-center text-sm text-muted-foreground">
            Notifications are per account. Enter an id to list them.
          </p>
        ) : (
          <DataTable
            columns={columns}
            rows={filtered}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            onRowClick={(row) => setOpenId(row.id)}
            selectedKey={openId ?? undefined}
            empty={empty}
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

