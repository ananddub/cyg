// Creating a console operator. Kept apart from the profile/password edits: a new
// operator needs a credential chosen here, while editing one never touches it.

import { useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Select, TextInput } from "@/components/console/form"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS } from "@/lib/console"
import { CONSOLE_ROLES } from "@/lib/console/roles"

const USERS = CONSOLE_ENDPOINTS.users

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
    <FormSheet
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
      <Select label="Role" value={role} onChange={setRole} options={CONSOLE_ROLES} />
    </FormSheet>
  )
}
