// Console administration: the operators, the audit trail, the ACL templates and
// the server settings — one module per area, re-exported so a page imports them
// the way it imports any other view.

export { AclTemplatesView } from "./acl"
export { AuditLogView } from "./audit"
export { SettingsView } from "./settings"
export { ConsoleUsersView } from "./users"
