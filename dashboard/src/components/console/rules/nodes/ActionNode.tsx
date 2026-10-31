import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Gift, Trash2 } from 'lucide-react';
import type { ActionNodeData } from '../types';
import { RuleSelect } from '../RuleSelect';

export function ActionNode({ data }: NodeProps & { data: ActionNodeData }) {
  const actionTypes = [
    { value: 'coins', label: 'Grant Coins / Currency' },
    { value: 'badge', label: 'Unlock Badge / Achievement' },
    { value: 'skin', label: 'Grant Skin / Item' },
    { value: 'event', label: 'Trigger Webhook Event' },
  ];

  return (
    <div className="w-76 rounded-xl border border-emerald-500/50 bg-zinc-950/95 p-3.5 shadow-2xl shadow-emerald-500/15 backdrop-blur-md">
      <Handle
        type="target"
        position={Position.Left}
        id="in"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-zinc-950 !bg-emerald-400"
      />

      <div className="flex items-center justify-between border-b border-border/40 pb-2.5">
        <div className="flex items-center gap-2.5">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-emerald-500/25 text-emerald-300">
            <Gift className="h-4 w-4" />
          </div>
          <div>
            <div className="text-xs font-bold uppercase tracking-wider text-emerald-300">Reward / Action</div>
            <div className="text-[11px] text-zinc-400">Execution outcome</div>
          </div>
        </div>
        {data.onDelete && (
          <button
            type="button"
            onClick={data.onDelete}
            className="rounded p-1 text-zinc-400 transition hover:bg-zinc-800 hover:text-red-400 nodrag cursor-pointer"
            title="Delete node"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        )}
      </div>

      <div className="mt-3 space-y-2.5 nodrag nopan">
        <div>
          <label className="text-[11px] font-semibold text-zinc-200">Action Type</label>
          <div className="mt-1">
            <RuleSelect
              options={actionTypes}
              value={data.actionType}
              accent="emerald"
              onValueChange={(val) =>
                data.onDataChange?.({
                  actionType: val as ActionNodeData['actionType'],
                })
              }
            />
          </div>
        </div>

        {data.actionType === 'coins' ? (
          <div>
            <label className="text-[11px] font-semibold text-zinc-200">Coin Amount</label>
            <input
              type="number"
              className="mt-1 h-8 w-full rounded-lg border border-zinc-700 bg-zinc-900 px-2.5 py-1 text-xs font-medium text-zinc-100 shadow-sm placeholder-zinc-500 transition focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/40 focus:outline-none nodrag"
              placeholder="e.g. 500"
              value={data.amount ?? 100}
              onChange={(e) => data.onDataChange?.({ amount: Number(e.target.value) })}
            />
          </div>
        ) : (
          <div>
            <label className="text-[11px] font-semibold text-zinc-200">
              {data.actionType === 'badge'
                ? 'Badge Identifier'
                : data.actionType === 'skin'
                ? 'Item / Skin ID'
                : 'Event Name'}
            </label>
            <input
              type="text"
              className="mt-1 h-8 w-full rounded-lg border border-zinc-700 bg-zinc-900 px-2.5 py-1 text-xs font-medium text-zinc-100 shadow-sm placeholder-zinc-500 transition focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/40 focus:outline-none nodrag"
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
