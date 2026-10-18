// Vendor IAP maintenance beside the ledger: run a credential through a vendor's
// validation route (`POST /v2/console/iap/{vendor}/validate`) and clear a
// vendor's stored configuration (`DELETE /v2/console/iap/{vendor}/reset`).

import { useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { JsonBlock, Panel } from "@/components/console/data-table"
import { Checkbox, Select, TextArea, TextInput } from "@/components/console/form"
import { useConsoleMutation } from "@/hooks/use-console"

/** `/v2/console/iap` has no entry in the endpoint catalogue yet; these are the
 * five spellings the server's `Vendor::parse` accepts, with the credential
 * field each vendor's player route reads. */
const IAP = "/v2/console/iap"

const VENDORS = [
  { value: "apple", label: "Apple", credential: "receipt", subscriptions: true },
  { value: "google", label: "Google", credential: "purchase_token", subscriptions: true },
  { value: "huawei", label: "Huawei", credential: "purchase_token", subscriptions: true },
  { value: "facebookinstant", label: "Facebook Instant", credential: "signed_request", subscriptions: false },
  { value: "samsung", label: "Samsung", credential: "purchase_token", subscriptions: false },
]

const KIND_OPTIONS = [
  { value: "purchase", label: "Purchase" },
  { value: "subscription", label: "Subscription" },
]

export function IapPanel() {
  const [vendor, setVendor] = useState("apple")
  const [kind, setKind] = useState("purchase")
  const [credential, setCredential] = useState("")
  const [productId, setProductId] = useState("")
  const [userId, setUserId] = useState("")
  const [persist, setPersist] = useState(true)
  const [reply, setReply] = useState<unknown>(null)
  const [resetting, setResetting] = useState<string | null>(null)
  const write = useConsoleMutation()

  const active = VENDORS.find((candidate) => candidate.value === vendor) ?? VENDORS[0]

  function confirm() {
    if (credential === "") return
    // One credential, keyed under the field this vendor's route reads first.
    const body: Record<string, unknown> = { [active.credential]: credential, kind, persist }
    if (productId !== "") body.product_id = productId
    if (userId !== "") body.user_id = userId
    write.mutate(
      {
        method: "POST",
        path: `${IAP}/${encodeURIComponent(active.value)}/validate`,
        body,
        success: "Receipt validated.",
      },
      { onSuccess: (data) => setReply(data ?? null) },
    )
  }

  return (
    <div className="space-y-4">
      <Panel title="Vendor validation" description="POST /v2/console/iap/{vendor}/validate">
        <div className="space-y-3 p-4">
          <div className="grid grid-cols-2 gap-3">
            <Select
              label="Vendor"
              value={vendor}
              onChange={setVendor}
              options={VENDORS.map((candidate) => ({ value: candidate.value, label: candidate.label }))}
            />
            <Select
              label="Kind"
              hint={active.subscriptions ? undefined : `${active.label} has no subscriptions.`}
              value={kind}
              onChange={setKind}
              options={KIND_OPTIONS}
            />
          </div>
          <TextArea
            label={`Credential (${active.credential})`}
            value={credential}
            onChange={setCredential}
            placeholder="receipt / purchase token / signed request"
          />
          <div className="grid grid-cols-2 gap-3">
            <TextInput
              label="Product id"
              hint="Optional. Recorded on the persisted row."
              value={productId}
              onChange={setProductId}
              mono
            />
            <TextInput
              label="Account id"
              hint={persist ? "Required while persisting: the row attaches to this account." : "Unused without persist."}
              value={userId}
              onChange={setUserId}
              mono
            />
          </div>
          <div className="flex items-center justify-between gap-3">
            <Checkbox label="Persist the validated row" checked={persist} onChange={setPersist} />
            <Button size="sm" onClick={confirm} disabled={write.isPending || credential === ""}>
              {write.isPending && <Loader2 className="animate-spin" />}
              Validate
            </Button>
          </div>
        </div>
      </Panel>

      {reply !== null && (
        <Panel title="Validation reply">
          <div className="p-3">
            <JsonBlock value={reply} />
          </div>
        </Panel>
      )}

      <Panel title="Vendor configuration" description="DELETE /v2/console/iap/{vendor}/reset">
        <div className="space-y-2 p-4">
          <p className="text-xs text-muted-foreground">
            Reset clears the stored <span className="font-mono">iap.{`{vendor}`}</span> settings — that vendor falls
            back to unconfigured.
          </p>
          <div className="flex flex-wrap gap-1.5">
            {VENDORS.map((candidate) => (
              <Button key={candidate.value} size="xs" variant="outline" onClick={() => setResetting(candidate.value)}>
                Reset {candidate.label}
              </Button>
            ))}
          </div>
        </div>
      </Panel>

      {/* One dialog for every vendor: the chosen name travels as state. */}
      <ConfirmAction
        open={resetting !== null}
        onClose={() => setResetting(null)}
        title="Reset vendor configuration"
        message={`The stored configuration for "${resetting ?? ""}" is deleted. Validations stop working until it is set again.`}
        confirmLabel="Reset vendor"
        destructive
        method="DELETE"
        path={`${IAP}/${encodeURIComponent(resetting ?? "")}/reset`}
        success="Vendor configuration reset."
      />
    </div>
  )
}
