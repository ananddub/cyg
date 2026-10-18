// Create and edit forms open in the same right-side drawer as every detail.
// Drop-in for `Modal` — same props — so a form's Cancel/Save footer moves over
// unchanged. Pure confirmations stay centered modals (console/modal.tsx).

import type { ReactNode } from "react"

import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"

export function FormSheet({
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
  return (
    <Sheet open={open} onOpenChange={(next) => !next && onClose()}>
      <SheetContent side="right" className="gap-0">
        <SheetHeader className="pr-14 text-left">
          <SheetTitle>{title}</SheetTitle>
          {description !== undefined && <SheetDescription>{description}</SheetDescription>}
        </SheetHeader>
        <div className="flex-1 space-y-3 overflow-y-auto px-6 py-4">{open && children}</div>
        {footer !== undefined && <SheetFooter className="flex-row justify-end">{footer}</SheetFooter>}
      </SheetContent>
    </Sheet>
  )
}
