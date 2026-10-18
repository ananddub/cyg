// One operator's detail: the profile read-back and the acts an admin performs —
// edit, reset the password, enable/disable, force the second-factor requirement,
// reset or clear the factor, delete. Every one is a confirmation and a route.

import { useState } from "react"
import { KeyRound, Pencil, RefreshCw, ShieldCheck, ShieldOff, Trash2, UserCheck, UserX } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { Field, JsonBlock, Panel } from "@/components/console/data-table"
import { CONSOLE_ENDPOINTS, type ConsoleUser } from "@/lib/console"
import { roleVariant } from "@/lib/console/roles"
import { formatTime, shortId } from "@/lib/format"

import { PasswordDialog, UpdateOperatorDialog } from "./user-edit-dialog"

const USERS = CONSOLE_ENDPOINTS.users

/** The acts that need a dialog; the rest are one confirmation. */
type Action =
  | "edit"
  | "password"
  | "disable"
  | "enable"
  | "mfa-require"
  | "mfa-reset"
  | "mfa"
  | "delete"

export function OperatorDetail({ user, onDeleted }: { user: ConsoleUser; onDeleted: () => void }) {
  const [action, setAction] = useState<Action | null>(null)
  const close = () => setAction(null)

  // The bootstrap config superadmin is never a listed row — the console's user list
  // reads `console_user` records, and the superadmin is a config account. So every
  // operator reached here can hold a second factor, and the require/reset affordances
  // are safe to offer. The reset is offered only where a factor is expected
  // (`mfa_required`), since there is otherwise nothing enrolled to reset.
  return (
    <div className="space-y-4">
      <Panel
        title={user.username}
        description={user.email === "" ? shortId(user.id) : user.email}
        actions={
          <Button size="sm" variant="outline" onClick={() => setAction("edit")}>
            <Pencil />
            Edit
          </Button>
        }
      >
        <div className="px-4 py-2">
          <Field label="Role">
            <Badge variant={roleVariant(user.role)}>{user.role}</Badge>
          </Field>
          <Field label="Second factor">{user.mfa_required ? "Required" : "Not enrolled"}</Field>
          <Field label="Status">{user.disabled ? "Disabled" : "Active"}</Field>
          <Field label="Created">{formatTime(user.create_time)}</Field>
          <Field label="Updated">{formatTime(user.update_time)}</Field>
        </div>
      </Panel>

      <Panel title="Metadata">
        <div className="p-3">
          <JsonBlock value={user.metadata} />
        </div>
      </Panel>

      <Panel title="Permissions" description="The ACL document the role name is derived from.">
        <div className="p-3">
          <JsonBlock value={user.acl} />
        </div>
      </Panel>

      <Panel title="Administer" description="Credentials sit outside the profile edit by design.">
        <div className="flex flex-wrap gap-2 p-4">
          <Button size="sm" variant="outline" onClick={() => setAction("password")}>
            <KeyRound />
            Reset password
          </Button>
          {user.disabled ? (
            <Button size="sm" variant="outline" onClick={() => setAction("enable")}>
              <UserCheck />
              Enable
            </Button>
          ) : (
            <Button size="sm" variant="outline" onClick={() => setAction("disable")}>
              <UserX />
              Disable
            </Button>
          )}
          <Button size="sm" variant="outline" onClick={() => setAction("mfa-require")}>
            <ShieldCheck />
            {user.mfa_required ? "Lift MFA requirement" : "Require MFA"}
          </Button>
          {user.mfa_required && (
            <Button size="sm" variant="outline" onClick={() => setAction("mfa-reset")}>
              <RefreshCw />
              Reset MFA
            </Button>
          )}
          <Button size="sm" variant="outline" onClick={() => setAction("mfa")}>
            <ShieldOff />
            Clear MFA
          </Button>
          <Button size="sm" variant="destructive" onClick={() => setAction("delete")}>
            <Trash2 />
            Delete
          </Button>
        </div>
      </Panel>

      <UpdateOperatorDialog user={user} open={action === "edit"} onClose={close} />
      <PasswordDialog id={user.id} open={action === "password"} onClose={close} />
      <ConfirmAction
        open={action === "disable"}
        onClose={close}
        title="Disable operator"
        message="Sign-in stops at once. A token already issued keeps working until it expires — a sign-in is the revocation boundary the console documents."
        confirmLabel="Disable"
        destructive
        method="POST"
        path={`${USERS}/${encodeURIComponent(user.id)}/disable`}
        success="Operator disabled."
      />
      <ConfirmAction
        open={action === "enable"}
        onClose={close}
        title="Enable operator"
        message="The operator may sign in again."
        confirmLabel="Enable"
        method="POST"
        path={`${USERS}/${encodeURIComponent(user.id)}/enable`}
        success="Operator enabled."
      />
      {/* One toggle: POST forces the requirement on, DELETE lifts it, keyed off the
          current flag. It writes only the requirement, never an enrolment. */}
      <ConfirmAction
        open={action === "mfa-require"}
        onClose={close}
        title={user.mfa_required ? "Lift the MFA requirement" : "Require a second factor"}
        message={
          user.mfa_required
            ? "The operator is no longer held to a second factor at sign-in. Any secret already enrolled is kept until reset or cleared."
            : "The operator must present a second factor at sign-in. If none is enrolled yet, they are told to enrol one — they cannot be locked out silently."
        }
        confirmLabel={user.mfa_required ? "Lift requirement" : "Require MFA"}
        destructive={user.mfa_required}
        method={user.mfa_required ? "DELETE" : "POST"}
        path={`${USERS}/${encodeURIComponent(user.id)}/mfa/require`}
        success={user.mfa_required ? "MFA requirement lifted." : "MFA is now required."}
      />
      {user.mfa_required && (
        <ConfirmAction
          open={action === "mfa-reset"}
          onClose={close}
          title="Reset the second factor"
          message="The enrolled secret and its recovery codes go, but the requirement stays — the operator re-enrols at their next sign-in."
          confirmLabel="Reset MFA"
          destructive
          method="DELETE"
          path={`${USERS}/${encodeURIComponent(user.id)}/mfa/reset`}
          success="Second factor reset."
        />
      )}
      <ConfirmAction
        open={action === "mfa"}
        onClose={close}
        title="Clear the second factor"
        message="The secret, the recovery codes and the requirement all go; the operator signs in with the password alone until a new factor is enrolled."
        confirmLabel="Clear MFA"
        destructive
        method="DELETE"
        path={`${USERS}/${encodeURIComponent(user.id)}/mfa`}
        success="Second factor cleared."
      />
      <ConfirmAction
        open={action === "delete"}
        onClose={close}
        onDone={onDeleted}
        title="Delete operator"
        message="The console account goes for good. The audit trail keeps the entries it wrote, because that is what a trail is for."
        confirmLabel="Delete"
        destructive
        method="DELETE"
        path={`${USERS}/${encodeURIComponent(user.id)}`}
        success="Operator deleted."
      />
    </div>
  )
}
