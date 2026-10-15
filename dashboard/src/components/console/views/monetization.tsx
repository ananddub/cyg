// Purchases and subscriptions: the IAP ledger, newest first.

import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { DataTable, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
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
    <div className="space-y-6">
      <PageHeading title="Purchases" description="Every verified in-app purchase receipt." />
      <Panel title="Purchases">
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.transaction_id}
          empty="No purchases recorded."
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>
    </div>
  )
}

export function SubscriptionsView() {
  const [offset, setOffset] = useState(0)
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
  ]

  return (
    <div className="space-y-6">
      <PageHeading title="Subscriptions" description="Recurring purchases and their expiry state." />
      <Panel title="Subscriptions">
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.original_transaction_id}
          empty="No subscriptions recorded."
        />
        <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
      </Panel>
    </div>
  )
}
