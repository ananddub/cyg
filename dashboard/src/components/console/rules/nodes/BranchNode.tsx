import { Handle, Position, type NodeProps } from '@xyflow/react';
import { GitBranch, Trash2 } from 'lucide-react';

// UE ka Branch node. Card ki height FIXED rows pe based hai, isliye har
// handle ka top offset exactly calculate hota hai — pin hamesha apni row
// ke saath align rahega, chahe zoom kuch bhi ho.
//
// Layout: header 44px | body pt-2: cond row 36 | true row 36 | false row 36
// Handle tops: in=22, condition=44+8+18=70, true=44+8+36+8+18=114, false=162
export function BranchNode({ data }: NodeProps & { data: { onDelete?: () => void } }) {
  const row = 'h-9 relative flex items-center rounded-lg px-3';
  return (
    <div className="w-72 rounded-xl border border-border border-l-4 border-l-indigo-500 bg-card/95 shadow-sm">
      <div className="flex h-11 items-center justify-between border-b border-border px-3">
        <div className="flex items-center gap-2.5">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-indigo-500/20 text-indigo-600 dark:text-indigo-300">
            <GitBranch className="h-4 w-4" />
          </div>
          <div className="text-xs font-bold uppercase tracking-wider text-foreground">Branch</div>
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

      <div className="space-y-2 px-3 py-2">
        <div className={`${row} bg-muted/50`}>
          <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
            Condition
          </span>
        </div>
        <div className={`${row} border border-emerald-500/30 bg-emerald-500/10`}>
          <span className="ml-auto text-[10px] font-bold uppercase tracking-wider text-emerald-600 dark:text-emerald-300">
            True
          </span>
        </div>
        <div className={`${row} border border-border bg-muted/40`}>
          <span className="ml-auto text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
            False
          </span>
        </div>
      </div>

      {/* Exec in — header ke bagal */}
      <Handle
        type="target"
        position={Position.Left}
        id="in"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-card !bg-slate-400"
        style={{ top: 22 }}
      />
      {/* Condition bool in — cond row ka left */}
      <Handle
        type="target"
        position={Position.Left}
        id="condition"
        className="!h-3 !w-3 !rounded-full !border-2 !border-card !bg-slate-300"
        style={{ top: 70 }}
      />
      {/* True / False exec out */}
      <Handle
        type="source"
        position={Position.Right}
        id="true"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-card !bg-emerald-500"
        style={{ top: 114 }}
      />
      <Handle
        type="source"
        position={Position.Right}
        id="false"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-card !bg-slate-400"
        style={{ top: 162 }}
      />
    </div>
  );
}
