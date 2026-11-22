import { useState } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Pencil, Plus, Trash2, Zap } from 'lucide-react';
import type { CardParam, TriggerNodeData } from '../types';
import { RuleSelect } from '../RuleSelect';

// UE Blueprint ke pin colors: type hi color batata hai (book: "The color of
// the pins represents the type of data variable they pass").
export const PARAM_TYPE_COLORS: Record<string, string> = {
  number: '#2ec1a0',
  string: '#f39bf9',
  bool: '#e04a4a',
};

export function TriggerNode({ data }: NodeProps & { data: TriggerNodeData }) {
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

  // Local: kaunsa param row edit mode me khula hai (sirf ek waqt me).
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);

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
  // Dot hi type button: click = number → string → bool → number…
  const cycleType = (index: number) => {
    const order: CardParam['type'][] = ['number', 'string', 'bool'];
    const current = order.indexOf(params[index].type);
    updateParam(index, { type: order[(current + 1) % order.length] });
  };

  return (
    <div className="w-72 rounded-xl border border-border border-l-4 border-l-violet-500 bg-card/95 shadow-sm">
      <div className="flex items-center gap-2.5 border-b border-border pb-2.5 px-3 pt-3">
        <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-violet-500/20 text-violet-600 dark:text-violet-300">
          <Zap className="h-4 w-4" />
        </div>
        <div className="text-xs font-bold uppercase tracking-wider text-foreground">Trigger</div>
      </div>

      <div className="px-3 pt-3 nodrag nopan">
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

      {/* Params: closed me sirf dot + edit + delete. Edit khulne par
          naam/value dono edit hote hain. Dot click = type cycle. */}
      <div className="px-3 pb-3 pt-2.5">
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
            [+] se param jodo — naam, type, value edit icon se
          </p>
        )}
        <div className="mt-1.5 space-y-1.5">
          {params.map((param, index) => {
            const isOpen = expandedIdx === index;
            return (
              <div key={param.uid || index} className="flex items-center gap-1.5">
                {/* Dot = type color, click se type cycle */}
                <button
                  type="button"
                  onClick={(e) => {
                    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                    setDotMenu({
                      index,
                      left: r.left,
                      top: r.bottom + 6,
                    });
                  }}
                  className="h-4 w-4 shrink-0 rounded-full border border-card shadow-sm transition hover:scale-110 cursor-pointer"
                  style={{ background: PARAM_TYPE_COLORS[param.type] ?? '#c8c8c8' }}
                  title={`type: ${param.type} — click for options`}
                />
                {isOpen && (
                  <>
                    <input
                      type="text"
                      className="h-7 w-20 min-w-0 rounded-md border border-input bg-background px-1.5 text-[10px] font-mono text-foreground"
                      value={param.name}
                      onChange={(e) =>
                        updateParam(index, {
                          name: e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, '_'),
                        })
                      }
                      placeholder="var_name"
                    />
                    {param.type === 'bool' ? (
                      <label className="flex h-7 items-center gap-1.5 cursor-pointer">
                        <input
                          type="checkbox"
                          className="h-3.5 w-3.5 cursor-pointer accent-red-500"
                          checked={param.default === 'true'}
                          onChange={(e) =>
                            updateParam(index, { default: e.target.checked ? 'true' : 'false' })
                          }
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
                  </>
                )}
                {!isOpen && (
                  <span
                    className="max-w-[90px] truncate text-[11px] font-mono font-bold"
                    style={{ color: PARAM_TYPE_COLORS[param.type] ?? '#c8c8c8' }}
                  >
                    {param.name}
                  </span>
                )}
                <button
                  type="button"
                  onClick={() => {
                    setExpandedIdx(isOpen ? null : index);
                    setDotMenu(null);
                  }}
                  className={`ml-auto rounded p-1 transition nodrag cursor-pointer ${
                    isOpen
                      ? 'text-primary hover:bg-primary/10'
                      : 'text-muted-foreground hover:bg-muted hover:text-foreground'
                  }`}
                  title={isOpen ? 'Close' : 'Edit name & value'}
                >
                  <Pencil className="h-3 w-3" />
                </button>
                <button
                  type="button"
                  onClick={() => removeParam(index)}
                  className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-destructive"
                  title="Remove param"
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              </div>
            );
          })}
        </div>
      </div>

      {/* Har param ka named data pin (card ke right side, bottom stacked) */}
      {params.map((param, index) => (
        <Handle
          key={param.uid || index}
          type="source"
          position={Position.Right}
          id={`param-${param.uid}`}
          className="!h-3 !w-3 !rounded-full !border-2 !border-card"
          style={{
            bottom: `${10 + (params.length - 1 - index) * 30}px`,
            top: 'auto',
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

      {/* Dot ka dropdown: portal, niche khulta hai, colored naam+dot options */}
      {dotMenu && (
        <div
          className="fixed z-[400] min-w-[140px] rounded-lg border border-border bg-popover/98 p-1 shadow-2xl nodrag nopan"
          style={{ left: dotMenu.left, top: dotMenu.top }}
        >
          {PARAM_TYPES.map((t) => {
            const selected = params[dotMenu.index]?.type === t.value;
            return (
              <button
                key={t.value}
                type="button"
                onClick={() => {
                  updateParam(dotMenu.index, { type: t.value as CardParam['type'] });
                  setDotMenu(null);
                }}
                className={`flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs font-semibold transition cursor-pointer ${
                  selected ? 'bg-primary/15 text-primary font-bold' : 'text-foreground hover:bg-muted'
                }`}
              >
                <span
                  className="h-2 w-2 shrink-0 rounded-full"
                  style={{ background: PARAM_TYPE_COLORS[t.value] ?? '#c8c8c8' }}
                />
                <span className="truncate">{t.label}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
