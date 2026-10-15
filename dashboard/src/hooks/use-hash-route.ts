// The console's route: a hash, so a deep link needs no server-side rewrite and a
// stale or unknown one falls back to the default area instead of a blank page.

import { useEffect, useState } from "react"

import { DEFAULT_AREA, isArea } from "@/lib/areas"

function read(): string {
  const id = window.location.hash.replace(/^#\/?/, "")
  return isArea(id) ? id : DEFAULT_AREA
}

export function useHashRoute(): [string, (id: string) => void] {
  const [route, setRoute] = useState(read)

  useEffect(() => {
    const onHashChange = () => setRoute(read())
    window.addEventListener("hashchange", onHashChange)
    return () => window.removeEventListener("hashchange", onHashChange)
  }, [])

  function navigate(id: string) {
    // Assigning the hash fires `hashchange`, which is what actually updates the
    // state — one path, whether the change came from a click or the address bar.
    window.location.hash = `#/${id}`
  }

  return [route, navigate]
}
