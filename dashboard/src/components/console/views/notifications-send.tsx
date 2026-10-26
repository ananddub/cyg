// The message the console sends a player. The sender is taken from the token,
// never the body, so a notification cannot claim someone else sent it.

import { useState } from "react"
import { Loader2, Send } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { JsonTextArea, TextInput } from "@/components/console/form"
import { parseJson } from "@/components/console/json"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS } from "@/lib/console"

/** The message the console sends a player. The sender is taken from the token,
 * never the body, so a notification cannot claim someone else sent it. */
export function SendNotificationDialog({
  userId,
  open,
  onClose,
}: {
  userId: string
  open: boolean
  onClose: () => void
}) {
  const [subject, setSubject] = useState("")
  const [content, setContent] = useState("{}")
  const [code, setCode] = useState("0")
  const write = useConsoleMutation()

  function close() {
    setSubject("")
    setContent("{}")
    setCode("0")
    onClose()
  }

  function confirm() {
    const parsed = parseJson(content)
    if ("error" in parsed) {
      toast.error(`The content must be JSON: ${parsed.error}`)
      return
    }
    const numeric = Number(code.trim() === "" ? "0" : code.trim())
    if (!Number.isInteger(numeric) || numeric < -32768 || numeric > 32767) {
      toast.error("The code must be a whole number between -32768 and 32767.")
      return
    }
    write.mutate(
      {
        method: "POST",
        path: CONSOLE_ENDPOINTS.notifications,
        body: { user_id: userId, subject, content: parsed.value, code: numeric },
        success: "Notification sent.",
      },
      { onSuccess: close },
    )
  }

  return (
    <FormSheet
      open={open}
      title="Send a notification"
      description={`To ${userId}. The sender is the signed-in operator, taken from the token.`}
      onClose={close}
      footer={
        <Button onClick={confirm} disabled={write.isPending}>
          {write.isPending ? <Loader2 className="animate-spin" /> : <Send />}
          Send
        </Button>
      }
    >
      <TextInput label="Subject" value={subject} onChange={setSubject} placeholder="What this is about" />
      <JsonTextArea label="Content" rows={5} value={content} onChange={setContent} />
      <TextInput
        label="Code"
        hint="The numeric code the client branches on; 0 when there is none."
        value={code}
        onChange={setCode}
        mono
      />
    </FormSheet>
  )
}
