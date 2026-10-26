// The console's dialogs, hand-rolled: the app carries no modal dependency, and a
// dialog here is a bordered card over a dimmed backdrop, left by Escape, the
// backdrop or the close button.

import { useEffect, type ReactNode } from "react"
import { Loader2, X } from "lucide-react"

import { Button } from "@/components/ui/button"

export function Modal({
  open,
  title,
  description,
  onClose,
  children,
  footer,
}: {
  open: boolean
  title: string
  description?: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
}) {
  // Escape closes: a dialog a keyboard cannot leave is a trap.
  useEffect(() => {
    if (!open) return
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [open, onClose])

  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div
        className="absolute inset-0 bg-background/80 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="relative z-10 flex max-h-[85vh] w-full max-w-lg flex-col rounded-lg border border-border bg-card shadow-lg"
      >
        <header className="flex items-start justify-between gap-3 border-b border-border/60 px-4 py-3">
          <div className="min-w-0">
            <h3 className="font-heading text-sm font-semibold text-foreground">{title}</h3>
            {description !== undefined && (
              <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
            )}
          </div>
          <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="Close">
            <X />
          </Button>
        </header>

        <div className="space-y-3 overflow-auto p-4">{children}</div>

        {footer !== undefined && (
          <footer className="flex justify-end gap-2 border-t border-border/60 px-4 py-3">
            {footer}
          </footer>
        )}
      </div>
    </div>
  )
}

/** A question with a yes: a confirmation that carries whatever the route also
 * needs — a reason, a code, a password — in `children`. */
export function ConfirmDialog({
  open,
  title,
  description,
  message,
  confirmLabel = "Confirm",
  destructive = false,
  busy = false,
  onConfirm,
  onClose,
  children,
}: {
  open: boolean
  title: string
  description?: string
  message?: string
  confirmLabel?: string
  destructive?: boolean
  busy?: boolean
  onConfirm: () => void
  onClose: () => void
  children?: ReactNode
}) {
  return (
    <Modal
      open={open}
      title={title}
      description={description}
      onClose={onClose}
      footer={
        <Button
          variant={destructive ? "destructive" : "default"}
          onClick={onConfirm}
          disabled={busy}
        >
          {busy && <Loader2 className="animate-spin" />}
          {confirmLabel}
        </Button>
      }
    >
      {message !== undefined && <p className="text-sm text-muted-foreground">{message}</p>}
      {children}
    </Modal>
  )
}
