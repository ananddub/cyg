import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Scale, Trash2 } from 'lucide-react';
import type { CompareNodeData } from '../types';
import { RuleSelect } from '../RuleSelect';

// UE ka relational operator node. Fixed rows:
// header 44 | A row 36 (top 52) | op row 36 (top 96) | B row 36 (top 140)
// Handle tops: a=70, bool=114, b=158
export function CompareNode({ data }: NodeProps & { data: CompareNodeData }) {
  const operators = [
    { value: 'gte', label: 'A >= B' },
    { value: 'gt', label: 'A > B' },
    { value: 'eq', label: 'A == B' },
    { value: 'ne', label: 'A != B' },
    { value: 'lte', label: 'A <= B' },
    { value: 'lt', label: 'A < B' },
    { value: 'contains', label: 'A contains B' },
  ];
  const row = 'h-8 relative flex items-center rounded-lg px-2.5 gap-2';

  return (
    <div className="w-72 rounded-xl border border-border border-l-4 border-l-fuchsia-500 bg-card/95 shadow-sm">
      <div className="flex h-11 items-center justify-between border-b border-border px-3">
        <div className="flex items-center gap-2.5">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-fuchsia-500/20 text-fuchsia-600 dark:text-fuchsia-300">
            <Scale className="h-4 w-4" />
          </div>
          <div className="text-xs font-bold uppercase tracking-wider text-foreground">Compare</div>
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

      <div className="space-y-1 px-3 py-2 nodrag nopan">
        {/* A row: param wire se avar, ya stat field text */}
        <div className={`${row} border border-border bg-muted/50`}>
          {data.avar ? (
            <span className="font-mono text-[11px] font-bold" style={{ color: data.acolor ?? '#c8c8c8' }}>
              {data.avar}
            </span>
          ) : (
            <input
              type="text"
              className="w-full bg-transparent text-[11px] font-medium text-foreground outline-none placeholder:text-muted-foreground"
              placeholder="A — stat field (kills…)"
              value={data.a}
              onChange={(e) => data.onDataChange?.({ a: e.target.value })}
            />
          )}
        </div>
        {/* Op row — select full width, apna border/padding nahi */}
        <div className="relative flex items-center rounded-lg border border-border bg-background">
          <RuleSelect
            options={operators}
            value={data.op}
            className="!border-0 !bg-transparent !shadow-none !rounded-none !h-8"
            onValueChange={(val) => data.onDataChange?.({ op: val as CompareNodeData['op'] })}
          />
        </div>
        {/* B row: param wire se bvar, ya literal */}
        <div className={`${row} border border-border bg-muted/50`}>
          {data.bvar ? (
            <span className="font-mono text-[11px] font-bold" style={{ color: data.bcolor ?? '#c8c8c8' }}>
              {data.bvar}
            </span>
          ) : (
            <input
              type="text"
              className="w-full bg-transparent text-[11px] font-medium text-foreground outline-none placeholder:text-muted-foreground"
              placeholder="B — value (10, gold…)"
              value={data.bval ?? ''}
              onChange={(e) => {
                const raw = e.target.value;
                data.onDataChange?.({
                  bval: raw !== '' && !Number.isNaN(Number(raw)) ? Number(raw) : raw,
                });
              }}
            />
          )}
        </div>
      </div>

      <Handle
        type="target"
        position={Position.Left}
        id="a"
        className="!h-3 !w-3 !rounded-full !border-2 !border-card"
        style={{ top: 68, background: data.acolor ?? '#94a3b8' }}
      />
      <Handle
        type="source"
        position={Position.Right}
        id="bool"
        className="!h-3 !w-3 !rounded-full !border-2 !border-card !bg-slate-400"
        style={{ top: 104 }}
      />
      <Handle
        type="target"
        position={Position.Left}
        id="b"
        className="!h-3 !w-3 !rounded-full !border-2 !border-card"
        style={{ top: 140, background: data.bcolor ?? '#94a3b8' }}
      />
    </div>
  );
}
