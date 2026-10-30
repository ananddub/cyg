// The economy view: what the window earned, granted and traded, computed by
// the console aggregate rather than by the browser summing raw rows.

import { useMemo, useState } from "react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Mono, PageHeading, Panel } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleEconomySummary } from "@/lib/console"

function isoDay(offsetDays: number): string {
  const date = new Date()
  date.setUTCDate(date.getUTCDate() + offsetDays)
  return date.toISOString().slice(0, 10)
}

export function EconomyView() {
  const [from, setFrom] = useState(isoDay(-30))
  const [to, setTo] = useState(isoDay(0))
  const [window_, setWindow] = useState({ from: isoDay(-30), to: isoDay(0) })

  const summary = useConsoleQuery<ConsoleEconomySummary>(
    "economy-summary",
    CONSOLE_ENDPOINTS.economySummary,
    { from: window_.from, to: window_.to },
  )

  const currencies = useMemo(
    () => Object.entries(summary.data?.wallet.net_by_currency ?? {}),
    [summary.data],
  )

  return (
    <div className="flex flex-col gap-4">
      <PageHeading
        title="Economy"
        description="Sinks and sources inside one window: purchases, wallet movement, quest claims and marketplace flow. An empty window renders zeros, never an error."
      />
      <Panel
        title="Window"
        actions={
          <div className="flex items-center gap-2">
            <Input
              type="date"
              className="h-8 w-40"
              value={from}
              onChange={(event) => setFrom(event.target.value)}
            />
            <Input
              type="date"
              className="h-8 w-40"
              value={to}
              onChange={(event) => setTo(event.target.value)}
            />
            <Button
              size="sm"
              onClick={() => setWindow({ from, to })}
              disabled={from === "" || to === ""}
            >
              Apply
            </Button>
          </div>
        }
      >
        {summary.isError ? (
          <p className="px-4 py-6 text-center text-sm text-red-400">
            {queryError(summary.error)}
          </p>
        ) : summary.data === undefined ? (
          <p className="px-4 py-6 text-center text-sm text-muted-foreground">Loading…</p>
        ) : (
          <div className="grid grid-cols-2 gap-px bg-border/40 md:grid-cols-4">
            <Stat
              label="Purchases"
              value={summary.data.purchases.total}
              hint={`${summary.data.purchases.refunded} refunded`}
            />
            <Stat label="Wallet credits" value={summary.data.wallet.credits} />
            <Stat label="Wallet debits" value={summary.data.wallet.debits} />
            <Stat
              label="Quest claims"
              value={summary.data.quests.claimed}
              hint={`${summary.data.quests.completed} completed`}
            />
            <Stat
              label="Trades settled"
              value={summary.data.market.trades_settled}
              hint={`${summary.data.market.listings_created} listed`}
            />
            <Stat label="Wagers held" value={summary.data.market.wagers_held} />
            <Stat label="Wagers settled" value={summary.data.market.wagers_settled} />
            <Stat label="Wagers refunded" value={summary.data.market.wagers_refunded} />
            <Stat label="Desyncs" value={summary.data.desyncs} />
          </div>
        )}
      </Panel>

      <Panel title="Net wallet change by currency">
        <div className="divide-y divide-border/40">
          {currencies.length === 0 ? (
            <p className="px-4 py-6 text-center text-sm text-muted-foreground">
              No wallet movement in this window.
            </p>
          ) : (
            currencies.map(([code, net]) => (
              <div key={code} className="flex items-center justify-between px-4 py-3 text-sm">
                <Mono>{code}</Mono>
                <Mono className={net < 0 ? "text-red-400" : "text-emerald-300"}>
                  {net.toFixed(2)}
                </Mono>
              </div>
            ))
          )}
        </div>
      </Panel>

      <Panel title="Purchases by environment">
        <div className="divide-y divide-border/40">
          {Object.entries(summary.data?.purchases.by_environment ?? {}).length === 0 ? (
            <p className="px-4 py-6 text-center text-sm text-muted-foreground">
              No purchases in this window.
            </p>
          ) : (
            Object.entries(summary.data?.purchases.by_environment ?? {}).map(([env, count]) => (
              <div key={env} className="flex items-center justify-between px-4 py-3 text-sm">
                <Mono>{env}</Mono>
                <Mono>{count}</Mono>
              </div>
            ))
          )}
        </div>
      </Panel>
    </div>
  )
}

function Stat({ label, value, hint }: { label: string; value: number; hint?: string }) {
  return (
    <div className="bg-card px-4 py-3">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-xl font-semibold text-foreground">{value}</p>
      {hint !== undefined && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  )
}
