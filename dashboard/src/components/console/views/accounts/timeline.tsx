// One account's merged timeline: wallet, inventory, purchases, subscriptions,
// notifications and sent messages in one ordered feed. The cursor comes back
// from the server, so "older" keeps its place even while new events land.

import { useState } from "react"

import { Button } from "@/components/ui/button"
import { JsonBlock, Mono, Panel } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleTimelinePage } from "@/lib/console"
import { formatTime } from "@/lib/format"

const KIND_LABEL: Record<string, string> = {
  wallet: "Wallet",
  inventory: "Inventory",
  purchase: "Purchase",
  subscription: "Subscription",
  notification: "Notification",
  message: "Message",
}

const KIND_CLASS: Record<string, string> = {
  wallet: "text-emerald-300",
  purchase: "text-sky-300",
  subscription: "text-violet-300",
  inventory: "text-amber-300",
  notification: "text-muted-foreground",
  message: "text-muted-foreground",
}

export function AccountTimeline({ id }: { id: string }) {
  const [cursor, setCursor] = useState<string | null>(null)
  const [pages, setPages] = useState<ConsoleTimelinePage["entries"]>([])
  const [openDetail, setOpenDetail] = useState<string | null>(null)

  const page = useConsoleQuery<ConsoleTimelinePage>(
    "timeline",
    `${CONSOLE_ENDPOINTS.timeline}/${id}`,
    cursor === null ? { limit: 25 } : { limit: 25, before: cursor },
  )

  const entries = [...pages, ...(page.data?.entries ?? [])]
  const nextCursor = page.data?.next_cursor ?? null

  function older() {
    if (page.data !== undefined) {
      setPages((previous) => [...previous, ...(page.data?.entries ?? [])])
      setCursor(page.data?.next_cursor ?? null)
    }
  }

  return (
    <Panel title="Timeline">
      {page.isError ? (
        <p className="px-4 py-6 text-center text-sm text-red-400">{queryError(page.error)}</p>
      ) : entries.length === 0 ? (
        <p className="px-4 py-6 text-center text-sm text-muted-foreground">
          Nothing has happened on this account yet.
        </p>
      ) : (
        <div className="divide-y divide-border/40">
          {entries.map((entry) => (
            <div key={`${entry.kind}-${entry.id}-${entry.at}`} className="px-4 py-3">
              <button
                type="button"
                className="flex w-full items-center justify-between gap-3 text-left"
                onClick={() =>
                  setOpenDetail(openDetail === entry.id ? null : entry.id)
                }
              >
                <div className="flex min-w-0 items-center gap-3">
                  <Mono className={`w-24 shrink-0 text-xs ${KIND_CLASS[entry.kind] ?? ""}`}>
                    {KIND_LABEL[entry.kind] ?? entry.kind}
                  </Mono>
                  <span className="truncate text-sm text-foreground">{entry.summary}</span>
                </div>
                <Mono className="shrink-0 text-xs text-muted-foreground">
                  {formatTime(entry.at)}
                </Mono>
              </button>
              {openDetail === entry.id && (
                <div className="mt-2">
                  <JsonBlock value={entry.detail} />
                </div>
              )}
            </div>
          ))}
        </div>
      )}
      {nextCursor !== null && (
        <div className="flex justify-center border-t border-border/40 py-3">
          <Button size="sm" variant="outline" onClick={older} disabled={page.isFetching}>
            Older
          </Button>
        </div>
      )}
    </Panel>
  )
}
