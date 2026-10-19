// The ACL template dialog, split from `acl.tsx`: create a template or replace
// the one that is selected. The body is the same document an operator row
// carries, so what is edited here is exactly what a role would be derived from.

import { useState } from "react"
import { Loader2 } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { JsonTextArea, TextInput } from "@/components/console/form"
import { jsonText, parseJson } from "@/components/console/json"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleAclTemplate } from "@/lib/console"

const ACL = CONSOLE_ENDPOINTS.aclTemplates

/** The document a role is derived from: `role` wins, then `admin`, then readonly. */
const DEFAULT_ACL = '{\n  "role": 3,\n  "admin": false\n}'

export function TemplateDialog({
  open,
  onClose,
  template,
}: {
  open: boolean
  onClose: () => void
  template: ConsoleAclTemplate | null
}) {
  const [name, setName] = useState(template?.name ?? "")
  const [description, setDescription] = useState(template?.description ?? "")
  const [acl, setAcl] = useState(template === null ? DEFAULT_ACL : jsonText(template.acl))
  const write = useConsoleMutation()
  const replacing = template !== null

  function close() {
    setName(template?.name ?? "")
    setDescription(template?.description ?? "")
    setAcl(template === null ? DEFAULT_ACL : jsonText(template.acl))
    onClose()
  }

  function confirm() {
    const parsed = parseJson(acl)
    if ("error" in parsed) {
      toast.error(`The ACL must be JSON: ${parsed.error}`)
      return
    }
    write.mutate(
      {
        method: replacing ? "PUT" : "POST",
        path: replacing ? `${ACL}/${template.id}` : ACL,
        body: { name, description, acl: parsed.value },
        success: replacing ? "Template saved." : "Template created.",
      },
      { onSuccess: close },
    )
  }

  return (
    <FormSheet
      open={open}
      title={replacing ? `Edit ${template.name}` : "New ACL template"}
      description="The document an operator row would carry, kept under a name so it can be reused."
      onClose={close}
      footer={
        <>
          <Button variant="outline" onClick={close} disabled={write.isPending}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={write.isPending}>
            {write.isPending && <Loader2 className="animate-spin" />}
            Save
          </Button>
        </>
      }
    >
      <TextInput label="Name" value={name} onChange={setName} placeholder="support" />
      <TextInput
        label="Description"
        value={description}
        onChange={setDescription}
        placeholder="What this template is for"
      />
      <JsonTextArea label="ACL" rows={5} value={acl} onChange={setAcl} />
      <p className="text-[11px] text-muted-foreground">
        <code>role</code> is 1 admin, 2 developer, 3 maintainer, 4 readonly. An unrecognised document falls back to
        readonly — least privilege, never more.
      </p>
    </FormSheet>
  )
}
