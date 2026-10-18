// The account area's dialogs that carry fields: the ban and its reason, the
// operator note, and the wallet grant — a map of currency to *delta*, added to
// what the account holds, which is the shape the ledger records. The ban and
// the note are one field each and stay centered; the wallet is a real form, so
// it opens in the drawer.

import { useState } from "react"
import { Loader2 } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { TextArea, TextInput } from "@/components/console/form"
import { FormSheet } from "@/components/console/form-sheet"
import { parseJson } from "@/components/console/json"
import { ConfirmDialog } from "@/components/console/modal"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS } from "@/lib/console"

const ACCOUNT = CONSOLE_ENDPOINTS.accounts

/** The three properties every per-account dialog needs: which account, whether it
 * is open, and how to close it. */
export interface AccountDialogProps {
  id: string
  open: boolean
  onClose: () => void
}

/** Ban an account. `disable_time` becomes now; the reason is recorded in the audit
 * trail rather than on the account, so the trail is where the act lives. */
export function BanDialog({ id, open, onClose }: AccountDialogProps) {
  const [reason, setReason] = useState("")
  const write = useConsoleMutation()

  function close() {
    setReason("")
    onClose()
  }

  return (
    <ConfirmDialog
      open={open}
      title="Ban account"
      message="The account is disabled until an operator lifts the ban. The reason is recorded in the audit trail, not on the account."
      confirmLabel="Ban"
      destructive
      busy={write.isPending}
      onClose={close}
      onConfirm={() =>
        write.mutate(
          {
            method: "POST",
            path: `${ACCOUNT}/${id}/ban`,
            body: { reason },
            success: "Account banned.",
          },
          { onSuccess: close },
        )
      }
    >
      <TextInput
        label="Reason"
        value={reason}
        onChange={setReason}
        placeholder="Why this account is banned"
      />
    </ConfirmDialog>
  )
}

/** An operator note. The author comes from the token rather than this body, so the
 * trail — not the caller — says who wrote it. */
export function AddNoteDialog({ id, open, onClose }: AccountDialogProps) {
  const [note, setNote] = useState("")
  const write = useConsoleMutation()

  function close() {
    setNote("")
    onClose()
  }

  return (
    <ConfirmDialog
      open={open}
      title="Add a note"
      message="Notes are for operators; the player never sees them."
      confirmLabel="Add note"
      busy={write.isPending}
      onClose={close}
      onConfirm={() =>
        write.mutate(
          {
            method: "POST",
            path: `${ACCOUNT}/${id}/note`,
            body: { note },
            success: "Note added.",
          },
          { onSuccess: close },
        )
      }
    >
      <TextArea label="Note" rows={3} value={note} onChange={setNote} placeholder="What happened" />
    </ConfirmDialog>
  )
}

const DEFAULT_CHANGESET = '{\n  "gold": 100\n}'

export function WalletDialog({ id, open, onClose }: AccountDialogProps) {
  const [changeset, setChangeset] = useState(DEFAULT_CHANGESET)
  const [metadata, setMetadata] = useState("")
  const write = useConsoleMutation()

  function close() {
    setChangeset(DEFAULT_CHANGESET)
    setMetadata("")
    onClose()
  }

  function confirm() {
    const changes = parseJson(changeset)
    if ("error" in changes) {
      toast.error(`The changeset must be JSON: ${changes.error}`)
      return
    }
    if (changes.value === undefined) {
      toast.error("A changeset is required.")
      return
    }
    const extra = parseJson(metadata)
    if ("error" in extra) {
      toast.error(`The metadata must be JSON: ${extra.error}`)
      return
    }
    write.mutate(
      {
        method: "POST",
        path: `${ACCOUNT}/${id}/wallet`,
        body: { changeset: changes.value, metadata: extra.value },
        success: "Wallet updated.",
      },
      { onSuccess: close },
    )
  }

  return (
    <FormSheet
      open={open}
      title="Adjust the wallet"
      description="Each value is added to what the account holds, and the same map is written to the ledger — so the trail records what was granted, not merely the new total."
      onClose={close}
      footer={
        <>
          <Button variant="outline" onClick={close} disabled={write.isPending}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={write.isPending}>
            {write.isPending && <Loader2 className="animate-spin" />}
            Apply
          </Button>
        </>
      }
    >
      <TextArea label="Changeset" rows={10} value={changeset} onChange={setChangeset} />
      <TextArea
        label="Metadata"
        hint="Optional, and recorded beside the change."
        rows={4}
        value={metadata}
        onChange={setMetadata}
      />
    </FormSheet>
  )
}
