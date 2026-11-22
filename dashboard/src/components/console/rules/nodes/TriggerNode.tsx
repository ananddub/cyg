import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Plus, Trash2, Zap } from 'lucide-react';
import type { CardParam, TriggerNodeData } from '../types';
import { RuleSelect } from '../RuleSelect';

// UE Blueprint ke pin colors: type hi color batata hai (book: "The color of
// the pins represents the type of data variable they pass").
const PARAM_TYPES = [
  { value: 'number', label: 'number', color: '#2ec1a0' },
  { value: 'string', label: 'string', color: '#f39bf9' },
  { value: 'bool', label: 'bool', color: '#e04a4a' },
];

export const PARAM_TYPE_COLORS: Record<string, string> = {
  number: '#2ec1a0',
  string: '#f39bf9',
  bool: '#e04a4a',
};

export function TriggerNode({ data }: NodeProps & { data: TriggerNodeData }) {
  // The engine fires these itself. Anything else the game sends through
  // record_events reaches the pump too, so a lowercase snake_case event name
  // is a valid trigger — the custom field below takes it.
  const events = [
    { value: 'purchase', label: 'Purchase completed' },
    { value: 'wallet_change', label: 'Wallet changed' },
    { value: 'level_up', label: 'Player level up' },
    { value: 'match_end', label: 'Match ended' },
    { value: 'leaderboard_submit', label: 'Leaderboard score submitted' },
    { value: 'player_create', label: 'Player created' },
  ];
  const isCustom = !events.some((e) => e.value === data.event);
  const params = data.params ?? [];

  const setParams = (next: CardParam[]) => data.onDataChange?.({ params: next });
  const addParam = () =>
    setParams([
      ...params,
      {
        uid: Math.random().toString(36).slice(2, 9),
        name: `param_${params.length + 1}`,
        type: 'number',
        default: '0',
      },
    ]);
  const updateParam = (index: number, patch: Partial<CardParam>) =>
    setParams(params.map((p, i) => (i === index ? { ...p, ...patch } : p)));
  const removeParam = (index: number) => setParams(params.filter((_, i) => i !== index));

  return (
    <div className="w-72 rounded-xl border border-border border-l-4 border-l-violet-500 bg-card/95 p-3.5 shadow-sm">
      <div className="flex items-center gap-2.5 border-b border-border pb-2.5">
        <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-violet-500/20 text-violet-600 dark:text-violet-300">
          <Zap className="h-4 w-4" />
        </div>
        <div>
          <div className="text-xs font-bold uppercase tracking-wider text-foreground">Trigger</div>
        </div>
      </div>

      <div className="mt-3 nodrag nopan">
        <div className="mt-1">
          <RuleSelect
            options={[...events, { value: '__custom__', label: 'Custom game event…' }]}
            value={isCustom ? '__custom__' : data.event}
            onValueChange={(val) => {
              if (val === '__custom__') {
                data.onDataChange?.({ event: '' });
              } else {
                data.onDataChange?.({ event: val });
              }
            }}
          />
          {isCustom && (
            <input
              type="text"
              className="mt-2 w-full rounded-lg border border-input bg-background px-2.5 py-1.5 text-xs font-medium text-foreground shadow-xs placeholder-muted-foreground transition focus:border-ring focus:ring-1 focus:ring-ring focus:outline-none"
              placeholder="event name — e.g. boss_died, combo_3x"
              value={data.event}
              onChange={(e) => data.onDataChange?.({ event: e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, '_') })}
            />
          )}
        </div>
      </div>

      {/* Params: name / type / value — sab card pe hi, sidebar ki zaroorat nahi.
          Har param ka apna named data pin right side pe banta hai. */}
      <div className="mt-3 border-t border-border pt-2.5 nodrag nopan">
        <div className="flex items-center justify-between">
          <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Params</span>
          <button
            type="button"
            onClick={addParam}
            className="flex items-center gap-1 rounded-md border border-border bg-muted/40 px-2 py-0.5 text-[10px] font-medium text-foreground transition hover:bg-muted"
            title="Add param"
          >
            <Plus className="h-3 w-3" /> Add
          </button>
        </div>
        {params.length === 0 && (
          <p className="mt-1.5 text-[10px] text-muted-foreground">
            [+] se param jodo — name, type, value sab yahin set hoga
          </p>
        )}
        <div className="mt-1.5 space-y-1.5">
          {params.map((param, index) => (
            <div key={index} className="relative flex items-center gap-1">
              <input
                type="text"
                className="h-7 w-24 min-w-0 rounded-md border border-input bg-background px-1.5 text-[10px] font-mono text-foreground"
                value={param.name}
                onChange={(e) =>
                  updateParam(index, {
                    name: e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, '_'),
                  })
                }
                placeholder="var_name"
              />
              <div className="w-24">
                <RuleSelect
                  options={PARAM_TYPES}
                  value={param.type}
                  onValueChange={(val) => updateParam(index, { type: val as CardParam['type'] })}
                />
              </div>
              {param.type === 'bool' ? (
                <label className="flex h-7 items-center gap-1.5 cursor-pointer">
                  <input
                    type="checkbox"
                    className="h-3.5 w-3.5 cursor-pointer accent-red-500"
                    checked={param.default === 'true'}
                    onChange={(e) => updateParam(index, { default: e.target.checked ? 'true' : 'false' })}
                  />
                  <span className="text-[10px] font-medium text-foreground">
                    {param.default === 'true' ? 'true' : 'false'}
                  </span>
                </label>
              ) : (
                <input
                  type="text"
                  className="h-7 w-14 min-w-0 rounded-md border border-input bg-background px-1.5 text-[10px] text-foreground"
                  value={param.default}
                  onChange={(e) => updateParam(index, { default: e.target.value })}
                  placeholder={param.type === 'number' ? '0' : 'val'}
                />
              )}
              <button
                type="button"
                onClick={() => removeParam(index)}
                className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-destructive"
                title="Remove param"
              >
                <Trash2 className="h-3 w-3" />
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Har param ka named data pin card ke right side pe, bottom se stacked —
          rows ke saath align (params section card ke bottom me hota hai). */}
      {params.map((param, index) => (
        <Handle
          key={param.name || index}
          type="source"
          position={Position.Right}
          id={`param-${param.uid}`}
          className="!h-3 !w-3 !rounded-full !border-2 !border-card"
          style={{
            // bottom se stack: params card ke bottom me hain, to row ke
            // saath exact align hota hai — card ki height jo bhi ho.
            top: 'auto',
            bottom: `${19 + (params.length - 1 - index) * 34}px`,
            background: PARAM_TYPE_COLORS[param.type] ?? '#c8c8c8',
          }}
        />
      ))}
      <Handle
        type="source"
        position={Position.Right}
        id="out"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-card !bg-slate-400"
      />
    </div>
  );
}
