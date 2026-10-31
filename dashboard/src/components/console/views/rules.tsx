// The rules editor: rule sets are data (a typed expression tree), versions are
// append-only, and publishing marks one version active so every publish is
// reversible. The dry run evaluates a stored or inline body against a real
// player's stat map before anyone bets gameplay on it.

import { useMemo, useState } from "react"
import { ChevronDown } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { Field, JsonBlock, Mono, PageHeading, Panel } from "@/components/console/data-table"
import { useConsoleMutation, useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleRule, type ConsoleRuleDryRun } from "@/lib/console"
import { formatTime } from "@/lib/format"
import { VisualRuleBuilder } from "@/components/console/rules/VisualRuleBuilder"

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

  const handleVisualDryRun = async (ruleBody: unknown, context: unknown) => {
    try {
      const result = await dryRun.mutateAsync({
        method: "POST",
        path: CONSOLE_ENDPOINTS.ruleDryRun,
        body: {
          body: ruleBody,
          context,
        },
        success: "Dry run evaluated",
      })
      return { matched: result.matched, error: result.error ?? undefined }
    } catch (err: unknown) {
      return { matched: false, error: err instanceof Error ? err.message : "Dry run failed" }
    }
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
            {creating ? "Close editor" : "New rule (Visual Builder)"}
          </Button>
        }
      >
        {creating && (
          <div className="border-b border-border/40 bg-muted/20 p-4 space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="w-full max-w-sm">
                <Field label="Rule Key Identifier">
                  <input
                    className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm font-mono"
                    placeholder="e.g. quest_centurion or vip_wall_breaker"
                    value={key}
                    onChange={(event) => setKey(event.target.value)}
                  />
                </Field>
              </div>
              <div className="flex items-center gap-2">
                <Button size="sm" variant="ghost" onClick={() => setCreating(false)}>
                  Cancel
                </Button>
                <Button size="sm" onClick={submitDraft} disabled={key.trim() === ""}>
                  Draft version
                </Button>
              </div>
            </div>

            <VisualRuleBuilder
              initialJson={body}
              onChange={setBody}
              onDryRun={handleVisualDryRun}
            />
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
                <div key={ruleKey} className="group/row transition-colors hover:bg-muted/40">
                  <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-3">
                    <button
                      type="button"
                      className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 text-left"
                      onClick={() => setOpenKey(expanded ? null : ruleKey)}
                    >
                      <ChevronDown
                        className={`size-4 shrink-0 text-muted-foreground transition-transform duration-150 ${
                          expanded ? "rotate-180 text-foreground" : ""
                        }`}
                      />
                      <span className="font-medium text-foreground transition-colors group-hover/row:text-sky-600 dark:group-hover/row:text-sky-400">
                        {ruleKey}
                      </span>
                      <Mono className="text-muted-foreground">
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
                    <div className="space-y-2 px-4 pb-3">
                      {versions.map((rule) => (
                        <div key={rule.version} className="rounded-md border border-border/40 p-3 transition-colors hover:border-border hover:bg-muted/30">
                          <div className="mb-2 flex items-center justify-between">
                            <div className="flex items-center gap-3">
                              <Mono>v{rule.version}</Mono>
                              {rule.is_active ? (
                                <Mono className="text-emerald-300">active</Mono>
                              ) : (
                                <Mono className="text-muted-foreground">draft</Mono>
                              )}
                              <Mono className="text-muted-foreground">{rule.sha256.slice(0, 12)}</Mono>
                              <Mono className="text-muted-foreground">{formatTime(rule.create_time)}</Mono>
                            </div>
                            <Button
                              size="sm"
                              variant="ghost"
                              className="h-7 text-xs text-sky-400 hover:text-sky-300"
                              onClick={() => {
                                setKey(ruleKey)
                                setBody(JSON.stringify(rule.body, null, 2))
                                setCreating(true)
                                window.scrollTo({ top: 0, behavior: "smooth" })
                              }}
                            >
                              Load in Visual Editor
                            </Button>
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
