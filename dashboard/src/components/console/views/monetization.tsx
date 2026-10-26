// Purchases and subscriptions: the IAP ledger, newest first, with the vendor
// validation panel and the product-level subscription cancel.

import { useEffect, useState } from "react"
import { Ban } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ConfirmAction } from "@/components/console/actions"
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
import { VendorConfigurationPanel, VendorValidationPanel } from "@/components/console/views/monetization-iap"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import {
  CONSOLE_ENDPOINTS,
  type ConsolePurchase,
  type ConsoleSubscription,
} from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"

const PAGE_SIZE = 25

type EnvFilter = "all" | "sandbox" | "production"
type StateFilter = "all" | "active" | "expired"

const ENV_FILTERS: { id: EnvFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "sandbox", label: "Sandbox" },
  { id: "production", label: "Production" },
]

const STATE_FILTERS: { id: StateFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "active", label: "Active" },
  { id: "expired", label: "Expired" },
]

/** `1` sandbox, `2` production, anything else unknown — the schema's comment. */
function environment(env: number): string {
  if (env === 1) return "SANDBOX"
  if (env === 2) return "PROD"
  return "UNKNOWN"
}

export function PurchasesView() {
  const [offset, setOffset] = useState(0)
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<EnvFilter>("all")
  const list = useConsoleQuery<ConsolePurchase[]>("purchases", CONSOLE_ENDPOINTS.purchases, {
    limit: PAGE_SIZE,
    offset,
  })

  useEffect(() => {
    setOffset(0)
  }, [query, filter])

  const needle = query.trim().toLowerCase()
  const filtered = (list.data ?? []).filter((row) => {
    if (filter === "sandbox" && row.environment !== 1) return false
    if (filter === "production" && row.environment !== 2) return false
    if (needle === "") return true
    return [row.product_id, row.transaction_id, row.user_id, environment(row.environment)].some(
      (value) => value.toLowerCase().includes(needle),
    )
  })

  const empty =
    list.data !== undefined && list.data.length > 0
      ? "No purchases match these filters on this page."
      : "No purchases recorded."

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
    <div className="flex min-h-0 flex-1 flex-col gap-5">
      <PageHeading
        title="Purchases"
        description="Verified receipts, store validation and the credentials each vendor is configured with."
      />

      <Tabs defaultValue="ledger" className="flex min-h-0 flex-1 flex-col">
        <TabsList className="h-12 w-full justify-start gap-1.5">
          <TabsTrigger value="ledger">Purchases</TabsTrigger>
          <TabsTrigger value="validation">Vendor validation</TabsTrigger>
          <TabsTrigger value="configuration">Vendor configuration</TabsTrigger>
        </TabsList>

        <TabsContent value="ledger" keepMounted className="flex min-h-0 flex-1 flex-col gap-3 pt-2">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
            <SearchField
              value={query}
              onChange={setQuery}
              placeholder="Search purchases by product, transaction or account"
              label="Search purchases"
              className="sm:flex-1"
            />
            <FilterPills
              filters={ENV_FILTERS}
              value={filter}
              onChange={setFilter}
              className="sm:ml-auto"
            />
          </div>

          <Panel fill>
            <DataTable
              columns={columns}
              rows={filtered}
              loading={list.isPending}
              error={queryError(list.error)}
              rowKey={(row) => row.transaction_id}
              empty={empty}
              selectable
              fill
            />
            <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
          </Panel>
        </TabsContent>

        <TabsContent value="validation" keepMounted className="pt-2">
          <VendorValidationPanel />
        </TabsContent>

        <TabsContent value="configuration" keepMounted className="pt-2">
          <VendorConfigurationPanel />
        </TabsContent>
      </Tabs>
    </div>
  )
}

export function SubscriptionsView() {
  const [offset, setOffset] = useState(0)
  // The product the cancel dialog points at: one hook for the whole table, with
  // each row's id passed in as variables.
  const [cancelling, setCancelling] = useState<ConsoleSubscription | null>(null)
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<StateFilter>("all")
  const list = useConsoleQuery<ConsoleSubscription[]>("subscriptions", CONSOLE_ENDPOINTS.subscriptions, {
    limit: PAGE_SIZE,
    offset,
  })

  useEffect(() => {
    setOffset(0)
  }, [query, filter])

  const needle = query.trim().toLowerCase()
  const filtered = (list.data ?? []).filter((row) => {
    if (filter === "active" && !row.active) return false
    if (filter === "expired" && row.active) return false
    if (needle === "") return true
    return [
      row.product_id,
      row.original_transaction_id,
      row.user_id,
      row.active ? "active" : "expired",
    ].some((value) => value.toLowerCase().includes(needle))
  })

  const empty =
    list.data !== undefined && list.data.length > 0
      ? "No subscriptions match these filters on this page."
      : "No subscriptions recorded."

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

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder="Search subscriptions by product, transaction or account"
          label="Search subscriptions"
          className="sm:flex-1"
        />
        <FilterPills
          filters={STATE_FILTERS}
          value={filter}
          onChange={setFilter}
          className="sm:ml-auto"
        />
      </div>

      <Panel fill>
        <DataTable
          columns={columns}
          rows={filtered}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.original_transaction_id}
          empty={empty}
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
