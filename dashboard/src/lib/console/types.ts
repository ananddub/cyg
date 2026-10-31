// Row shapes, mirrored from `crates/engine-api/src/dto/console/*.rs`. The wire is
// snake_case because those DTOs have no `rename_all`, exactly as `/v2` does it.

/** The token plus the identity it carries, so the shell can render its header
 * without decoding the JWT. */
export interface ConsoleSession {
  token: string;
  username: string;
  email?: string;
  role: string;
}

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

/** The export/import document. `password` is never part of it, and absent optional
 * columns stay `null` so an import can tell "no email" from "empty email". The
 * seven subject-wide families are written by the player's GDPR export; the
 * operator import restores only the account and the rows above them. */
export interface ConsoleAccountExport {
  account: Record<string, unknown>;
  devices: { id: string; provider: string }[];
  notes: ConsoleAccountNote[];
  wallet_ledger: ConsoleWalletLedger[];
  storage: { collection: string; key: string; value: unknown; version: string; read: number; write: number; create_time: string; update_time: string }[];
  edges: { destination_id: string; state: number; position: number; metadata: unknown }[];
  purchases: unknown[];
  subscriptions: unknown[];
  inventory: unknown[];
  leaderboard_records: unknown[];
  group_memberships: unknown[];
  notifications: unknown[];
  messages: unknown[];
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

/** `GET /v2/console/group/{id}/export` — one group whole: its row and its members.
 * A download document, so the field names match what the server sends. */
export interface ConsoleGroupExport {
  group: ConsoleGroup;
  members: ConsoleGroupMember[];
}

/** `GET /v2/console/audit/log/users` — the operators who appear in the trail, most
 * active first. The source that fills the audit view's username filter. */
export interface ConsoleAuditLogUsers {
  usernames: string[];
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
  join_required: boolean;
  metadata: unknown;
  size: number;
  max_size: number;
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

/** `GET /v2/console/status` — the server's own facts, secrets absent. */
export interface ConsoleStatus {
  name: string;
  version: string;
  api_port: number;
  uptime_secs: number;
  database: boolean;
  accounts: number;
  console_users: number;
  workers: number;
  seat_ttl_secs: number;
  rate_limit_rps: number;
  rate_limit_burst: number;
  player_token_expiry_secs: number;
  console_token_expiry_secs: number;
  agones: boolean;
}

/** One route the API explorer can call, read from the same OpenAPI document the
 * route macros register. */
export interface ConsoleRoute {
  method: string;
  path: string;
  tags: string[];
  summary: string;
}

/** `POST /v2/console/mfa` — `recovery_codes` are shown exactly once. */
export interface MfaEnrolment {
  secret: string;
  uri: string;
  recovery_codes: string[];
}

/** A delete that can take more than one row says how many went. */
export interface DeletedCount {
  removed: number;
}

/** One joined player, as the match state reports it — the realtime presence. */
export interface ConsoleMatchPresence {
  user_id: string;
  session_id: string;
  username: string;
  node?: string;
}

/** The `api.Match` a console row wraps. Relay matches carry no handler name or tick
 * rate, so those optional fields simply are not there. */
export interface ConsoleMatchApi {
  match_id: string;
  authoritative: boolean;
  label?: string;
  size: number;
  tick_rate?: number;
  handler_name?: string;
}

/** `console.MatchList.Match`: an `api_match` plus the node it runs on (empty on a
 * single-process plane). */
export interface ConsoleMatch {
  api_match: ConsoleMatchApi;
  node: string;
}

export interface ConsoleMatchList {
  matches: ConsoleMatch[];
}

/** `console.MatchState` — the members in the match, plus the tick and handler state
 * a relay match does not hold. */
export interface ConsoleMatchState {
  presences: ConsoleMatchPresence[];
  tick: number;
  state: string;
}

/** `console.RuleRow`: one stored version of one rule set. */
export interface ConsoleRule {
  key: string;
  game_id: string | null;
  version: number;
  sha256: string;
  is_active: boolean;
  body: unknown;
  create_time: string;
}

/** `console.RuleDryRunResult`. */
export interface ConsoleRuleDryRun {
  matched: boolean;
  evaluated_against: unknown;
  error: string | null;
}

/** `console.ConfigVersionRow` in list form (no body). */
export interface ConsoleConfigVersion {
  game_id: string;
  version: number;
  sha256: string;
  is_active: boolean;
  author: string | null;
  create_time: string;
  body?: unknown;
}

/** `console.TimelineEntry`: one merged row of a player's feed. */
export interface ConsoleTimelineEntry {
  id: string;
  kind: string;
  at: string;
  summary: string;
  detail: unknown;
}

export interface ConsoleTimelinePage {
  account_id: string;
  entries: ConsoleTimelineEntry[];
  next_cursor: string | null;
}

/** `console.EconomySummary` and its parts. */
export interface ConsoleEconomySummary {
  from: string;
  to: string;
  window_days: number;
  purchases: { total: number; refunded: number; by_environment: Record<string, number> };
  wallet: { credits: number; debits: number; net_by_currency: Record<string, number> };
  desyncs: number;
  quests: { completed: number; claimed: number };
  market: {
    listings_created: number;
    trades_settled: number;
    wagers_held: number;
    wagers_settled: number;
    wagers_refunded: number;
  };
  empty: boolean;
}

/** `engine_dashboard::models::log::LogLine` over the mirror feed. */
export interface ConsoleLogLine {
  atMs: number;
  level: string;
  target: string;
  message: string;
}

/** `dto::console::quest::QuestDefinitionRow` — one quest the operator wrote. */
export interface ConsoleQuest {
  quest_key: string;
  game_id: string | null;
  title: string;
  goal_kind: string;
  goal_amount: number;
  reward: unknown;
  season: string | null;
  requires_product: string | null;
  active_from: string;
  active_to: string;
  update_time: string;
}

/** `dto::console::quest::SeasonRow` — a season quests can be bound to. */
export interface ConsoleSeason {
  season_key: string;
  game_id: string | null;
  title: string;
  starts_at: string;
  ends_at: string;
  reward_track: unknown;
  update_time: string;
}
