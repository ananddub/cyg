// Purchases and subscriptions: the IAP ledger, newest first, with the vendor
// validation panel and the product-level subscription cancel.

import { useState } from "react"
import { Ban } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { IapPanel } from "@/components/console/views/monetization-iap"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import {
  CONSOLE_ENDPOINTS,
  type ConsolePurchase,
  type ConsoleSubscription,
} from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"

const PAGE_SIZE = 25

/** `1` sandbox, `2` production, anything else unknown — the schema's comment. */
function environment(env: number): string {
  if (env === 1) return "SANDBOX"
  if (env === 2) return "PROD"
  return "UNKNOWN"
}

export function PurchasesView() {
  const [offset, setOffset] = useState(0)
  const list = useConsoleQuery<ConsolePurchase[]>("purchases", CONSOLE_ENDPOINTS.purchases, {
    limit: PAGE_SIZE,
    offset,
  })

  const columns: Column<ConsolePurchase>[] = [
    { header: "Product", cell: (row) => <span className="font-medium text-foreground">{row.product_id}</span> },
    { header: "Transaction", cell: (row) => <Mono>{shortId(row.transaction_id)}</Mono> },
    { header: "Account", cell: (row) => <Mono>{shortId(row.user_id)}</Mono> },
    { header: "Store", cell: (row) => <Mono>#{row.store}</Mono> },
    { header: "Env", cell: (row) => <Badge variant="outline">{environment(row.environment)}</Badge> },
    { header: "Purchased", cell: (row) => <Mono>{formatTime(row.purchase_time)}</Mono> },
    { header: "Refunded", cell: (row) => <Mono>{formatTime(row.refund_time)}</Mono> },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading title="Purchases" description="Every verified in-app purchase receipt." />
      <Panel title="Purchases" fill>
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.transaction_id}
          empty="No purchases recorded."
          fill
          selectable
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>

      <IapPanel />
    </div>
  )
}

export function SubscriptionsView() {
  const [offset, setOffset] = useState(0)
  // The product the cancel dialog points at: one hook for the whole table, with
  // each row's id passed in as variables.
  const [cancelling, setCancelling] = useState<ConsoleSubscription | null>(null)
  const list = useConsoleQuery<ConsoleSubscription[]>("subscriptions", CONSOLE_ENDPOINTS.subscriptions, {
    limit: PAGE_SIZE,
    offset,
  })

  const columns: Column<ConsoleSubscription>[] = [
    { header: "Product", cell: (row) => <span className="font-medium text-foreground">{row.product_id}</span> },
    { header: "Transaction", cell: (row) => <Mono>{shortId(row.original_transaction_id)}</Mono> },
    { header: "Account", cell: (row) => <Mono>{shortId(row.user_id)}</Mono> },
    { header: "Store", cell: (row) => <Mono>#{row.store}</Mono> },
    { header: "Env", cell: (row) => <Badge variant="outline">{environment(row.environment)}</Badge> },
    { header: "Expires", cell: (row) => <Mono>{formatTime(row.expire_time)}</Mono> },
    {
      header: "State",
      cell: (row) => (row.active ? <Badge variant="secondary">Active</Badge> : <Badge variant="outline">Expired</Badge>),
    },
    {
      header: "Actions",
      cell: (row) => (
        <Button variant="ghost" size="icon-xs" aria-label={`Cancel ${row.product_id}`} onClick={() => setCancelling(row)}>
          <Ban />
        </Button>
      ),
    },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading title="Subscriptions" description="Recurring purchases and their expiry state." />
      <Panel title="Subscriptions" fill>
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.original_transaction_id}
          empty="No subscriptions recorded."
          fill
          selectable
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>

      <ConfirmAction
        open={cancelling !== null}
        onClose={() => setCancelling(null)}
        onDone={() => setCancelling(null)}
        title="Cancel subscription product"
        message={`Every subscription row for "${cancelling?.product_id ?? ""}" goes — a product-level cancel. Players must re-purchase to get it back.`}
        confirmLabel="Cancel product"
        destructive
        method="DELETE"
        // DELETE /v2/console/subscription/{product_id} answers Empty ({}), so the
        // refresh rides the shared invalidation; the id comes from the row state.
        path={`${CONSOLE_ENDPOINTS.subscriptions}/${encodeURIComponent(cancelling?.product_id ?? "")}`}
        success="Subscriptions for the product cancelled."
      />
    </div>
  )
}
