export type RuleOperator = 'eq' | 'ne' | 'gt' | 'gte' | 'lt' | 'lte' | 'in' | 'contains';

export interface TriggerNodeData {
  event: string;
  label?: string;
  onDataChange?: (data: Partial<TriggerNodeData>) => void;
}

export interface ConditionNodeData {
  field: string;
  op: RuleOperator;
  value: string | number;
  label?: string;
  onDataChange?: (data: Partial<ConditionNodeData>) => void;
  onDelete?: () => void;
}

export interface LogicGateNodeData {
  gate: 'all' | 'any' | 'not';
  label?: string;
  onDataChange?: (data: Partial<LogicGateNodeData>) => void;
  onDelete?: () => void;
}

export interface ActionNodeData {
  actionType: 'coins' | 'badge' | 'skin' | 'event';
  payload: string;
  amount?: number;
  label?: string;
  onDataChange?: (data: Partial<ActionNodeData>) => void;
  onDelete?: () => void;
}
