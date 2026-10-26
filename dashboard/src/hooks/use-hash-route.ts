// The console's route: a hash, so a deep link needs no server-side rewrite and a
// stale or unknown one falls back to the picker instead of a blank page.

import { useEffect, useState } from "react"

import { parseRoute, routeHash, type ConsoleRoute } from "@/lib/routes"

function read(): ConsoleRoute {
  return parseRoute(window.location.hash)
}

export function useHashRoute(): [ConsoleRoute, (to: ConsoleRoute) => void] {
  const [route, setRoute] = useState(read)

  useEffect(() => {
    const onHashChange = () => setRoute(read())
    window.addEventListener("hashchange", onHashChange)
    return () => window.removeEventListener("hashchange", onHashChange)
  }, [])

  function navigate(to: ConsoleRoute) {
    // Assigning the hash fires `hashchange`, which is what actually updates the
    // state — one path, whether the change came from a click or the address bar.
    window.location.hash = routeHash(to)
  }

  return [route, navigate]
}
