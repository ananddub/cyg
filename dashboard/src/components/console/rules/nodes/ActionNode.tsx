import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Gift, Trash2 } from 'lucide-react';
import type { ActionNodeData } from '../types';

export function ActionNode({ data }: NodeProps & { data: ActionNodeData }) {
  const actionTypes = [
    { value: 'coins', label: 'Grant Coins / Currency' },
    { value: 'badge', label: 'Unlock Badge / Achievement' },
    { value: 'skin', label: 'Grant Skin / Item' },
    { value: 'event', label: 'Trigger Webhook Event' },
  ];

  return (
    <div className="w-68 rounded-xl border border-emerald-500/40 bg-zinc-900/95 p-3.5 shadow-xl shadow-emerald-500/10 backdrop-blur-md">
      <Handle
        type="target"
        position={Position.Left}
        id="in"
        className="!h-3 !w-3 !rounded-full !border-2 !border-zinc-900 !bg-emerald-400"
      />

      <div className="flex items-center justify-between border-b border-border/40 pb-2">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-emerald-500/20 text-emerald-400">
            <Gift className="h-4 w-4" />
          </div>
          <div>
            <div className="text-xs font-semibold uppercase tracking-wider text-emerald-400">Reward / Action</div>
            <div className="text-[11px] text-muted-foreground">Execution outcome</div>
          </div>
        </div>
        {data.onDelete && (
          <button
            type="button"
            onClick={data.onDelete}
            className="rounded p-1 text-zinc-500 hover:bg-zinc-800 hover:text-red-400"
            title="Delete node"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        )}
      </div>

      <div className="mt-3 space-y-2">
        <div>
          <label className="text-[11px] font-medium text-zinc-300">Action Type</label>
          <select
            className="mt-1 w-full rounded-md border border-border/60 bg-zinc-950 px-2 py-1 text-xs text-foreground focus:border-emerald-500 focus:outline-none"
            value={data.actionType}
            onChange={(e) => data.onDataChange?.({ actionType: e.target.value as ActionNodeData['actionType'] })}
          >
            {actionTypes.map((act) => (
              <option key={act.value} value={act.value}>
                {act.label}
              </option>
            ))}
          </select>
        </div>

        {data.actionType === 'coins' ? (
          <div>
            <label className="text-[11px] font-medium text-zinc-300">Coin Amount</label>
            <input
              type="number"
              className="mt-1 w-full rounded-md border border-border/60 bg-zinc-950 px-2 py-1 text-xs text-foreground placeholder-zinc-600 focus:border-emerald-500 focus:outline-none"
              placeholder="e.g. 500"
              value={data.amount ?? 100}
              onChange={(e) => data.onDataChange?.({ amount: Number(e.target.value) })}
            />
          </div>
        ) : (
          <div>
            <label className="text-[11px] font-medium text-zinc-300">
              {data.actionType === 'badge' ? 'Badge Identifier' : data.actionType === 'skin' ? 'Item / Skin ID' : 'Event Name'}
            </label>
            <input
              type="text"
              className="mt-1 w-full rounded-md border border-border/60 bg-zinc-950 px-2 py-1 text-xs text-foreground placeholder-zinc-600 focus:border-emerald-500 focus:outline-none"
              placeholder={
                data.actionType === 'badge'
                  ? 'e.g. Centurion_Badge'
                  : data.actionType === 'skin'
                  ? 'e.g. dragon_sniper'
                  : 'e.g. quest.completed'
              }
              value={data.payload}
              onChange={(e) => data.onDataChange?.({ payload: e.target.value })}
            />
          </div>
        )}
      </div>
    </div>
  );
}
