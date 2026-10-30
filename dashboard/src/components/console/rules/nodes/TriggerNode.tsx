import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Zap } from 'lucide-react';
import type { TriggerNodeData } from '../types';

export function TriggerNode({ data }: NodeProps & { data: TriggerNodeData }) {
  const events = [
    { id: 'player.stat_update', label: 'Stat Update (Kills, Score, etc.)' },
    { id: 'match.ended', label: 'Match Ended' },
    { id: 'player.level_up', label: 'Player Level Up' },
    { id: 'player.login', label: 'Daily Login' },
    { id: 'iap.purchase_completed', label: 'Purchase Completed' },
  ];

  return (
    <div className="w-64 rounded-xl border border-violet-500/40 bg-zinc-900/95 p-3.5 shadow-xl shadow-violet-500/10 backdrop-blur-md">
      <div className="flex items-center gap-2 border-b border-border/40 pb-2">
        <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-violet-500/20 text-violet-400">
          <Zap className="h-4 w-4" />
        </div>
        <div>
          <div className="text-xs font-semibold uppercase tracking-wider text-violet-400">Trigger Event</div>
          <div className="text-[11px] text-muted-foreground">Source of game event</div>
        </div>
      </div>

      <div className="mt-3">
        <label className="text-[11px] font-medium text-zinc-300">Event</label>
        <select
          className="mt-1 w-full rounded-md border border-border/60 bg-zinc-950 px-2 py-1.5 text-xs text-foreground focus:border-violet-500 focus:outline-none"
          value={data.event}
          onChange={(e) => data.onDataChange?.({ event: e.target.value })}
        >
          {events.map((ev) => (
            <option key={ev.id} value={ev.id}>
              {ev.label}
            </option>
          ))}
        </select>
      </div>

      <Handle
        type="source"
        position={Position.Right}
        id="out"
        className="!h-3 !w-3 !rounded-full !border-2 !border-zinc-900 !bg-violet-400"
      />
    </div>
  );
}
