// The account's wallet ledger and the row action that removes an entry.

import { useState } from "react"
import { Trash2, Wallet } from "lucide-react"

import { Button } from "@/components/ui/button"
import { JsonBlock, Mono, Panel } from "@/components/console/data-table"
import { useConsoleQuery } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleWalletLedger } from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"

import { DeleteLedgerDialog } from "./confirm-dialogs"

export function WalletLedgerPanel({ id, onAdjust }: { id: string; onAdjust: () => void }) {
  const [walletToDelete, setWalletToDelete] = useState<string | null>(null)
  const wallet = useConsoleQuery<ConsoleWalletLedger[]>(
    "account-wallet",
    `${CONSOLE_ENDPOINTS.accounts}/${id}/wallet`,
    { limit: 25 },
  )

  return (
    <>
      <Panel
        title="Wallet"
        actions={
          <Button size="sm" variant="outline" onClick={onAdjust}>
            <Wallet />
            Adjust
          </Button>
        }
      >
        {wallet.data === undefined || wallet.data.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-muted-foreground">No ledger entries.</p>
        ) : (
          <div className="divide-y divide-border/40">
            {wallet.data.map((entry) => (
              <div key={entry.id} className="px-4 py-3">
                <div className="mb-1 flex items-center justify-between">
                  <Mono>{shortId(entry.id)}</Mono>
                  <div className="flex items-center gap-2">
                    <Mono>{formatTime(entry.create_time)}</Mono>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label="Delete ledger entry"
                      onClick={() => setWalletToDelete(entry.id)}
                    >
                      <Trash2 />
                    </Button>
                  </div>
                </div>
                <JsonBlock value={entry.changeset} />
              </div>
            ))}
          </div>
        )}
      </Panel>
      <DeleteLedgerDialog
        id={id}
        walletId={walletToDelete ?? ""}
        open={walletToDelete !== null}
        onClose={() => setWalletToDelete(null)}
      />
    </>
  )
}
