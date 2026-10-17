// The console's one-click writes: a confirmation, a route, a toast.

import { ConfirmDialog } from "@/components/console/modal"
import { useConsoleMutation } from "@/hooks/use-console"
import type { HttpMethod } from "@/lib/console"

/** A write that needs nothing but a yes. The message says what will happen and the
 * route says what does it; `onDone` runs only after the server accepted it, which
 * is where a caller clears a selection the row no longer backs. */
export function ConfirmAction({
  open,
  onClose,
  onDone,
  title,
  message,
  confirmLabel,
  destructive = false,
  method,
  path,
  body,
  success,
}: {
  open: boolean
  onClose: () => void
  onDone?: () => void
  title: string
  message: string
  confirmLabel: string
  destructive?: boolean
  method: HttpMethod
  path: string
  body?: unknown
  success: string
}) {
  const write = useConsoleMutation()

  return (
    <ConfirmDialog
      open={open}
      title={title}
      message={message}
      confirmLabel={confirmLabel}
      destructive={destructive}
      busy={write.isPending}
      onClose={onClose}
      onConfirm={() =>
        write.mutate(
          { method, path, body, success },
          {
            onSuccess: () => {
              onClose()
              onDone?.()
            },
          },
        )
      }
    />
  )
}
