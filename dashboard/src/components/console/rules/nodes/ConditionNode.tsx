import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Filter, Trash2 } from 'lucide-react';
import type { ConditionNodeData, RuleOperator } from '../types';

export function ConditionNode({ data }: NodeProps & { data: ConditionNodeData }) {
  const operators: { value: RuleOperator; label: string }[] = [
    { value: 'gte', label: '>= (Greater or Eq)' },
    { value: 'gt', label: '> (Greater than)' },
    { value: 'eq', label: '== (Equals)' },
    { value: 'ne', label: '!= (Not equal)' },
    { value: 'lte', label: '<= (Less or Eq)' },
    { value: 'lt', label: '< (Less than)' },
    { value: 'contains', label: 'contains' },
    { value: 'in', label: 'in (array match)' },
  ];

  const handleValueChange = (raw: string) => {
    // If it parses cleanly as a number, keep as number, else string
    if (raw !== '' && !Number.isNaN(Number(raw))) {
      data.onDataChange?.({ value: Number(raw) });
    } else {
      data.onDataChange?.({ value: raw });
    }
  };

  return (
    <div className="w-72 rounded-xl border border-sky-500/40 bg-zinc-900/95 p-3.5 shadow-xl shadow-sky-500/10 backdrop-blur-md">
      <Handle
        type="target"
        position={Position.Left}
        id="in"
        className="!h-3 !w-3 !rounded-full !border-2 !border-zinc-900 !bg-sky-400"
      />

      <div className="flex items-center justify-between border-b border-border/40 pb-2">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-sky-500/20 text-sky-400">
            <Filter className="h-4 w-4" />
          </div>
          <div>
            <div className="text-xs font-semibold uppercase tracking-wider text-sky-400">Condition Check</div>
            <div className="text-[11px] text-muted-foreground">Stat / State comparison</div>
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
          <label className="text-[11px] font-medium text-zinc-300">Field Name</label>
          <input
            type="text"
            className="mt-1 w-full rounded-md border border-border/60 bg-zinc-950 px-2 py-1 text-xs text-foreground placeholder-zinc-600 focus:border-sky-500 focus:outline-none"
            placeholder="e.g. kills, rank, level"
            value={data.field}
            onChange={(e) => data.onDataChange?.({ field: e.target.value })}
          />
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div>
            <label className="text-[11px] font-medium text-zinc-300">Operator</label>
            <select
              className="mt-1 w-full rounded-md border border-border/60 bg-zinc-950 px-2 py-1 text-xs text-foreground focus:border-sky-500 focus:outline-none"
              value={data.op}
              onChange={(e) => data.onDataChange?.({ op: e.target.value as RuleOperator })}
            >
              {operators.map((op) => (
                <option key={op.value} value={op.value}>
                  {op.label}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="text-[11px] font-medium text-zinc-300">Value</label>
            <input
              type="text"
              className="mt-1 w-full rounded-md border border-border/60 bg-zinc-950 px-2 py-1 text-xs text-foreground placeholder-zinc-600 focus:border-sky-500 focus:outline-none"
              placeholder="e.g. 100 or 'gold'"
              value={data.value ?? ''}
              onChange={(e) => handleValueChange(e.target.value)}
            />
          </div>
        </div>
      </div>

      <Handle
        type="source"
        position={Position.Right}
        id="out"
        className="!h-3 !w-3 !rounded-full !border-2 !border-zinc-900 !bg-sky-400"
      />
    </div>
  );
}
