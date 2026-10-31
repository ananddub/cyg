import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Zap } from 'lucide-react';
import type { TriggerNodeData } from '../types';
import { RuleSelect } from '../RuleSelect';

export function TriggerNode({ data }: NodeProps & { data: TriggerNodeData }) {
  const events = [
    { value: 'player.stat_update', label: 'Stat Update (Kills, Score, etc.)' },
    { value: 'match.ended', label: 'Match Ended' },
    { value: 'player.level_up', label: 'Player Level Up' },
    { value: 'player.login', label: 'Daily Login' },
    { value: 'iap.purchase_completed', label: 'Purchase Completed' },
  ];

  return (
    <div className="w-72 rounded-xl border border-violet-500/50 bg-card/95 p-3.5 shadow-xl shadow-violet-500/10 backdrop-blur-md">
      <div className="flex items-center gap-2.5 border-b border-border pb-2.5">
        <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-violet-500/20 text-violet-600 dark:text-violet-300">
          <Zap className="h-4 w-4" />
        </div>
        <div>
          <div className="text-xs font-bold uppercase tracking-wider text-violet-600 dark:text-violet-300">Trigger</div>
        </div>
      </div>

      <div className="mt-3 nodrag nopan">
        <div className="mt-1">
          <RuleSelect
            options={events}
            value={data.event}
            accent="violet"
            onValueChange={(val) => data.onDataChange?.({ event: val })}
          />
        </div>
      </div>

      <Handle
        type="source"
        position={Position.Right}
        id="out"
        className="!h-3.5 !w-3.5 !rounded-full !border-2 !border-card !bg-violet-500"
      />
    </div>
  );
}
