// One account's detail: the profile, the wallet ledger, the operator notes, and
// the acts an operator performs on it — ban, note, wallet, export, delete.

import { useState } from "react"
import { Ban, Download, Loader2, Plus, ShieldCheck, Trash2, Wallet } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Field, JsonBlock, Mono, Panel } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import {
  CONSOLE_ENDPOINTS,
  consoleGet,
  downloadJson,
  type ConsoleAccount,
  type ConsoleAccountExport,
  type ConsoleAccountNote,
  type ConsoleWalletLedger,
} from "@/lib/console"
import { useSession } from "@/lib/session"
import { dash, formatTime, shortId } from "@/lib/format"

import { AddNoteDialog, BanDialog, WalletDialog } from "./write-dialogs"
import { DeleteAccountDialog, DeleteNoteDialog, UnbanDialog } from "./confirm-dialogs"

const ACCOUNT = CONSOLE_ENDPOINTS.accounts
type Action = "ban" | "unban" | "note" | "wallet" | "delete"

export function AccountDetail({ id, onDeleted }: { id: string; onDeleted: () => void }) {
  const token = useSession()?.token ?? ""
  const [action, setAction] = useState<Action | null>(null)
  const [noteToDelete, setNoteToDelete] = useState<string | null>(null)
  const [exporting, setExporting] = useState(false)

  const account = useConsoleQuery<ConsoleAccount>("account", `${ACCOUNT}/${id}`)
  const notes = useConsoleQuery<ConsoleAccountNote[]>("account-notes", `${ACCOUNT}/${id}/note`)
  const wallet = useConsoleQuery<ConsoleWalletLedger[]>("account-wallet", `${ACCOUNT}/${id}/wallet`, {
    limit: 25,
  })

  const disabled =
    account.data !== undefined && new Date(account.data.disable_time).getTime() > 0

  /** The export is the import's other half, so it is saved as a file the operator
   * can carry to another deployment. */
  async function exportAccount() {
    setExporting(true)
    try {
      const document_ = await consoleGet<ConsoleAccountExport>(`${ACCOUNT}/${id}/export`, token)
      downloadJson(`account-${account.data?.username ?? id}.json`, document_)
      toast.success("Exported.")
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setExporting(false)
    }
  }

  return (
    <div className="space-y-4">
      <Panel
        title="Account"
        description={shortId(id)}
        actions={
          <div className="flex shrink-0 gap-1.5">
            <Button size="sm" variant="outline" onClick={exportAccount} disabled={exporting}>
              {exporting ? <Loader2 className="animate-spin" /> : <Download />}
              Export
            </Button>
            {disabled ? (
              <Button size="sm" variant="outline" onClick={() => setAction("unban")}>
                <ShieldCheck />
                Unban
              </Button>
            ) : (
              <Button size="sm" variant="outline" onClick={() => setAction("ban")}>
                <Ban />
                Ban
              </Button>
            )}
            <Button size="sm" variant="destructive" onClick={() => setAction("delete")}>
              <Trash2 />
              Delete
            </Button>
          </div>
        }
      >
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

      <Panel
        title="Wallet"
        actions={
          <Button size="sm" variant="outline" onClick={() => setAction("wallet")}>
            <Wallet />
            Adjust
          </Button>
        }
      >
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

      <Panel
        title="Operator notes"
        actions={
          <Button size="sm" variant="outline" onClick={() => setAction("note")}>
            <Plus />
            Add note
          </Button>
        }
      >
        {notes.data === undefined || notes.data.length === 0 ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">No notes.</p>
        ) : (
          <div className="divide-y divide-border/40">
            {notes.data.map((note) => (
              <div key={note.id} className="flex items-start justify-between gap-3 px-4 py-3">
                <div className="min-w-0 text-sm text-foreground">
                  <p className="whitespace-pre-wrap">{note.note}</p>
                  <Mono className="mt-1 block">{formatTime(note.create_time)}</Mono>
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Delete note"
                  onClick={() => setNoteToDelete(note.id)}
                >
                  <Trash2 />
                </Button>
              </div>
            ))}
          </div>
        )}
      </Panel>

      <BanDialog id={id} open={action === "ban"} onClose={() => setAction(null)} />
      <UnbanDialog id={id} open={action === "unban"} onClose={() => setAction(null)} />
      <AddNoteDialog id={id} open={action === "note"} onClose={() => setAction(null)} />
      <WalletDialog id={id} open={action === "wallet"} onClose={() => setAction(null)} />
      <DeleteAccountDialog
        id={id}
        open={action === "delete"}
        onClose={() => setAction(null)}
        onDeleted={onDeleted}
      />
      <DeleteNoteDialog
        id={id}
        noteId={noteToDelete ?? ""}
        open={noteToDelete !== null}
        onClose={() => setNoteToDelete(null)}
      />
    </div>
  )
}