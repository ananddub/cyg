// Editing an existing operator: the profile (username, email, role, the second-factor
// requirement) and, separately, handing them a new password. Credentials and profile
// are separate routes by design — editing a profile must never change how someone signs in.

import { useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Checkbox, Select, TextInput } from "@/components/console/form"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleUser } from "@/lib/console"
import { CONSOLE_ROLES } from "@/lib/console/roles"

const USERS = CONSOLE_ENDPOINTS.users

/** `PUT /v2/console/user/{id}` — the mutable profile: username, email, role and
 * the second-factor requirement. The password is absent from this body by
 * construction; `PasswordDialog` is the only route that touches it. */
export function UpdateOperatorDialog({
  user,
  open,
  onClose,
}: {
  user: ConsoleUser
  open: boolean
  onClose: () => void
}) {
  const [username, setUsername] = useState(user.username)
  const [email, setEmail] = useState(user.email)
  const [role, setRole] = useState(user.role)
  const [mfaRequired, setMfaRequired] = useState(user.mfa_required)
  const write = useConsoleMutation()

  function close() {
    setUsername(user.username)
    setEmail(user.email)
    setRole(user.role)
    setMfaRequired(user.mfa_required)
    onClose()
  }

  function confirm() {
    if (username === "") return
    write.mutate(
      {
        method: "PUT",
        path: `${USERS}/${encodeURIComponent(user.id)}`,
        body: { username, email, role, mfa_required: mfaRequired },
        success: "Operator saved.",
      },
      { onSuccess: close },
    )
  }

  return (
    <FormSheet
      open={open}
      title={`Edit ${user.username}`}
      description="The profile only. Credentials have their own route, so a profile edit cannot change how someone signs in."
      onClose={close}
      footer={
        <Button onClick={confirm} disabled={write.isPending || username === ""}>
          {write.isPending && <Loader2 className="animate-spin" />}
          Save
        </Button>
      }
    >
      <TextInput label="Username" value={username} onChange={setUsername} />
      <TextInput label="Email" value={email} onChange={setEmail} />
      <Select label="Role" value={role} onChange={setRole} options={CONSOLE_ROLES} />
      <Checkbox
        label="Require a second factor at sign-in"
        checked={mfaRequired}
        onChange={setMfaRequired}
      />
    </FormSheet>
  )
}

/** `POST /v2/console/user/{id}/password` — an operator handing someone a new
 * credential. Nothing else travels in this body. */
export function PasswordDialog({
  id,
  open,
  onClose,
}: {
  id: string
  open: boolean
  onClose: () => void
}) {
  const [password, setPassword] = useState("")
  const write = useConsoleMutation()

  function close() {
    setPassword("")
    onClose()
  }

  function confirm() {
    if (password === "") return
    write.mutate(
      {
        method: "POST",
        path: `${USERS}/${encodeURIComponent(id)}/password`,
        body: { password },
        success: "Password set.",
      },
      { onSuccess: close },
    )
  }

  return (
    <FormSheet
      open={open}
      title="Reset password"
      description="The operator will not be told the new password — it is set here, and passed on out of band."
      onClose={close}
      footer={
        <Button onClick={confirm} disabled={write.isPending || password === ""}>
          {write.isPending && <Loader2 className="animate-spin" />}
          Set password
        </Button>
      }
    >
      <TextInput label="New password" type="password" value={password} onChange={setPassword} autoFocus />
    </FormSheet>
  )
}
