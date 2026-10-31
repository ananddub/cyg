import { Handle, Position, type NodeProps } from '@xyflow/react';
import { GitBranch, Trash2 } from 'lucide-react';
import type { LogicGateNodeData } from '../types';

export function LogicGateNode({ data }: NodeProps & { data: LogicGateNodeData }) {
  return (
    <div className="w-60 rounded-xl border border-amber-500/50 bg-zinc-950/95 p-3.5 shadow-2xl shadow-amber-500/15 backdrop-blur-md">
      <Handle
        type="target"
        position={Position.Left}
        id="in"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-zinc-950 !bg-amber-400"
      />

      <div className="flex items-center justify-between border-b border-border/40 pb-2.5">
        <div className="flex items-center gap-2.5">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-amber-500/25 text-amber-300">
            <GitBranch className="h-4 w-4" />
          </div>
          <div>
            <div className="text-xs font-bold uppercase tracking-wider text-amber-300">Logic Gate</div>
            <div className="text-[11px] text-zinc-400">Combinator branch</div>
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

      <div className="mt-3 nodrag nopan">
        <label className="text-[11px] font-semibold text-zinc-200">Gate Type</label>
        <div className="mt-1 flex rounded-lg border border-zinc-700 bg-zinc-900 p-1 shadow-sm">
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-bold transition nodrag cursor-pointer ${
              data.gate === 'all'
                ? 'bg-amber-400 text-zinc-950 shadow'
                : 'text-zinc-400 hover:text-zinc-100'
            }`}
            onClick={() => data.onDataChange?.({ gate: 'all' })}
          >
            AND (all)
          </button>
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-bold transition nodrag cursor-pointer ${
              data.gate === 'any'
                ? 'bg-amber-400 text-zinc-950 shadow'
                : 'text-zinc-400 hover:text-zinc-100'
            }`}
            onClick={() => data.onDataChange?.({ gate: 'any' })}
          >
            OR (any)
          </button>
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-bold transition nodrag cursor-pointer ${
              data.gate === 'not'
                ? 'bg-amber-400 text-zinc-950 shadow'
                : 'text-zinc-400 hover:text-zinc-100'
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
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-zinc-950 !bg-amber-400"
      />
    </div>
  );
}
