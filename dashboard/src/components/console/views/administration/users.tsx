// Console operators: who may sign in, the role each one holds, and the acts an
// admin performs on one — edit, reset the password, disable, clear a second
// factor, delete.

import { useState } from "react"
import { KeyRound, Pencil, Plus, ShieldOff, Trash2, UserCheck, UserX } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import {
  DataTable,
  Field,
  JsonBlock,
  Mono,
  PageHeading,
  Pager,
  Panel,
} from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleUser } from "@/lib/console"
import { dash, formatTime, shortId } from "@/lib/format"

import { CreateOperatorDialog, PasswordDialog, UpdateOperatorDialog } from "./user-dialogs"

const PAGE_SIZE = 25
const USERS = CONSOLE_ENDPOINTS.users

/** The role badge tone: the wider the access, the louder the badge. */
const ROLE_VARIANT: Record<string, "default" | "secondary" | "outline" | "destructive"> = {
  admin: "destructive",
  developer: "default",
  maintainer: "secondary",
  readonly: "outline",
}

/** The acts that need a dialog; the rest are one confirmation. */
type Action = "edit" | "password" | "disable" | "enable" | "mfa" | "delete"

export function ConsoleUsersView() {
  const [offset, setOffset] = useState(0)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)

  const list = useConsoleQuery<ConsoleUser[]>("console-users", USERS, { limit: PAGE_SIZE, offset })
  // The row is read back from the list rather than held: an edit must show what
  // the server now holds, not the values that were clicked.
  const selected = list.data?.find((user) => user.id === selectedId) ?? null

  const columns: Column<ConsoleUser>[] = [
    { header: "Username", cell: (row) => <span className="font-medium text-foreground">{row.username}</span> },
    { header: "Email", cell: (row) => <Mono>{dash(row.email)}</Mono> },
    { header: "Role", cell: (row) => <Badge variant={ROLE_VARIANT[row.role] ?? "outline"}>{row.role}</Badge> },
    { header: "MFA", cell: (row) => <Mono>{row.mfa_required ? "required" : "off"}</Mono> },
    {
      header: "Status",
      cell: (row) =>
        row.disabled ? <Badge variant="destructive">Disabled</Badge> : <Badge variant="secondary">Active</Badge>,
    },
    { header: "Created", cell: (row) => <Mono>{formatTime(row.create_time)}</Mono> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="Console Users"
        description="Operators who may sign in to this console, and the role each one holds. The config bootstrap account is not listed — it is not a row."
      />

      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_26rem]">
        <Panel
          title="Operators"
          actions={
            <Button size="sm" variant="outline" onClick={() => setCreating(true)}>
              <Plus />
              New operator
            </Button>
          }
        >
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.id}
            onRowClick={(row) => setSelectedId(row.id)}
            selectedKey={selectedId ?? undefined}
            empty="No console users besides the bootstrap account."
          />
          <Pager offset={offset} limit={PAGE_SIZE} count={list.data?.length ?? 0} onChange={setOffset} />
        </Panel>

        {selected === null ? (
          <Panel title="Operator" description="No operator selected">
            <p className="px-4 py-6 text-sm text-muted-foreground">Select a row to inspect it.</p>
          </Panel>
        ) : (
          <OperatorDetail key={selected.id} user={selected} onDeleted={() => setSelectedId(null)} />
        )}
      </div>

      <CreateOperatorDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  )
}

function OperatorDetail({ user, onDeleted }: { user: ConsoleUser; onDeleted: () => void }) {
  const [action, setAction] = useState<Action | null>(null)
  const close = () => setAction(null)

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
            <Badge variant={ROLE_VARIANT[user.role] ?? "outline"}>{user.role}</Badge>
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

      <Panel
        title="Administer"
        description="Credentials sit outside the profile edit by design."
      >
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
        path={`${USERS}/${user.id}/disable`}
        success="Operator disabled."
      />
      <ConfirmAction
        open={action === "enable"}
        onClose={close}
        title="Enable operator"
        message="The operator may sign in again."
        confirmLabel="Enable"
        method="POST"
        path={`${USERS}/${user.id}/enable`}
        success="Operator enabled."
      />
      <ConfirmAction
        open={action === "mfa"}
        onClose={close}
        title="Clear the second factor"
        message="The secret, the recovery codes and the requirement all go; the operator signs in with the password alone until a new factor is enrolled."
        confirmLabel="Clear MFA"
        destructive
        method="DELETE"
        path={`${USERS}/${user.id}/mfa`}
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
        path={`${USERS}/${user.id}`}
        success="Operator deleted."
      />
    </div>
  )
}
