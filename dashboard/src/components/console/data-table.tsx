import type { ReactNode } from "react"
import { AlertTriangle, ChevronLeft, ChevronRight, Inbox, Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { cn } from "@/lib/utils"

export interface Column<T> {
  header: string
  cell: (row: T) => ReactNode
  /** Extra classes for both the header and the body cell. */
  className?: string
}

interface DataTableProps<T> {
  columns: Column<T>[]
  rows: T[] | undefined
  loading: boolean
  error?: string | null
  rowKey: (row: T) => string
  onRowClick?: (row: T) => void
  selectedKey?: string
  empty?: string
}

/** The console's one table. It renders exactly one of four states — loading,
 * failed, empty, rows — so no area has to re-invent a spinner or an error line,
 * and "nothing here" never looks like "nothing loaded". */
export function DataTable<T>({
  columns,
  rows,
  loading,
  error,
  rowKey,
  onRowClick,
  selectedKey,
  empty = "Nothing to show.",
}: DataTableProps<T>) {
  if (error !== null && error !== undefined) {
    return (
      <div className="flex items-center gap-2 px-4 py-6 text-sm text-destructive">
        <AlertTriangle className="size-4 shrink-0" />
        <span>{error}</span>
      </div>
    )
  }

  if (loading) {
    return (
      <div className="flex items-center gap-2 px-4 py-6 text-sm text-muted-foreground">
        <Loader2 className="size-4 shrink-0 animate-spin" />
        <span>Loading…</span>
      </div>
    )
  }

  if (rows === undefined || rows.length === 0) {
    return (
      <div className="flex items-center gap-2 px-4 py-6 text-sm text-muted-foreground">
        <Inbox className="size-4 shrink-0" />
        <span>{empty}</span>
      </div>
    )
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          {columns.map((column) => (
            <TableHead key={column.header} className={column.className}>
              {column.header}
            </TableHead>
          ))}
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((row) => {
          const key = rowKey(row)
          const active = selectedKey !== undefined && selectedKey === key
          return (
            <TableRow
              key={key}
              onClick={onRowClick === undefined ? undefined : () => onRowClick(row)}
              className={cn(
                active && "bg-primary/10 hover:bg-primary/10",
                onRowClick !== undefined && "cursor-pointer",
              )}
            >
              {columns.map((column) => (
                <TableCell key={column.header} className={column.className}>
                  {column.cell(row)}
                </TableCell>
              ))}
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}

/** A titled section: the console's card, with an optional right-hand action. */
export function Panel({
  title,
  description,
  actions,
  children,
  className,
}: {
  title: string
  description?: string
  actions?: ReactNode
  children: ReactNode
  className?: string
}) {
  return (
    <section className={cn("rounded-lg border border-border bg-card shadow-xs", className)}>
      <header className="flex items-center justify-between gap-3 border-b border-border/60 px-4 py-3">
        <div className="min-w-0">
          <h3 className="font-heading text-sm font-semibold text-foreground">{title}</h3>
          {description !== undefined && (
            <p className="mt-0.5 truncate text-xs text-muted-foreground">{description}</p>
          )}
        </div>
        {actions}
      </header>
      <div className="overflow-hidden">{children}</div>
    </section>
  )
}

/** The page heading every console view starts with. */
export function PageHeading({ title, description }: { title: string; description: string }) {
  return (
    <div>
      <h1 className="font-heading text-xl font-bold tracking-tight text-foreground">{title}</h1>
      <p className="mt-1 text-sm text-muted-foreground">{description}</p>
    </div>
  )
}

/** A JSON value as the viewer prints it. Astro-free: no syntax colouring, just
 * the structure, which is what an operator reads. */
export function JsonBlock({ value }: { value: unknown }) {
  const text = JSON.stringify(value ?? null, null, 2) ?? "null"
  return (
    <pre className="max-h-80 overflow-auto rounded-md border border-border/60 bg-background/60 p-3 font-mono text-xs leading-relaxed text-muted-foreground">
      {text}
    </pre>
  )
}

/** A label/value list for a detail pane. */
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-border/40 py-1.5 last:border-0">
      <span className="shrink-0 text-xs text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate text-right font-mono text-xs text-foreground">{children}</span>
    </div>
  )
}

/** A short, monospaced id or timestamp string with a middle ellipsis. */
export function Mono({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span className={cn("font-mono text-xs tabular-nums text-muted-foreground", className)}>
      {children}
    </span>
  )
}

/** Offset paging for a console list. "Next" is offered only while a full page
 * came back, since the API reports no total count. */
export function Pager({
  offset,
  limit,
  count,
  onChange,
}: {
  offset: number
  limit: number
  count: number
  onChange: (offset: number) => void
}) {
  const hasPrevious = offset > 0
  const hasNext = count === limit
  return (
    <div className="flex items-center justify-between gap-3 border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
      <span className="tabular-nums">
        {count === 0 ? "0 results" : `${offset + 1}–${offset + count}`}
      </span>
      <div className="flex gap-1.5">
        <Button
          variant="outline"
          size="xs"
          disabled={!hasPrevious}
          onClick={() => onChange(Math.max(0, offset - limit))}
        >
          <ChevronLeft />
          Prev
        </Button>
        <Button
          variant="outline"
          size="xs"
          disabled={!hasNext}
          onClick={() => onChange(offset + limit)}
        >
          Next
          <ChevronRight />
        </Button>
      </div>
    </div>
  )
}
