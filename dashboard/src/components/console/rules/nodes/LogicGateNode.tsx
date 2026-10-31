import { Handle, Position, type NodeProps } from '@xyflow/react';
import { GitBranch, Trash2 } from 'lucide-react';
import type { LogicGateNodeData } from '../types';

export function LogicGateNode({ data }: NodeProps & { data: LogicGateNodeData }) {
  return (
    <div className="w-60 rounded-xl border border-amber-500/50 bg-card/95 p-3.5 shadow-xl shadow-amber-500/10 backdrop-blur-md">
      <Handle
        type="target"
        position={Position.Left}
        id="in"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-card !bg-amber-500"
      />

      <div className="flex items-center justify-between border-b border-border pb-2.5">
        <div className="flex items-center gap-2.5">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-amber-500/20 text-amber-600 dark:text-amber-300">
            <GitBranch className="h-4 w-4" />
          </div>
          <div>
            <div className="text-xs font-bold uppercase tracking-wider text-amber-600 dark:text-amber-300">Logic Gate</div>
          </div>
        </div>
        {data.onDelete && (
          <button
            type="button"
            onClick={data.onDelete}
            className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-destructive nodrag cursor-pointer"
            title="Delete node"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        )}
      </div>

      <div className="mt-2.5 nodrag nopan">
        <div className="flex rounded-lg border border-border bg-muted/60 p-1 shadow-xs">
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-bold transition nodrag cursor-pointer ${
              data.gate === 'all'
                ? 'bg-amber-500 text-white dark:bg-amber-400 dark:text-zinc-950 shadow-xs'
                : 'text-muted-foreground hover:text-foreground'
            }`}
            onClick={() => data.onDataChange?.({ gate: 'all' })}
          >
            AND (all)
          </button>
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-bold transition nodrag cursor-pointer ${
              data.gate === 'any'
                ? 'bg-amber-500 text-white dark:bg-amber-400 dark:text-zinc-950 shadow-xs'
                : 'text-muted-foreground hover:text-foreground'
            }`}
            onClick={() => data.onDataChange?.({ gate: 'any' })}
          >
            OR (any)
          </button>
          <button
            type="button"
            className={`flex-1 rounded py-1 text-xs font-bold transition nodrag cursor-pointer ${
              data.gate === 'not'
                ? 'bg-amber-500 text-white dark:bg-amber-400 dark:text-zinc-950 shadow-xs'
                : 'text-muted-foreground hover:text-foreground'
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
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-zinc-950 !bg-amber-400 dark:!border-card"
      />
    </div>
  );
}
