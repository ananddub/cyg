export type RuleOperator = 'eq' | 'ne' | 'gt' | 'gte' | 'lt' | 'lte' | 'in' | 'contains';

export interface CardParam {
  /** Stable id — pin/wire isse bind hote hain, rename se wire toot-ta nahi. */
  uid: string;
  name: string;
  type: 'number' | 'string' | 'bool';
  default: string;
}

export interface TriggerNodeData {
  event: string;
  /** Card-level parameters: name/type/default, editable on the card itself.
      Each param also renders a named data pin on the card's right side. */
  params?: CardParam[];
  label?: string;
  onDataChange?: (data: Partial<TriggerNodeData>) => void;
}

export interface ConditionNodeData {
  field: string;
  op: RuleOperator;
  value: string | number;
  /** Literal compares against a typed number; variable compares against a
      named value from the rule's own variables, so the threshold is tunable. */
  valueKind?: 'literal' | 'var';
  /** The variable name when `valueKind` is `var`. */
  valueVar?: string;
  /** Param ke type ka color — Var label isme dikhta hai. */
  varColor?: string;
  /** Kaunsa param (uid) is condition se wired hai — realtime sync ke liye. */
  boundUid?: string;
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

export interface CompareNodeData {
  /** A side: the stat field name (kills, rank…) — ya param wire se avar. */
  a: string;
  /** A pe wire hua param ka naam + color. */
  avar?: string;
  acolor?: string;
  op: RuleOperator;
  /** B side: wired from a param pin (bvar) or a typed literal (bval). */
  bvar?: string;
  /** Param ke type ka color. */
  bcolor?: string;
  bval?: string | number;
  label?: string;
  onDataChange?: (data: Partial<CompareNodeData>) => void;
  onDelete?: () => void;
}

export interface BranchNodeData {
  label?: string;
  onDataChange?: (data: Partial<BranchNodeData>) => void;
  onDelete?: () => void;
}
