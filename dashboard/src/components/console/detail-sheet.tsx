import type { ReactNode } from "react"

import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { cn } from "cn"

// Body mounts only while `open` — seed state from props, discarded on close.
// Detail bodies should key by entity id so switching rows remounts.
export function DetailSheet({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  bodyClassName,
}: {
  open: boolean
  onClose: () => void
  title: ReactNode
  description?: ReactNode
  children: ReactNode
  footer?: ReactNode
  bodyClassName?: string
}) {
  return (
    <Sheet open={open} onOpenChange={(next) => !next && onClose()}>
      <SheetContent side="right" className="gap-0">
        <SheetHeader>
          <SheetTitle>{title}</SheetTitle>
          {description != null && (
            <SheetDescription>{description}</SheetDescription>
          )}
        </SheetHeader>
        <div className={cn("flex-1 overflow-y-auto px-6 py-4", bodyClassName)}>
          {open && children}
        </div>
        {footer}
      </SheetContent>
    </Sheet>
  )
}
