import { useEffect } from "react"
import { useMutation, useQuery, useQueryClient, type UseQueryResult } from "@tanstack/react-query"
import { toast } from "sonner"

import type { LiveMark } from "@/hooks/use-metrics-ws"

import { consoleGet, consoleWrite, type HttpMethod } from "@/lib/console"
import { useGameScope } from "@/lib/console/scope"
import { useSession } from "@/lib/session"

/** A console read. Disabled until there is a token, never retried (a refusal is
 * an answer, not a flake), and keyed by the path plus its filters so paging and
 * per-user views do not share a cache entry. `enabled` lets a view hold off on a
 * request it has no argument for yet.
 *
 * Inside a game workspace the active game id rides along automatically, so a
 * view cannot accidentally list every tenant's rows. */
export function useConsoleQuery<T>(
  key: string,
  path: string,
  params: Record<string, string | number | undefined> = {},
  enabled = true,
): UseQueryResult<T> {
  const token = useSession()?.token ?? ""
  const scope = useGameScope()
  const scoped = scope === null ? params : { game_id: scope, ...params }
  return useQuery({
    queryKey: ["console", key, scoped],
    enabled: enabled && token !== "",
    retry: 0,
    queryFn: () => consoleGet<T>(path, token, scoped),
  })
}

// A burst (bulk import, wallet sync) collapses into one refetch: the timer
// restarts on each mark.
const LIVE_REFRESH_MS = 300

/** A live mark means some console read is stale; the open views refetch their
 * own endpoint, so a view that is not open costs nothing. */
export function useLiveRefetch(live: LiveMark | null) {
  const client = useQueryClient()
  useEffect(() => {
    if (live === null) return
    const timer = window.setTimeout(() => {
      void client.invalidateQueries({ queryKey: ["console"] })
    }, LIVE_REFRESH_MS)
    return () => window.clearTimeout(timer)
  }, [live, client])
}

/** The one-line summary a failed console read shows. */
export function queryError(error: unknown): string | null {
  if (error === null || error === undefined) return null
  if (error instanceof Error) return error.message
  return String(error)
}

/** A console write. The method, path and body travel as one variable so a table
 * can hold a single hook while each row points it at its own route — a hook per
 * row would be a hook in a loop. Invalidation is not the caller's business: a
 * write can move any list, so the whole `["console"]` cache is refreshed. */
export function useConsoleMutation<TData = unknown>() {
  const token = useSession()?.token ?? ""
  const client = useQueryClient()
  const scope = useGameScope()
  return useMutation<
    TData,
    Error,
    { method: HttpMethod; path: string; body?: unknown; success: string }
  >({
    mutationFn: ({ method, path, body }) =>
      consoleWrite<TData>(method, path, token, body, scope === null ? {} : { game_id: scope }),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: ["console"] })
      toast.success(variables.success)
    },
    onError: (error) => toast.error(error.message),
  })
}
