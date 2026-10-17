// The admin console's network layer. Same rules as `api.ts`: root-absolute paths
// (the app is served under `/dashboard/`), a timeout on every request, and never a
// swallowed failure — a refusal carries the server's own message so the UI can show
// what the console actually said.

import { clearSession } from "@/lib/session";

export * from "@/lib/console/types";
import type { ConsoleSession } from "@/lib/console/types";

/** Every console path this client knows, so a test can enumerate the surface. */
export const CONSOLE_ENDPOINTS = {
  authenticate: "/v2/console/authenticate",
  logout: "/v2/console/authenticate/logout",
  status: "/v2/console/status",
  routes: "/v2/console/route",
  mfa: "/v2/console/mfa",
  accounts: "/v2/console/account",
  accountImport: "/v2/console/account/import",
  users: "/v2/console/user",
  groups: "/v2/console/group",
  leaderboards: "/v2/console/leaderboard",
  tournaments: "/v2/console/tournament",
  storage: "/v2/console/storage",
  storageCollections: "/v2/console/storage/collections",
  notifications: "/v2/console/notification",
  purchases: "/v2/console/purchase",
  subscriptions: "/v2/console/subscription",
  settings: "/v2/console/setting",
  auditLog: "/v2/console/audit/log",
  aclTemplates: "/v2/console/acl/template",
} as const;

const REQUEST_TIMEOUT_MS = 8000;

/** The refusal a login with an enrolled second factor gets: the password was
 * right and the console is asking for the code, which is the one refusal the login
 * page acts on rather than shows. */
export const MFA_CODE_REQUIRED = "MFA code required.";

export type HttpMethod = "POST" | "PUT" | "DELETE";

/** A refusal the console UI branches on: `status` picks the treatment, `message` is
 * the server's wording and is what it shows. */
export class ConsoleError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ConsoleError";
    this.status = status;
  }
}

async function request<T>(path: string, init: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(path, {
      ...init,
      headers: { accept: "application/json", ...init.headers },
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
  } catch (cause) {
    throw new ConsoleError(0, describe(cause));
  }
  const text = await response.text();
  if (!response.ok) {
    // A dead or revoked token must not leave the UI on an authenticated-looking
    // screen: the session is dropped here, and the shell falls back to the login.
    if (response.status === 401) clearSession();
    throw new ConsoleError(response.status, refusalMessage(text, response.statusText));
  }
  if (text === "") return undefined as T;
  try {
    return JSON.parse(text) as T;
  } catch (cause) {
    throw new ConsoleError(response.status, describe(cause));
  }
}

/** The `/v2` failure body is `{"message": "…"}`; anything else is shown verbatim. */
function refusalMessage(text: string, fallback: string): string {
  try {
    const body = JSON.parse(text) as { message?: unknown };
    if (typeof body.message === "string" && body.message !== "") return body.message;
  } catch {
    // Not JSON: fall through to the raw text.
  }
  return text.trim() === "" ? fallback : text.trim();
}

function describe(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/** `mfa` is empty unless the operator has enrolments; the server refuses a login
 * without one only once a secret exists. */
export function consoleLogin(
  username: string,
  password: string,
  mfa = "",
): Promise<ConsoleSession> {
  return request<ConsoleSession>(CONSOLE_ENDPOINTS.authenticate, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ username, password, mfa }),
  });
}

export async function consoleLogout(token: string): Promise<void> {
  await request<void>(CONSOLE_ENDPOINTS.logout, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: "{}",
  });
}

/** A console `GET`. `params` drops `undefined`/`""` so a filter that is not set
 * never becomes an empty query argument the server has to ignore. */
export function consoleGet<T>(
  path: string,
  token: string,
  params: Record<string, string | number | undefined> = {},
): Promise<T> {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== "") query.set(key, String(value));
  }
  const suffix = query.size > 0 ? `?${query.toString()}` : "";
  return request<T>(`${path}${suffix}`, {
    method: "GET",
    headers: { authorization: `Bearer ${token}` },
  });
}

/** A console write. The body is always sent (an empty object when a route takes
 * none), because the console's extractors treat an empty body as "no fields set". */
export function consoleWrite<T>(
  method: HttpMethod,
  path: string,
  token: string,
  body: unknown = {},
): Promise<T> {
  return request<T>(path, {
    method,
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify(body ?? {}),
  });
}
