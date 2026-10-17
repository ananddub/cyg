import { useMutation, useQuery, useQueryClient, type UseQueryResult } from "@tanstack/react-query"
import { toast } from "sonner"

import { consoleGet, consoleWrite, type HttpMethod } from "@/lib/console"
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

/** A console write. The method, path and body travel as one variable so a table
 * can hold a single hook while each row points it at its own route — a hook per
 * row would be a hook in a loop. Invalidation is not the caller's business: a
 * write can move any list, so the whole `["console"]` cache is refreshed. */
export function useConsoleMutation<TData = unknown>() {
  const token = useSession()?.token ?? ""
  const client = useQueryClient()
  return useMutation<
    TData,
    Error,
    { method: HttpMethod; path: string; body?: unknown; success: string }
  >({
    mutationFn: ({ method, path, body }) => consoleWrite<TData>(method, path, token, body),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: ["console"] })
      toast.success(variables.success)
    },
    onError: (error) => toast.error(error.message),
  })
}
