import { useQuery, type UseQueryResult } from "@tanstack/react-query"

import { consoleGet } from "@/lib/console"
import { useSession } from "@/lib/session"

/** A console read. Disabled until there is a token, never retried (a refusal is
 * an answer, not a flake), and keyed by the path plus its filters so paging and
 * per-user views do not share a cache entry. `enabled` lets a view hold off on a
 * request it has no argument for yet. */
export function useConsoleQuery<T>(
  key: string,
  path: string,
  params: Record<string, string | number | undefined> = {},
  enabled = true,
): UseQueryResult<T> {
  const token = useSession()?.token ?? ""
  return useQuery({
    queryKey: ["console", key, params],
    enabled: enabled && token !== "",
    retry: 0,
    queryFn: () => consoleGet<T>(path, token, params),
  })
}

/** The one-line summary a failed console read shows. */
export function queryError(error: unknown): string | null {
  if (error === null || error === undefined) return null
  if (error instanceof Error) return error.message
  return String(error)
}
