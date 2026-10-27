// Simple single-step "Register Game" sheet.
//
// Directly collects the game name and runtime configuration (sync rate,
// max players, persistent rooms & auto-close window) without forcing the
// developer through engine selection cards.

import { useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Checkbox, Select, TextInput } from "@/components/console/form"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"

import { GAMES_ENDPOINT } from "./routes"
import { CAPACITY_OPTIONS, TICK_RATE_OPTIONS } from "./options"
import type { CreatedGame } from "./model"

export function CreateGameDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void
  onCreated: (game: CreatedGame) => void
}) {
  const [name, setName] = useState("")
  const [tickRate, setTickRate] = useState("30")
  const [maxClients, setMaxClients] = useState("8")
  const [persistentRooms, setPersistentRooms] = useState(false)
  const [autoClose, setAutoClose] = useState("60")

  const create = useConsoleMutation<CreatedGame>()

  const tickOptions = TICK_RATE_OPTIONS.map((hz) => ({ value: String(hz), label: `${hz} Hz` }))
  const capacityOptions = CAPACITY_OPTIONS.map((n) => ({ value: String(n), label: `${n} players` }))

  function register() {
    if (name.trim() === "") return
    create.mutate(
      {
        method: "POST",
        path: GAMES_ENDPOINT,
        body: {
          name: name.trim(),
          engine_type: "custom",
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
      title="Register game"
      description={name.trim() === "" ? "Configure your multiplayer game settings" : name.trim()}
      onClose={onClose}
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={register} disabled={create.isPending || name.trim() === ""}>
            {create.isPending && <Loader2 className="animate-spin" />}
            Register game
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <TextInput
          label="Name"
          value={name}
          onChange={setName}
          placeholder="Arena Shooter"
          autoFocus
        />
        <div className="grid grid-cols-2 gap-3">
          <Select
            label="Sync rate"
            value={tickRate}
            onChange={setTickRate}
            options={tickOptions}
          />
          <Select
            label="Max players"
            value={maxClients}
            onChange={setMaxClients}
            options={capacityOptions}
          />
        </div>
        <div className="space-y-1 rounded-md border border-border/60 p-2.5">
          <Checkbox
            label="Persistent rooms"
            checked={persistentRooms}
            onChange={setPersistentRooms}
          />
          <p className="text-[11px] text-muted-foreground">
            Keep a room alive between matches so player-built bases and world state survive.
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
    </FormSheet>
  )
}
