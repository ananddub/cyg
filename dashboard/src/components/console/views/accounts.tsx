// Player accounts: the directory, and — for the selected row — its notes and
// wallet ledger.

import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { DataTable, Field, JsonBlock, Mono, PageHeading, Pager, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import {
  CONSOLE_ENDPOINTS,
  type ConsoleAccount,
  type ConsoleAccountNote,
  type ConsoleWalletLedger,
} from "@/lib/console"
import { dash, formatTime, num, shortId } from "@/lib/format"

const PAGE_SIZE = 25

export function AccountsView() {
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState<string | null>(null)

  const list = useConsoleQuery<ConsoleAccount[]>("accounts", CONSOLE_ENDPOINTS.accounts, {
    limit: PAGE_SIZE,
    offset,
  })

  const columns: Column<ConsoleAccount>[] = [
    { header: "Username", cell: (row) => <span className="font-medium text-foreground">{row.username}</span> },
    { header: "Display name", cell: (row) => dash(row.display_name) },
    { header: "Email", cell: (row) => <Mono>{dash(row.email)}</Mono> },
    { header: "Groups", cell: (row) => <Mono>{num(row.edge_count)}</Mono> },
    { header: "Created", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
    {
      header: "Status",
      cell: (row) =>
        new Date(row.disable_time).getTime() > 0 ? (
          <Badge variant="destructive">Disabled</Badge>
        ) : (
          <Badge variant="secondary">Active</Badge>
        ),
    },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="Accounts"
        description="Player accounts. Select one to read its notes, wallet ledger and provider links."
      />

      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_24rem]">
        <Panel title="All accounts" description={`page ${Math.floor(offset / PAGE_SIZE) + 1}`}>
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            onRowClick={(row) => setSelected(row.id)}
            selectedKey={selected ?? undefined}
            empty="No accounts yet."
          />
          <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
        </Panel>

        {selected === null ? (
          <Panel title="Account detail" description="No account selected">
            <p className="px-4 py-6 text-sm text-muted-foreground">Select a row to inspect it.</p>
          </Panel>
        ) : (
          <AccountDetail id={selected} />
        )}
      </div>
    </div>
  )
}

function AccountDetail({ id }: { id: string }) {
  const account = useConsoleQuery<ConsoleAccount>("account", `${CONSOLE_ENDPOINTS.accounts}/${id}`)
  const notes = useConsoleQuery<ConsoleAccountNote[]>("account-notes", `${CONSOLE_ENDPOINTS.accounts}/${id}/note`)
  const wallet = useConsoleQuery<ConsoleWalletLedger[]>("account-wallet", `${CONSOLE_ENDPOINTS.accounts}/${id}/wallet`, {
    limit: 25,
  })

  return (
    <div className="space-y-4">
      <Panel title="Account" description={shortId(id)}>
        {queryError(account.error) !== null ? (
          <p className="px-4 py-6 text-sm text-destructive">{queryError(account.error)}</p>
        ) : account.data === undefined ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">Loading…</p>
        ) : (
          <div className="px-4 py-2">
            <Field label="Username">{account.data.username}</Field>
            <Field label="Display name">{dash(account.data.display_name)}</Field>
            <Field label="Email">{dash(account.data.email)}</Field>
            <Field label="Language">{dash(account.data.lang_tag)}</Field>
            <Field label="Custom id">{dash(account.data.custom_id)}</Field>
            <Field label="Steam id">{dash(account.data.steam_id)}</Field>
            <Field label="Google id">{dash(account.data.google_id)}</Field>
            <Field label="Apple id">{dash(account.data.apple_id)}</Field>
            <Field label="Created">{formatTime(account.data.create_time)}</Field>
            <Field label="Updated">{formatTime(account.data.update_time)}</Field>
          </div>
        )}
      </Panel>

      <Panel title="Metadata">
        <div className="p-3">
          <JsonBlock value={account.data?.metadata ?? {}} />
        </div>
      </Panel>

      <Panel title="Wallet">
        {wallet.data === undefined || wallet.data.length === 0 ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">No ledger entries.</p>
        ) : (
          <div className="divide-y divide-border/40">
            {wallet.data.map((entry) => (
              <div key={entry.id} className="px-4 py-3">
                <div className="mb-1 flex items-center justify-between">
                  <Mono>{shortId(entry.id)}</Mono>
                  <Mono>{formatTime(entry.create_time)}</Mono>
                </div>
                <JsonBlock value={entry.changeset} />
              </div>
            ))}
          </div>
        )}
      </Panel>

      <Panel title="Operator notes">
        {notes.data === undefined || notes.data.length === 0 ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">No notes.</p>
        ) : (
          <div className="divide-y divide-border/40">
            {notes.data.map((note) => (
              <div key={note.id} className="px-4 py-3 text-sm text-foreground">
                <p className="whitespace-pre-wrap">{note.note}</p>
                <Mono className="mt-1 block">{formatTime(note.create_time)}</Mono>
              </div>
            ))}
          </div>
        )}
      </Panel>
    </div>
  )
}
