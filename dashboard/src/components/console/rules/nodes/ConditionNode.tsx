import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Filter, Trash2 } from 'lucide-react';
import type { ConditionNodeData, RuleOperator } from '../types';
import { RuleSelect } from '../RuleSelect';

export function ConditionNode({ data }: NodeProps & { data: ConditionNodeData }) {
  const operators = [
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
    if (raw !== '' && !Number.isNaN(Number(raw))) {
      data.onDataChange?.({ value: Number(raw) });
    } else {
      data.onDataChange?.({ value: raw });
    }
  };

  return (
    <div className="w-80 rounded-xl border border-sky-500/50 bg-zinc-950/95 p-3.5 shadow-2xl shadow-sky-500/15 backdrop-blur-md">
      <Handle
        type="target"
        position={Position.Left}
        id="in"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-zinc-950 !bg-sky-400"
      />

      <div className="flex items-center justify-between border-b border-border/40 pb-2.5">
        <div className="flex items-center gap-2.5">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-sky-500/25 text-sky-300">
            <Filter className="h-4 w-4" />
          </div>
          <div>
            <div className="text-xs font-bold uppercase tracking-wider text-sky-300">Condition Check</div>
            <div className="text-[11px] text-zinc-400">Stat / State comparison</div>
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
          <label className="text-[11px] font-semibold text-zinc-200">Field Name</label>
          <input
            type="text"
            className="mt-1 w-full rounded-lg border border-zinc-700 bg-zinc-900 px-2.5 py-1.5 text-xs font-medium text-zinc-100 shadow-sm placeholder-zinc-500 transition focus:border-sky-400 focus:ring-1 focus:ring-sky-400/40 focus:outline-none nodrag"
            placeholder="e.g. kills, rank, level"
            value={data.field}
            onChange={(e) => data.onDataChange?.({ field: e.target.value })}
          />
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div>
            <label className="text-[11px] font-semibold text-zinc-200">Operator</label>
            <div className="mt-1">
              <RuleSelect
                options={operators}
                value={data.op}
                accent="sky"
                onValueChange={(val) => data.onDataChange?.({ op: val as RuleOperator })}
              />
            </div>
          </div>

          <div>
            <label className="text-[11px] font-semibold text-zinc-200">Value</label>
            <input
              type="text"
              className="mt-1 h-8 w-full rounded-lg border border-zinc-700 bg-zinc-900 px-2.5 py-1 text-xs font-medium text-zinc-100 shadow-sm placeholder-zinc-500 transition focus:border-sky-400 focus:ring-1 focus:ring-sky-400/40 focus:outline-none nodrag"
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
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-zinc-950 !bg-sky-400"
      />
    </div>
  );
}
