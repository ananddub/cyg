// Import the document an Export writes, so an account can be moved between
// deployments — or restored from a file. The id and the username must be free,
// and the password is never exported, so a restored account has none until one
// is set.

import { useRef, useState, type ChangeEvent } from "react"
import { Loader2, Upload } from "lucide-react"
import { toast } from "sonner"

import { TextArea } from "@/components/console/form"
import { parseJson } from "@/components/console/json"
import { Modal } from "@/components/console/modal"
import { Button } from "@/components/ui/button"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS } from "@/lib/console"

const ACCOUNT = CONSOLE_ENDPOINTS.accounts

export function ImportDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [text, setText] = useState("")
  const file = useRef<HTMLInputElement>(null)
  const write = useConsoleMutation()

  function close() {
    setText("")
    if (file.current !== null) file.current.value = ""
    onClose()
  }

  async function choose(event: ChangeEvent<HTMLInputElement>) {
    const chosen = event.target.files?.[0]
    if (chosen === undefined) return
    setText(await chosen.text())
  }

  function confirm() {
    const document_ = parseJson(text)
    if ("error" in document_) {
      toast.error(`The document must be JSON: ${document_.error}`)
      return
    }
    if (document_.value === undefined) {
      toast.error("Paste an export document, or choose its file.")
      return
    }
    write.mutate(
      {
        method: "POST",
        path: `${ACCOUNT}/import`,
        body: document_.value,
        success: "Account imported.",
      },
      { onSuccess: close },
    )
  }

  return (
    <Modal
      open={open}
      title="Import an account"
      description="The document an Export writes back."
      onClose={close}
      footer={
        <>
          <Button variant="outline" onClick={close} disabled={write.isPending}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={write.isPending}>
            {write.isPending ? <Loader2 className="animate-spin" /> : <Upload />}
            Import
          </Button>
        </>
      }
    >
      <TextArea
        label="Export document"
        rows={10}
        value={text}
        onChange={setText}
        placeholder='{"account": { … }, "devices": [], …}'
      />
      <label className="block space-y-1.5">
        <span className="block text-xs font-medium text-muted-foreground">Or choose its file</span>
        <input
          ref={file}
          type="file"
          accept="application/json,.json"
          onChange={choose}
          className="text-xs text-muted-foreground"
        />
      </label>
      <p className="text-[11px] text-muted-foreground">
        The id and the username must be free, and a conflicting provider id is refused by name.
      </p>
    </Modal>
  )
}
