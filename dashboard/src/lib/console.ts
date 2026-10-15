// The admin console's network layer. Same rules as `api.ts`: root-absolute
// paths (the app is served under `/dashboard/`), a timeout on every request, and
// never a swallowed failure — a refusal carries the server's own message so the
// UI can show what the console actually said.
//
// Field names are snake_case because the Rust DTOs have no `rename_all`: the wire
// is the struct, exactly as `/v2` does it.

import { clearSession } from "@/lib/session";

/** Every console path this client knows, so a test can enumerate the surface. */
export const CONSOLE_ENDPOINTS = {
  authenticate: "/v2/console/authenticate",
  logout: "/v2/console/authenticate/logout",
  accounts: "/v2/console/account",
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

export interface ConsoleSession {
  token: string;
  username: string;
  email?: string;
  role: string;
}

/** A refusal the console UI branches on: `status` picks the treatment, `message`
 * is the server's wording and is what it shows. */
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

export function consoleLogin(username: string, password: string): Promise<ConsoleSession> {
  return request<ConsoleSession>(CONSOLE_ENDPOINTS.authenticate, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ username, password }),
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

// ── Row shapes, mirrored from `crates/engine-api/src/dto/console/*.rs` ──

export interface ConsoleAccount {
  id: string;
  username: string;
  display_name: string;
  avatar_url: string;
  lang_tag: string;
  location: string;
  timezone: string;
  email: string;
  metadata: unknown;
  wallet: unknown;
  edge_count: number;
  create_time: string;
  update_time: string;
  disable_time: string;
  custom_id: string;
  facebook_id: string;
  google_id: string;
  gamecenter_id: string;
  steam_id: string;
  apple_id: string;
  facebook_instant_game_id: string;
}

export interface ConsoleAccountNote {
  id: string;
  note: string;
  create_time: string;
  update_time: string;
}

export interface ConsoleWalletLedger {
  id: string;
  changeset: unknown;
  metadata: unknown;
  create_time: string;
  update_time: string;
}

export interface ConsoleUser {
  id: string;
  username: string;
  email: string;
  role: string;
  acl: unknown;
  metadata: unknown;
  mfa_required: boolean;
  disabled: boolean;
  create_time: string;
  update_time: string;
}

export interface ConsoleGroup {
  id: string;
  creator_id: string;
  name: string;
  description: string;
  avatar_url: string;
  lang_tag: string;
  metadata: unknown;
  open: boolean;
  edge_count: number;
  max_count: number;
  create_time: string;
  update_time: string;
}

export interface ConsoleGroupMember {
  id: string;
  username: string;
  display_name: string;
  avatar_url: string;
  metadata: unknown;
  state: string;
  create_time: string;
  edge_update_time: string | null;
}

export interface ConsoleLeaderboard {
  id: string;
  title: string;
  description: string;
  kind: string;
  category: number;
  operator: number;
  sort_order: number;
  reset_schedule: string;
  authoritative: boolean;
  metadata: unknown;
  size: number;
  max_num_score: number;
  start_time: string;
  end_time: string;
  duration: number;
  create_time: string;
}

export interface ConsoleRecord {
  leaderboard_id: string;
  owner_id: string;
  username: string;
  score: number;
  subscore: number;
  num_score: number;
  metadata: unknown;
  create_time: string;
  update_time: string;
  expiry_time: string;
}

export interface ConsoleStorageObject {
  collection: string;
  key: string;
  user_id: string;
  value: unknown;
  version: string;
  read: number;
  write: number;
  create_time: string;
  update_time: string;
}

export interface ConsoleNotification {
  id: string;
  user_id: string;
  subject: string;
  content: unknown;
  code: number;
  sender_id: string;
  create_time: string;
}

export interface ConsolePurchase {
  transaction_id: string;
  user_id: string;
  product_id: string;
  store: number;
  environment: number;
  purchase_time: string;
  refund_time: string;
  create_time: string;
}

export interface ConsoleSubscription {
  original_transaction_id: string;
  user_id: string;
  product_id: string;
  store: number;
  environment: number;
  purchase_time: string;
  expire_time: string;
  refund_time: string;
  active: boolean;
}

export interface ConsoleSetting {
  name: string;
  value: unknown;
  update_time: string;
}

export interface ConsoleAuditLog {
  create_time: string;
  id: string;
  console_user_id: string;
  console_username: string;
  email: string;
  action: string;
  resource: string;
  message: string;
  metadata: unknown;
}

export interface ConsoleAclTemplate {
  id: string;
  name: string;
  description: string;
  acl: unknown;
  create_time: string;
  update_time: string;
}
