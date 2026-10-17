// The operator dialogs: create one, edit the profile of one, and hand one a new
// password. Credentials and profile are separate routes by design — editing a
// profile must never be a way to change how someone signs in.

import { useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Checkbox, Select, TextInput } from "@/components/console/form"
import { Modal } from "@/components/console/modal"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleUser } from "@/lib/console"

const USERS = CONSOLE_ENDPOINTS.users

/** The roles the console knows. A name, never a raw ACL document: an operator
 * may only assign permissions somebody already reviewed. */
const ROLE_OPTIONS = [
  { value: "admin", label: "Admin" },
  { value: "developer", label: "Developer" },
  { value: "maintainer", label: "Maintainer" },
  { value: "readonly", label: "Readonly" },
]

/** `POST /v2/console/user` — username, password, an optional email and the role
 * name. The body is `CreateConsoleUserRequest`. */
export function CreateOperatorDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [username, setUsername] = useState("")
  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")
  const [role, setRole] = useState("readonly")
  const write = useConsoleMutation()

  function close() {
    setUsername("")
    setEmail("")
    setPassword("")
    setRole("readonly")
    onClose()
  }

  function confirm() {
    if (username === "" || password === "") return
    write.mutate(
      {
        method: "POST",
        path: USERS,
        body: { username, email, password, role },
        success: "Operator created.",
      },
      { onSuccess: close },
    )
  }

  return (
    <Modal
      open={open}
      title="New operator"
      description="The role decides what this account can see; least privilege is the default."
      onClose={close}
      footer={
        <>
          <Button variant="outline" onClick={close} disabled={write.isPending}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={write.isPending || username === "" || password === ""}>
            {write.isPending && <Loader2 className="animate-spin" />}
            Create
          </Button>
        </>
      }
    >
      <TextInput label="Username" value={username} onChange={setUsername} placeholder="jdoe" autoFocus />
      <TextInput
        label="Email"
        hint="Optional. Shown under the name in the operator list."
        value={email}
        onChange={setEmail}
        placeholder="jdoe@example.com"
      />
      <TextInput label="Password" type="password" value={password} onChange={setPassword} />
      <Select label="Role" value={role} onChange={setRole} options={ROLE_OPTIONS} />
    </Modal>
  )
}

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
        path: `${USERS}/${user.id}`,
        body: { username, email, role, mfa_required: mfaRequired },
        success: "Operator saved.",
      },
      { onSuccess: close },
    )
  }

  return (
    <Modal
      open={open}
      title={`Edit ${user.username}`}
      description="The profile only. Credentials have their own route, so a profile edit cannot change how someone signs in."
      onClose={close}
      footer={
        <>
          <Button variant="outline" onClick={close} disabled={write.isPending}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={write.isPending || username === ""}>
            {write.isPending && <Loader2 className="animate-spin" />}
            Save
          </Button>
        </>
      }
    >
      <TextInput label="Username" value={username} onChange={setUsername} />
      <TextInput label="Email" value={email} onChange={setEmail} />
      <Select label="Role" value={role} onChange={setRole} options={ROLE_OPTIONS} />
      <Checkbox
        label="Require a second factor at sign-in"
        checked={mfaRequired}
        onChange={setMfaRequired}
      />
    </Modal>
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
        path: `${USERS}/${id}/password`,
        body: { password },
        success: "Password set.",
      },
      { onSuccess: close },
    )
  }

  return (
    <Modal
      open={open}
      title="Reset password"
      description="The operator will not be told the new password — it is set here, and passed on out of band."
      onClose={close}
      footer={
        <>
          <Button variant="outline" onClick={close} disabled={write.isPending}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={write.isPending || password === ""}>
            {write.isPending && <Loader2 className="animate-spin" />}
            Set password
          </Button>
        </>
      }
    >
      <TextInput label="New password" type="password" value={password} onChange={setPassword} autoFocus />
    </Modal>
  )
}
