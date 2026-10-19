// Which rows of a table the operator has picked. Shared by the console's
// DataTable and the runtime tables, so "select all" means the same thing twice.

import { useState } from "react"

/** Keys no longer on screen drop out of the set, so a pick never silently
 * carries across a page or a live refresh and then acts on a row nobody is
 * looking at. */
export function useSelection(keys: readonly string[]) {
  const [picked, setPicked] = useState<ReadonlySet<string>>(() => new Set())
  const onScreen = new Set(keys)
  const selected = new Set<string>()
  for (const key of picked) if (onScreen.has(key)) selected.add(key)

  return {
    selected,
    allSelected: keys.length > 0 && selected.size === keys.length,
    someSelected: selected.size > 0 && selected.size < keys.length,
    toggle(key: string) {
      setPicked((current) => {
        const next = new Set(current)
        if (next.has(key)) next.delete(key)
        else next.add(key)
        return next
      })
    },
    toggleAll() {
      setPicked((current) => {
        const next = new Set(current)
        if (selected.size === keys.length) for (const key of keys) next.delete(key)
        else for (const key of keys) next.add(key)
        return next
      })
    },
    clear() {
      setPicked(new Set())
    },
  }
}
