// The account area's destructive dialogs: a one-click write each, so they are a
// confirmation and a route rather than a form.

import { ConfirmAction } from "@/components/console/actions"
import { CONSOLE_ENDPOINTS } from "@/lib/console"

import type { AccountDialogProps } from "./write-dialogs"

const ACCOUNT = CONSOLE_ENDPOINTS.accounts

export function UnbanDialog({ id, open, onClose }: AccountDialogProps) {
  return (
    <ConfirmAction
      open={open}
      onClose={onClose}
      title="Lift the ban"
      message="The account becomes active again, and the ban disappears from its status."
      confirmLabel="Unban"
      method="POST"
      path={`${ACCOUNT}/${id}/unban`}
      success="Account unbanned."
    />
  )
}

export function DeleteNoteDialog({
  id,
  noteId,
  open,
  onClose,
}: AccountDialogProps & { noteId: string }) {
  return (
    <ConfirmAction
      open={open}
      onClose={onClose}
      title="Delete the note"
      message="The note is removed for good; the audit trail keeps the record that it was."
      confirmLabel="Delete"
      destructive
      method="DELETE"
      path={`${ACCOUNT}/${id}/note/${noteId}`}
      success="Note deleted."
    />
  )
}

export function DeleteAccountDialog({
  id,
  open,
  onClose,
  onDeleted,
}: AccountDialogProps & { onDeleted: () => void }) {
  return (
    <ConfirmAction
      open={open}
      onClose={onClose}
      onDone={onDeleted}
      title="Delete account"
      message="The account is tombstoned and its rows go with it: devices, notes, storage and messages. Purchases are left pointing at the nil id."
      confirmLabel="Delete account"
      destructive
      method="DELETE"
      path={`${ACCOUNT}/${id}`}
      success="Account deleted."
    />
  )
}
