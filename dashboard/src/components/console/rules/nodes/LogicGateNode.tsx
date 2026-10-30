import { Handle, Position, type NodeProps } from '@xyflow/react';
import { GitBranch, Trash2 } from 'lucide-react';
import type { LogicGateNodeData } from '../types';

export function LogicGateNode({ data }: NodeProps & { data: LogicGateNodeData }) {
  return (
    <div className="w-56 rounded-xl border border-amber-500/40 bg-zinc-900/95 p-3.5 shadow-xl shadow-amber-500/10 backdrop-blur-md">
      <Handle
        type="target"
        position={Position.Left}
        id="in"
        className="!h-3 !w-3 !rounded-full !border-2 !border-zinc-900 !bg-amber-400"
      />

      <div className="flex items-center justify-between border-b border-border/40 pb-2">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-amber-500/20 text-amber-400">
            <GitBranch className="h-4 w-4" />
          </div>
          <div>
            <div className="text-xs font-semibold uppercase tracking-wider text-amber-400">Logic Gate</div>
            <div className="text-[11px] text-muted-foreground">Combinator branch</div>
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

      <div className="mt-3">
        <label className="text-[11px] font-medium text-zinc-300">Gate Type</label>
        <div className="mt-1 flex rounded-lg border border-border/60 bg-zinc-950 p-1">
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-semibold transition ${
              data.gate === 'all'
                ? 'bg-amber-500 text-black shadow-sm'
                : 'text-zinc-400 hover:text-foreground'
            }`}
            onClick={() => data.onDataChange?.({ gate: 'all' })}
          >
            AND (all)
          </button>
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-semibold transition ${
              data.gate === 'any'
                ? 'bg-amber-500 text-black shadow-sm'
                : 'text-zinc-400 hover:text-foreground'
            }`}
            onClick={() => data.onDataChange?.({ gate: 'any' })}
          >
            OR (any)
          </button>
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-semibold transition ${
              data.gate === 'not'
                ? 'bg-amber-500 text-black shadow-sm'
                : 'text-zinc-400 hover:text-foreground'
            }`}
            onClick={() => data.onDataChange?.({ gate: 'not' })}
          >
            NOT
          </button>
        </div>
      </div>

      <Handle
        type="source"
        position={Position.Right}
        id="out"
        className="!h-3 !w-3 !rounded-full !border-2 !border-zinc-900 !bg-amber-400"
      />
    </div>
  );
}
