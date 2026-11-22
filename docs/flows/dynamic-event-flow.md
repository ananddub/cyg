# Dynamic Event Flow — DB Parameters → Round End Trigger → Player Store

## Goal

Operator dashboard se ek **dynamic event** banaye (code nahi likhe), uske
**parameters DB mein store** ho, **game/round end pe wo event trigger** ho,
aur uska **result DB + player store** dono mein update ho jaye.

## Flow (end to end)

```
Dashboard (VisualRuleBuilder)
        │  operator sets parameters
        ▼
┌──────────────────────────┐
│  event_definition (DB)   │   trigger = match_end / round_end
│  params + rule document  │   conditions = field/op/value tree
│  actions   = stat_write, │   actions    = currency, item, badge, notify
│              currency…   │
└────────────┬─────────────┘
             │ saved via engine-api
             ▼
   Round / Match ends (engine)
             │  emits round_end event
             ▼
┌──────────────────────────┐
│  Rule evaluator          │   loads event_definition for this game
│  (round end hook)        │   evaluates conditions against final state
└────────────┬─────────────┘
             │ per player, when rule matches
             ▼
┌──────────────────────────┐
│  stat_write receipt      │   idempotent write (write_id = room:tick)
│  + player_stat UPSERT    │   delta applied exactly once
└────────────┬─────────────┘
             │
             ▼
┌──────────────────────────┐
│  Player store sync       │   player profile / inventory / currency
│  (profile & wallet)      │   updated from the same receipt
└──────────────────────────┘
```

## Steps

### 1. Operator sets parameters (dashboard)

- VisualRuleBuilder mein **trigger node** ka event `round_end` / `match_end` select hota hai.
- **Condition nodes** se qualifying rules (e.g. `kills >= 5`, `score > 100`).
- **Action nodes** se result decide hota hai: stat write (delta), currency reward, item grant, badge.
- Graph → `RuleDocument` (trigger + actions) + `RuleExpression` (condition tree) mein convert hota hai (`graph-to-rule.ts`), aur engine-api pe save hota hai.

### 2. Parameters DB mein store

- `event_definition` row mein store hota hai: game id, trigger event, condition JSON, action list, enabled flag.
- Ye parameter set hi source of truth hai — engine restart ke baad bhi event wahi se evaluate hota hai.

### 3. Round end pe trigger

- Round/match complete hone pe engine `round_end` event emit karta hai (scoring hook — "a round scores itself when it ends").
- Evaluator us game ke saare enabled `event_definition` rows load karta hai, trigger match karta hai.

### 4. Result store (idempotent)

- Matching players ke liye `StatWriteReceipt` row banti hai (`stat_write` table, `write_id = room:tick:rule`).
- `apply_stat_write` transaction mein:
  - receipt already present → skip (no double count),
  - warna `player_stat` UPSERT — `value = value + delta`.
- Isi transaction mein event ka reward action (currency/item/badge) bhi apply hota hai.

### 5. Player store update

- Receipt/apply ka result se player profile store sync hota hai: currency balance, inventory (item quantity), badge list.
- Client SDK ka player store next fetch/push pe updated values dekhta hai — dashboard aur in-game UI consistent rehte hain.

## Existing building blocks

| Piece | Status | Location |
|---|---|---|
| Visual rule builder (drag-drop nodes) | ✅ hai | `dashboard/src/components/console/rules/` |
| Graph → rule document conversion | ✅ hai | `dashboard/.../rules/graph-to-rule.ts` |
| Idempotent stat write + player_stat UPSERT | ✅ hai (naya) | `crates/engine-api/src/repositories/stat_write.rs` |
| Round end self-scoring hook | ✅ hai | engine rooms / quest events |
| `round_end` trigger type in builder | ❌ add karna hai | trigger node options |
| `write stat` action type in builder | ❌ add karna hai | action node options |
| event_definition save/load API | ❌ add karna hai | engine-api services |
| Player store sync from receipt | ⚠️ partial | quest/profile services |

## Gaps (implementation order)

1. Builder mein trigger: `round_end` / `match_end` option.
2. Builder mein action: `write_stat` (stat name + delta).
3. engine-api: `event_definition` table + CRUD (save from dashboard, list per game).
4. Round-end hook: definitions load → evaluate → `apply_stat_write` + reward actions.
5. Player store sync: applied receipt se profile/currency/inventory update.
