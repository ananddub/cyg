// The two-step "Register Game" sheet.
//
// Step 1 picks the engine, which is the only decision that matters: the server
// derives `backend_mode` from it (unity/unreal/godot → C# Relay, web/custom →
// server-side WASM). We send `engine_type` alone and never a `backend_mode` —
// the mode is the server's, echoed back in the answer. Step 2 collects the name
// and the four config fields the revised plan keeps (tick rate, capacity, the
// persistent-rooms toggle and its auto-close window).

import { useState } from "react"
import { ArrowLeft, ArrowRight, Check, Loader2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox, Select, TextInput } from "@/components/console/form"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"

import {
  GAMES_ENDPOINT,
  isWasmMode,
  type BackendMode,
} from "./routes"
import {
  CAPACITY_OPTIONS,
  ENGINE_CHOICES,
  TICK_RATE_OPTIONS,
  WASM_PREREQ_NOTE,
} from "./options"
import type { CreatedGame } from "./model"

/** A radio card: the engine, the mode badge the server will tag it with, and —
 * for the WASM engines only — the one-line note that they need a room module. */
function EngineCard({
  choice,
  selected,
  onSelect,
}: {
  choice: (typeof ENGINE_CHOICES)[number]
  selected: boolean
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={`flex w-full flex-col gap-1.5 rounded-lg border p-3 text-left transition-colors ${
        selected
          ? "border-primary bg-primary/5 ring-1 ring-primary"
          : "border-border hover:border-primary/50"
      }`}
    >
      <span className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-2 text-sm font-medium text-foreground">
          {selected && <Check className="size-4 shrink-0 text-primary" />}
          {choice.label}
          {choice.value === "unity" && (
            <Badge variant="outline" className="ml-1">
              Recommended
            </Badge>
          )}
        </span>
        {selected && (
          <Badge variant="secondary" className="shrink-0">
            {choice.mode === "relay" ? "C# Relay" : "Server WASM"}
          </Badge>
        )}
      </span>
      <span className="text-xs text-muted-foreground">{choice.badge}</span>
      {isWasmMode(choice.mode) && (
        <span className="text-[11px] text-amber-600 dark:text-amber-500">{WASM_PREREQ_NOTE}</span>
      )}
    </button>
  )
}

const MODE_SUMMARY: Record<BackendMode, string> = {
  relay: "Zero Server Code — Pure C# Relay & BaaS. No WASM module is used for this game.",
  wasm: "Server-Side WASM Simulation. Upload a room module in the WASM Studio before players join.",
}

export function CreateGameDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void
  onCreated: (game: CreatedGame) => void
}) {
  const [step, setStep] = useState<1 | 2>(1)
  const [engine, setEngine] = useState<string>("unity")
  const [name, setName] = useState("")
  const [tickRate, setTickRate] = useState("30")
  const [maxClients, setMaxClients] = useState("8")
  const [persistentRooms, setPersistentRooms] = useState(false)
  const [autoClose, setAutoClose] = useState("60")

  const create = useConsoleMutation<CreatedGame>()

  const mode: BackendMode = ENGINE_CHOICES.find((choice) => choice.value === engine)?.mode ?? "relay"
  const tickOptions = TICK_RATE_OPTIONS.map((hz) => ({ value: String(hz), label: `${hz} Hz` }))
  const capacityOptions = CAPACITY_OPTIONS.map((n) => ({ value: String(n), label: `${n} players` }))

  function register() {
    if (name.trim() === "") return
    // Flat fields win server-side over a nested config; send exactly that.
    create.mutate(
      {
        method: "POST",
        path: GAMES_ENDPOINT,
        body: {
          name: name.trim(),
          engine_type: engine,
          tick_rate: Number(tickRate) || 0,
          max_clients: Number(maxClients) || 0,
          persistent_rooms: persistentRooms,
          auto_close_seconds: Number(autoClose) || 0,
        },
        success: "Game registered.",
      },
      { onSuccess: (game) => onCreated(game) },
    )
  }

  return (
    <FormSheet
      open
      title={step === 1 ? "Register game — engine" : "Register game — configuration"}
      description={step === 1 ? "Step 1 of 2 · the engine decides the server mode" : `Step 2 of 2 · ${name.trim() === "" ? "untitled game" : name.trim()}`}
      onClose={onClose}
      footer={
        step === 1 ? (
          <>
            <Button variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button onClick={() => setStep(2)}>
              Next
              <ArrowRight />
            </Button>
          </>
        ) : (
          <>
            <Button variant="outline" onClick={() => setStep(1)} disabled={create.isPending}>
              <ArrowLeft />
              Back
            </Button>
            <Button onClick={register} disabled={create.isPending || name.trim() === ""}>
              {create.isPending && <Loader2 className="animate-spin" />}
              Register
            </Button>
          </>
        )
      }
    >
      {step === 1 ? (
        <div role="radiogroup" aria-label="Engine" className="space-y-2">
          {ENGINE_CHOICES.map((choice) => (
            <EngineCard
              key={choice.value}
              choice={choice}
              selected={choice.value === engine}
              onSelect={() => setEngine(choice.value)}
            />
          ))}
        </div>
      ) : (
        <div className="space-y-3">
          <p className="rounded-md border border-border/60 bg-background/60 p-2 text-xs text-muted-foreground">
            {MODE_SUMMARY[mode]}
          </p>
          <TextInput label="Name" value={name} onChange={setName} placeholder="Arena Shooter" autoFocus />
          <div className="grid grid-cols-2 gap-3">
            <Select label="Sync rate" value={tickRate} onChange={setTickRate} options={tickOptions} />
            <Select label="Max players" value={maxClients} onChange={setMaxClients} options={capacityOptions} />
          </div>
          <div className="space-y-1 rounded-md border border-border/60 p-2.5">
            <Checkbox
              label="Persistent rooms"
              checked={persistentRooms}
              onChange={setPersistentRooms}
            />
            <p className="text-[11px] text-muted-foreground">
              Keep a room alive between matches so player-built bases and world state survive.
              For base-building and survival games.
            </p>
          </div>
          {persistentRooms && (
            <TextInput
              label="Auto-close after (seconds)"
              hint="How long an empty persistent room stays open before the server closes it."
              type="number"
              value={autoClose}
              onChange={setAutoClose}
            />
          )}
        </div>
      )}
    </FormSheet>
  )
}
