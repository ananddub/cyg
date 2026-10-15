import { useMemo, useState } from "react"
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  XAxis,
  YAxis,
} from "recharts"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { TickSeries } from "@/lib/types"

const tickChartConfig = {
  p99: { label: "p99", color: "var(--chart-1)" },
  p50: { label: "p50", color: "var(--chart-2)" },
} satisfies ChartConfig

const RANGES_SEC = [60, 120, 300] as const

/** The contract speaks µs; the chart plots ms and labels the axis by size. */
function formatDuration(ms: number): string {
  return ms >= 1 ? `${ms.toFixed(2)} ms` : `${Math.round(ms * 1000)} µs`
}

interface TickChartProps {
  series: TickSeries | undefined
  nominalUs: number | undefined
  loading: boolean
}

interface ChartPoint {
  label: string
  p50: number | null
  p99: number | null
}

export function TickChart({ series, nominalUs, loading }: TickChartProps) {
  const [rangeSec, setRangeSec] = useState<number>(120)

  const data = useMemo<ChartPoint[]>(() => {
    const points = series?.points ?? []
    if (points.length === 0) return []
    const step = Math.max(1, series?.intervalSec ?? 1)
    const visible = points.slice(-Math.max(2, Math.floor(rangeSec / step)))
    const latest = visible[visible.length - 1]!.atMs
    return visible.map((p) => {
      const agoSec = Math.round((latest - p.atMs) / 1000)
      return {
        label: agoSec === 0 ? "now" : `-${agoSec}s`,
        // A gap is a missing sample, not a flat run: null breaks the line.
        p50: p.gap ? null : p.p50Us / 1000,
        p99: p.gap ? null : p.p99Us / 1000,
      }
    })
  }, [series, rangeSec])

  const nominalMs =
    nominalUs != null && nominalUs > 0 ? nominalUs / 1000 : null

  if (loading && (series?.points.length ?? 0) === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Tick time</CardTitle>
        </CardHeader>
        <CardContent>
          <Skeleton className="h-56 w-full" />
        </CardContent>
      </Card>
    )
  }

  if (data.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Tick time</CardTitle>
          <CardDescription>waiting for the sim to publish</CardDescription>
        </CardHeader>
      </Card>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Tick time</CardTitle>
        <CardDescription>ms per tick, one sample per second</CardDescription>
        <CardAction>
          <Tabs value={String(rangeSec)} onValueChange={(v) => setRangeSec(Number(v))}>
            <TabsList variant="line" className="h-7">
              {RANGES_SEC.map((r) => (
                <TabsTrigger key={r} value={String(r)} className="px-2 py-0 text-xs">
                  {r} s
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
        </CardAction>
      </CardHeader>
      <CardContent>
        <ChartContainer config={tickChartConfig} className="h-56 w-full">
          <LineChart data={data} margin={{ left: 4, right: 12, top: 8 }}>
            <CartesianGrid strokeDasharray="3 3" vertical={false} />
            <XAxis
              dataKey="label"
              tickLine={false}
              axisLine={false}
              tickMargin={8}
              minTickGap={32}
            />
            <YAxis
              tickLine={false}
              axisLine={false}
              width={64}
              tickFormatter={(v: number) => formatDuration(v)}
            />
            <ChartTooltip
              cursor={false}
              content={
                <ChartTooltipContent
                  formatter={(value) =>
                    typeof value === "number" ? formatDuration(value) : String(value)
                  }
                />
              }
            />
            {nominalMs != null && (
              <ReferenceLine
                y={nominalMs}
                stroke="var(--color-muted-foreground)"
                strokeDasharray="6 4"
                label={{
                  value: `nominal ${formatDuration(nominalMs)}`,
                  position: "insideTopRight",
                  fill: "var(--color-muted-foreground)",
                  fontSize: 10,
                }}
              />
            )}
            <Line
              name="p99"
              dataKey="p99"
              type="monotone"
              stroke="var(--color-p99)"
              strokeWidth={1.5}
              dot={false}
              connectNulls={false}
              isAnimationActive={false}
            />
            <Line
              name="p50"
              dataKey="p50"
              type="monotone"
              stroke="var(--color-p50)"
              strokeWidth={1.5}
              dot={false}
              connectNulls={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ChartContainer>
      </CardContent>
    </Card>
  )
}
