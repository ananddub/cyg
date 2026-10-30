import type { Node, Edge } from '@xyflow/react';
import type { ConditionNodeData, LogicGateNodeData } from './types';

export interface RuleComparison {
  field: string;
  op: string;
  value: unknown;
}

export interface RuleGroupAll {
  all: RuleExpression[];
}

export interface RuleGroupAny {
  any: RuleExpression[];
}

export interface RuleGroupNot {
  not: RuleExpression;
}

export interface RuleTruth {
  value: boolean;
}

export type RuleExpression =
  | RuleComparison
  | RuleGroupAll
  | RuleGroupAny
  | RuleGroupNot
  | RuleTruth;

/**
 * Converts XYFlow nodes & edges to the engine-api rule expression tree.
 */
export function graphToRuleJson(nodes: Node[], edges: Edge[]): RuleExpression {
  const conditionNodes = nodes.filter((n) => n.type === 'condition');
  const gateNodes = nodes.filter((n) => n.type === 'logicGate');

  if (conditionNodes.length === 0 && gateNodes.length === 0) {
    return { value: true };
  }

  // Helper to convert a single condition node to comparison JSON
  const toComparison = (node: Node): RuleComparison => {
    const data = node.data as unknown as ConditionNodeData;
    return {
      field: data.field || 'player.kills',
      op: data.op || 'gte',
      value: data.value ?? 0,
    };
  };

  // If there's a logic gate node
  if (gateNodes.length > 0) {
    const primaryGate = gateNodes[0];
    const gateData = primaryGate.data as unknown as LogicGateNodeData;
    const gateType = gateData.gate || 'all';

    // Find all condition nodes that target this gate
    const incomingEdgeSourceIds = edges
      .filter((e) => e.target === primaryGate.id)
      .map((e) => e.source);

    const connectedConditions = conditionNodes.filter((c) =>
      incomingEdgeSourceIds.includes(c.id),
    );

    const condList =
      connectedConditions.length > 0 ? connectedConditions : conditionNodes;

    const children: RuleExpression[] = condList.map(toComparison);

    if (gateType === 'all') {
      return { all: children.length > 0 ? children : [{ field: 'kills', op: 'gte', value: 0 }] };
    }
    if (gateType === 'any') {
      return { any: children.length > 0 ? children : [{ field: 'kills', op: 'gte', value: 0 }] };
    }
    if (gateType === 'not') {
      return { not: children[0] || { field: 'kills', op: 'gte', value: 0 } };
    }
  }

  // If multiple condition nodes with no gate, wrap in "all"
  if (conditionNodes.length > 1) {
    return {
      all: conditionNodes.map(toComparison),
    };
  }

  // Single condition node
  return toComparison(conditionNodes[0]);
}

/**
 * Parses an existing rule JSON into visual graph nodes and edges.
 */
export function ruleJsonToGraph(
  json: unknown,
  onNodeChange: (id: string, partial: Record<string, unknown>) => void,
  onDeleteNode: (id: string) => void,
): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = [];
  const edges: Edge[] = [];

  // Always create an entry Trigger node on the left
  const triggerId = 'node-trigger-1';
  nodes.push({
    id: triggerId,
    type: 'trigger',
    position: { x: 50, y: 150 },
    data: {
      event: 'player.stat_update',
      onDataChange: (data: Record<string, unknown>) => onNodeChange(triggerId, data),
    },
  });

  let currentX = 360;
  let currentY = 80;

  const parseExpr = (
    expr: unknown,
    parentSourceId: string,
    depth: number,
  ) => {
    if (!expr || typeof expr !== 'object') return;
    const obj = expr as Record<string, unknown>;

    if (Array.isArray(obj.all) || Array.isArray(obj.any)) {
      const isAll = Array.isArray(obj.all);
      const list = (isAll ? obj.all : obj.any) as unknown[];
      const gateId = `gate-${depth}-${Math.random().toString(36).substring(2, 7)}`;

      nodes.push({
        id: gateId,
        type: 'logicGate',
        position: { x: currentX, y: currentY },
        data: {
          gate: isAll ? 'all' : 'any',
          onDataChange: (data: Record<string, unknown>) => onNodeChange(gateId, data),
          onDelete: () => onDeleteNode(gateId),
        },
      });

      edges.push({
        id: `e-${parentSourceId}-${gateId}`,
        source: parentSourceId,
        target: gateId,
        animated: true,
        style: { stroke: '#f59e0b', strokeWidth: 2 },
      });

      const nextX = currentX + 300;
      let branchY = currentY - ((list.length - 1) * 70);

      list.forEach((child, idx) => {
        const condId = `cond-${depth}-${idx}-${Math.random().toString(36).substring(2, 7)}`;
        if (child && typeof child === 'object' && 'field' in child) {
          const c = child as Record<string, unknown>;
          nodes.push({
            id: condId,
            type: 'condition',
            position: { x: nextX, y: branchY },
            data: {
              field: String(c.field ?? 'kills'),
              op: String(c.op ?? 'gte'),
              value: c.value ?? 0,
              onDataChange: (data: Record<string, unknown>) => onNodeChange(condId, data),
              onDelete: () => onDeleteNode(condId),
            },
          });

          edges.push({
            id: `e-${gateId}-${condId}`,
            source: gateId,
            target: condId,
            animated: true,
            style: { stroke: '#38bdf8', strokeWidth: 2 },
          });

          branchY += 150;
        }
      });

      currentX = nextX + 340;
    } else if ('field' in obj) {
      const condId = `cond-single-${Math.random().toString(36).substring(2, 7)}`;
      nodes.push({
        id: condId,
        type: 'condition',
        position: { x: currentX, y: currentY },
        data: {
          field: String(obj.field ?? 'kills'),
          op: String(obj.op ?? 'gte'),
          value: obj.value ?? 0,
          onDataChange: (data: Record<string, unknown>) => onNodeChange(condId, data),
          onDelete: () => onDeleteNode(condId),
        },
      });

      edges.push({
        id: `e-${parentSourceId}-${condId}`,
        source: parentSourceId,
        target: condId,
        animated: true,
        style: { stroke: '#38bdf8', strokeWidth: 2 },
      });

      currentX += 340;
    }
  };

  parseExpr(json, triggerId, 0);

  // Add an Action (Reward) node at the end
  const actionId = 'action-reward-1';
  nodes.push({
    id: actionId,
    type: 'action',
    position: { x: Math.max(currentX, 750), y: 150 },
    data: {
      actionType: 'badge',
      payload: 'Centurion_Badge',
      amount: 500,
      onDataChange: (data: Record<string, unknown>) => onNodeChange(actionId, data),
      onDelete: () => onDeleteNode(actionId),
    },
  });

  // Connect last condition to action
  const lastCond = nodes.filter((n) => n.type === 'condition').pop();
  if (lastCond) {
    edges.push({
      id: `e-${lastCond.id}-${actionId}`,
      source: lastCond.id,
      target: actionId,
      animated: true,
      style: { stroke: '#10b981', strokeWidth: 2 },
    });
  }

  return { nodes, edges };
}
