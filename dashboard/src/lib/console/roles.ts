// The console operator role catalogue, kept out of the view components: a module
// that exports data (not just components) opts every importer out of fast
// refresh, so the names and badge tones live here where a view can read them.

/** The badge tone a role name earns. The wider the access, the louder the badge. */
type RoleVariant = "default" | "secondary" | "outline" | "destructive";

/** The roles the console knows, in the order a selector lists them. A name, never
 * a raw ACL document: an operator may only assign permissions somebody reviewed. */
export const CONSOLE_ROLES: readonly { value: string; label: string }[] = [
  { value: "admin", label: "Admin" },
  { value: "developer", label: "Developer" },
  { value: "maintainer", label: "Maintainer" },
  { value: "readonly", label: "Readonly" },
]

const ROLE_VARIANT: Record<string, RoleVariant> = {
  admin: "destructive",
  developer: "default",
  maintainer: "secondary",
  readonly: "outline",
}

/** The badge tone for a role name, falling back to a neutral outline for anything
 * the catalogue does not name. */
export function roleVariant(role: string): RoleVariant {
  return ROLE_VARIANT[role] ?? "outline"
}
