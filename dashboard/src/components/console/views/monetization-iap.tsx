// Vendor IAP maintenance: run a credential through a vendor's validation route
// (`POST /v2/console/iap/{vendor}/validate`) and clear a vendor's stored
// configuration (`DELETE /v2/console/iap/{vendor}/reset`).

import { useState } from "react"
import { Loader2, ShieldCheck, Trash2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
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
  { value: "purchase", label: "One-off purchase" },
  { value: "subscription", label: "Subscription" },
]

function RouteChip({ method, path }: { method: string; path: string }) {
  return (
    <code className="hidden rounded-md border border-border/60 bg-muted/60 px-2 py-1 font-mono text-[10px] font-medium text-muted-foreground sm:block">
      {method} {path}
    </code>
  )
}

export function VendorValidationPanel() {
  const [vendor, setVendor] = useState("apple")
  const [kind, setKind] = useState("purchase")
  const [credential, setCredential] = useState("")
  const [productId, setProductId] = useState("")
  const [userId, setUserId] = useState("")
  const [persist, setPersist] = useState(true)
  const [reply, setReply] = useState<unknown>(null)
  const write = useConsoleMutation()

  const active = VENDORS.find((candidate) => candidate.value === vendor) ?? VENDORS[0]

  function chooseVendor(next: string) {
    setVendor(next)
    if (!VENDORS.find((candidate) => candidate.value === next)?.subscriptions) setKind("purchase")
  }

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
      <Panel
        title="Validate a receipt"
        description="Run one store credential through the vendor's verification API."
        actions={<RouteChip method="POST" path="/v2/console/iap/{vendor}/validate" />}
      >
        <div className="space-y-4 p-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <Select
              label="Store"
              value={vendor}
              onChange={chooseVendor}
              options={VENDORS.map((candidate) => ({ value: candidate.value, label: candidate.label }))}
            />
            <Select
              label="Purchase kind"
              hint={active.subscriptions ? undefined : `${active.label} sells one-off purchases only.`}
              value={kind}
              onChange={setKind}
              options={KIND_OPTIONS}
            />
          </div>

          <TextArea
            label={`Credential · ${active.credential}`}
            hint={`Sent as the "${active.credential}" field this store's player route reads.`}
            rows={3}
            value={credential}
            onChange={setCredential}
            placeholder="Receipt, purchase token or signed request"
          />

          <div className="grid gap-3 sm:grid-cols-2">
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

          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border/60 pt-3">
            <Checkbox label="Persist the validated purchase" checked={persist} onChange={setPersist} />
            <Button size="sm" onClick={confirm} disabled={write.isPending || credential === ""}>
              {write.isPending ? <Loader2 className="animate-spin" /> : <ShieldCheck />}
              Validate receipt
            </Button>
          </div>
        </div>
      </Panel>

      {reply !== null && (
        <Panel title="Validation reply" description="The vendor's response, exactly as the server returned it.">
          <div className="p-3 sm:p-4">
            <JsonBlock value={reply} />
          </div>
        </Panel>
      )}
    </div>
  )
}

export function VendorConfigurationPanel() {
  const [resetting, setResetting] = useState<string | null>(null)

  return (
    <div className="space-y-4">
      <Panel
        title="Stored store credentials"
        description="Clear a vendor's saved configuration so it falls back to unconfigured."
        actions={<RouteChip method="DELETE" path="/v2/console/iap/{vendor}/reset" />}
      >
        <ul className="divide-y divide-border/60">
          {VENDORS.map((candidate) => (
            <li key={candidate.value} className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium text-foreground">{candidate.label}</span>
                  <Badge variant="outline">
                    {candidate.subscriptions ? "One-off + subscriptions" : "One-off only"}
                  </Badge>
                </div>
                <p className="mt-0.5 text-xs text-muted-foreground">
                  Credential <span className="font-mono text-foreground/80">{candidate.credential}</span>
                </p>
              </div>
              <Button size="sm" variant="outline" onClick={() => setResetting(candidate.value)}>
                <Trash2 />
                Reset
              </Button>
            </li>
          ))}
        </ul>
        <p className="border-t border-border/60 px-4 py-3 text-[11px] text-muted-foreground">
          Resetting deletes the stored settings for that store. Receipt validation stops working until it is configured
          again.
        </p>
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
