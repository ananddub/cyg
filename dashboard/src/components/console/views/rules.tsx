// The rules editor: rule sets are data (a typed expression tree), versions are
// append-only, and publishing marks one version active so every publish is
// reversible. The dry run evaluates a stored or inline body against a real
// player's stat map before anyone bets gameplay on it.

import { useMemo, useState } from "react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { Field, JsonBlock, Mono, PageHeading, Panel } from "@/components/console/data-table"
import { useConsoleMutation, useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleRule, type ConsoleRuleDryRun } from "@/lib/console"
import { formatTime } from "@/lib/format"

const SAMPLE = JSON.stringify(
  {
    all: [
      { field: "kills", op: "gte", value: 10 },
      {
        any: [
          { field: "rank", op: "eq", value: "gold" },
          { field: "rank", op: "eq", value: "platinum" },
        ],
      },
    ],
  },
  null,
  2,
)

export function RulesView() {
  const rules = useConsoleQuery<ConsoleRule[]>("rules", CONSOLE_ENDPOINTS.rules)
  const write = useConsoleMutation()

  const [creating, setCreating] = useState(false)
  const [key, setKey] = useState("")
  const [body, setBody] = useState(SAMPLE)
  const [openKey, setOpenKey] = useState<string | null>(null)
  const [deleteKey, setDeleteKey] = useState<string | null>(null)

  const [dryKey, setDryKey] = useState("")
  const [dryContext, setDryContext] = useState("{}\n")
  const [dryResult, setDryResult] = useState<ConsoleRuleDryRun | null>(null)
  const [dryError, setDryError] = useState<string | null>(null)
  const dryRun = useConsoleMutation<ConsoleRuleDryRun>()

  const groups = useMemo(() => {
    const byKey = new Map<string, ConsoleRule[]>()
    for (const rule of rules.data ?? []) {
      const versions = byKey.get(rule.key) ?? []
      versions.push(rule)
      byKey.set(rule.key, versions)
    }
    return [...byKey.entries()]
  }, [rules.data])

  function submitDraft() {
    let parsed: unknown
    try {
      parsed = JSON.parse(body)
    } catch {
      return
    }
    write.mutate(
      {
        method: "POST",
        path: CONSOLE_ENDPOINTS.rules,
        body: { key: key.trim(), body: parsed },
        success: `Drafted ${key.trim()}`,
      },
      {
        onSuccess: () => {
          setCreating(false)
          setKey("")
        },
      },
    )
  }

  function runDry() {
    let context: unknown
    try {
      context = JSON.parse(dryContext)
    } catch {
      setDryError("The context is not valid JSON.")
      return
    }
    setDryError(null)
    dryRun.mutate(
      {
        method: "POST",
        path: CONSOLE_ENDPOINTS.ruleDryRun,
        body: {
          key: dryKey.trim() === "" ? undefined : dryKey.trim(),
          context,
        },
        success: "Dry run complete",
      },
      {
        onSuccess: (result) => setDryResult(result),
        onError: (error) => setDryError(error.message),
      },
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <PageHeading
        title="Rules"
        description="Game rules as data: an all/any tree of field comparisons over a player's stats. Publishing marks one version active; older versions stay for the rollback."
      />
      <Panel
        title="Rule sets"
        actions={
          <Button size="sm" onClick={() => setCreating((value) => !value)}>
            New rule
          </Button>
        }
      >
        {creating && (
          <div className="border-b border-border/40 bg-muted/20 p-4">
            <div className="grid gap-3 md:grid-cols-[1fr_3fr]">
              <Field label="Key">
                <input
                  className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
                  placeholder="vip_wall_breaker"
                  value={key}
                  onChange={(event) => setKey(event.target.value)}
                />
              </Field>
              <Field label="Body (all / any / not tree)">
                <textarea rows={8} className="w-full rounded-md border border-border bg-background p-3 font-mono text-xs" value={body} onChange={(event) => setBody(event.target.value)} />
              </Field>
            </div>
            <div className="mt-3 flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setCreating(false)}>
                Cancel
              </Button>
              <Button size="sm" onClick={submitDraft} disabled={key.trim() === ""}>
                Draft version
              </Button>
            </div>
          </div>
        )}
        {rules.isError ? (
          <p className="px-4 py-6 text-center text-sm text-red-400">{queryError(rules.error)}</p>
        ) : groups.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-muted-foreground">
            No rules yet. A rule is drafted as a version first, published second.
          </p>
        ) : (
          <div className="divide-y divide-border/40">
            {groups.map(([ruleKey, versions]) => {
              const active = versions.find((rule) => rule.is_active) ?? null
              const expanded = openKey === ruleKey
              return (
                <div key={ruleKey} className="px-4 py-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <button
                      type="button"
                      className="text-left"
                      onClick={() => setOpenKey(expanded ? null : ruleKey)}
                    >
                      <span className="font-medium text-foreground">{ruleKey}</span>
                      <Mono className="ml-3 text-muted-foreground">
                        {versions.length} version{versions.length === 1 ? "" : "s"}
                      </Mono>
                    </button>
                    <div className="flex items-center gap-2">
                      {active === null ? (
                        <Mono className="text-amber-300">unpublished</Mono>
                      ) : (
                        <Mono className="text-emerald-300">v{active.version} live</Mono>
                      )}
                      {active !== null && active.version > 1 && (
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() =>
                            write.mutate({
                              method: "POST",
                              path: `${CONSOLE_ENDPOINTS.rules}/${ruleKey}/publish/${active.version - 1}`,
                              success: `Rolled ${ruleKey} back to v${active.version - 1}`,
                            })
                          }
                        >
                          Roll back
                        </Button>
                      )}
                      {versions
                        .filter((rule) => !rule.is_active)
                        .slice(0, 1)
                        .map((rule) => (
                          <Button
                            key={rule.version}
                            size="sm"
                            variant="outline"
                            onClick={() =>
                              write.mutate({
                                method: "POST",
                                path: `${CONSOLE_ENDPOINTS.rules}/${ruleKey}/publish/${rule.version}`,
                                success: `Published ${ruleKey} v${rule.version}`,
                              })
                            }
                          >
                            Publish v{rule.version}
                          </Button>
                        ))}
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => setDeleteKey(ruleKey)}
                        aria-label={`Delete ${ruleKey}`}
                      >
                        Delete
                      </Button>
                    </div>
                  </div>
                  {expanded && (
                    <div className="mt-3 space-y-2">
                      {versions.map((rule) => (
                        <div key={rule.version} className="rounded-md border border-border/40 p-3">
                          <div className="mb-2 flex items-center gap-3">
                            <Mono>v{rule.version}</Mono>
                            {rule.is_active ? (
                              <Mono className="text-emerald-300">active</Mono>
                            ) : (
                              <Mono className="text-muted-foreground">draft</Mono>
                            )}
                            <Mono className="text-muted-foreground">{rule.sha256.slice(0, 12)}</Mono>
                            <Mono className="text-muted-foreground">{formatTime(rule.create_time)}</Mono>
                          </div>
                          <JsonBlock value={rule.body} />
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        )}
      </Panel>

      <Panel title="Dry run">
        <p className="px-4 pt-3 text-sm text-muted-foreground">
          Evaluate a stored rule (by key) against a context object — a player's stat map, or
          anything else the rule reads.
        </p>
        <div className="grid gap-3 p-4 md:grid-cols-3">
          <Field label="Rule key">
            <input
              className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
              placeholder="leave empty for inline body"
              value={dryKey}
              onChange={(event) => setDryKey(event.target.value)}
            />
          </Field>
          <Field label="Context (JSON)">
            <textarea rows={4} className="w-full rounded-md border border-border bg-background p-3 font-mono text-xs" value={dryContext} onChange={(event) => setDryContext(event.target.value)} />
          </Field>
          <div className="flex items-end">
            <Button size="sm" onClick={runDry}>
              Evaluate
            </Button>
          </div>
        </div>
        {dryError !== null && (
          <p className="px-4 pb-3 text-sm text-red-400">{dryError}</p>
        )}
        {dryResult !== null && (
          <div className="px-4 pb-4">
            <p className={dryResult.matched ? "text-sm text-emerald-300" : "text-sm text-amber-300"}>
              {dryResult.matched ? "The rule matches." : "The rule does not match."}
              {dryResult.error !== null && ` (${dryResult.error})`}
            </p>
          </div>
        )}
      </Panel>

      <ConfirmAction
        open={deleteKey !== null}
        title={`Delete ${deleteKey ?? ""}`}
        message="Every stored version of this rule is removed. Nothing evaluates after this."
        confirmLabel="Delete rule"
        destructive
        method="DELETE"
        path={deleteKey === null ? "" : `${CONSOLE_ENDPOINTS.rules}/${deleteKey}`}
        success={`Deleted ${deleteKey ?? ""}`}
        onClose={() => setDeleteKey(null)}
      />
    </div>
  )
}
