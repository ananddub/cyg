import { MarkerType, type Node, type Edge } from '@xyflow/react';
import dagre from '@dagrejs/dagre';
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

export function getNodeColor(nodeType?: string): string {
  switch (nodeType) {
    case 'trigger':
      return '#8b5cf6'; // Baingani / Violet
    case 'logicGate':
      return '#f59e0b'; // Yellow / Amber
    case 'condition':
      return '#38bdf8'; // Sky Blue
    case 'action':
      return '#10b981'; // Emerald Green
    default:
      return '#8b5cf6';
  }
}

const DUMMY: RuleComparison = { field: 'kills', op: 'gte', value: 0 };

/* Real rendered sizes per node type — dagre packs against these, so the
   separation it computes is always larger than the visible cards. */
export const NODE_SIZE: Record<string, { w: number; h: number }> = {
  trigger: { w: 300, h: 130 },
  condition: { w: 330, h: 220 },
  logicGate: { w: 250, h: 160 },
  action: { w: 310, h: 220 },
};

/**
 * dagre auto-layout (left → right). Computes a full layered arrangement —
 * ranks, spacing, no overlaps — for any graph shape, nested groups included.
 */
export function getLayoutedElements(nodes: Node[], edges: Edge[]): { nodes: Node[]; edges: Edge[] } {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: 'LR', nodesep: 60, ranksep: 110, marginx: 30, marginy: 30 });

  for (const n of nodes) {
    const size = NODE_SIZE[n.type ?? ''] ?? { w: 280, h: 180 };
    g.setNode(n.id, { width: size.w, height: size.h });
  }
  for (const e of edges) {
    g.setEdge(e.source, e.target);
  }

  dagre.layout(g);

  const layouted = nodes.map((n) => {
    const size = NODE_SIZE[n.type ?? ''] ?? { w: 280, h: 180 };
    const pos = g.node(n.id);
    return {
      ...n,
      position: { x: pos.x - size.w / 2, y: pos.y - size.h / 2 },
    };
  });

  return { nodes: layouted, edges };
}

/**
 * Converts XYFlow nodes & edges to the engine-api rule expression tree.
 * Walks the graph structurally: a gate's children are exactly the nodes its
 * outgoing edges point to (conditions or nested gates); unconnected nodes are
 * never smuggled into the output.
 */
export function graphToRuleJson(nodes: Node[], edges: Edge[]): RuleExpression {
  const conditionNodes = nodes.filter((n) => n.type === 'condition');
  const gateNodes = nodes.filter((n) => n.type === 'logicGate');

  if (conditionNodes.length === 0 && gateNodes.length === 0) {
    return { value: true };
  }

  const byId = new Map(nodes.map((n) => [n.id, n]));
  const gateIds = new Set(gateNodes.map((g) => g.id));

  const toComparison = (node: Node): RuleComparison => {
    const data = node.data as unknown as ConditionNodeData;
    return {
      field: data.field || 'kills',
      op: data.op || 'gte',
      value: data.value ?? 0,
    };
  };

  // Nodes a gate feeds: edges where the gate is the source.
  const childrenOf = (gateId: string): Node[] =>
    edges
      .filter((e) => e.source === gateId)
      .map((e) => byId.get(e.target))
      .filter((n): n is Node => Boolean(n));

  const buildFromGate = (gate: Node): RuleExpression => {
    const gateData = gate.data as unknown as LogicGateNodeData;
    const gateType = gateData.gate || 'all';

    const exprs: RuleExpression[] = [];
    for (const child of childrenOf(gate.id)) {
      if (child.type === 'condition') {
        exprs.push(toComparison(child));
      } else if (child.type === 'logicGate') {
        exprs.push(buildFromGate(child));
      }
      // trigger / action targets are not part of the expression tree
    }

    if (gateType === 'not') {
      return { not: exprs[0] ?? DUMMY };
    }
    if (gateType === 'any') {
      return { any: exprs.length > 0 ? exprs : [DUMMY] };
    }
    return { all: exprs.length > 0 ? exprs : [DUMMY] };
  };

  if (gateNodes.length > 0) {
    // Root gates: not fed by another gate. Prefer the one the trigger points at.
    const fedByGate = new Set(
      edges.filter((e) => gateIds.has(e.source)).map((e) => e.target),
    );
    const rootGates = gateNodes.filter((g) => !fedByGate.has(g.id));

    const trigger = nodes.find((n) => n.type === 'trigger');
    const triggerTarget = trigger
      ? edges.find((e) => e.source === trigger.id)?.target
      : undefined;
    const root =
      rootGates.find((g) => g.id === triggerTarget) ?? rootGates[0] ?? gateNodes[0];

    return buildFromGate(root);
  }

  // No gates: a single condition, or an implicit AND over all of them.
  if (conditionNodes.length > 1) {
    return { all: conditionNodes.map(toComparison) };
  }
  return toComparison(conditionNodes[0]);
}

/**
 * Parses an existing rule JSON into visual graph nodes and edges.
 * Fully recursive: nested all/any/not groups and comparisons at any depth.
 */
export function ruleJsonToGraph(
  json: unknown,
  onNodeChange: (id: string, partial: Record<string, unknown>) => void,
  onDeleteNode: (id: string) => void,
): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  let uid = 0;
  const nextId = (prefix: string) => `${prefix}-${uid++}`;

  // Simple layered layout: x grows with depth, every condition leaf gets its own row.
  let leafIndex = 0;
  const leafY = () => 40 + leafIndex++ * 200;
  const depthX = (depth: number) => 360 + depth * 340;

  // Entry trigger on the far left
  const triggerId = 'node-trigger-1';
  nodes.push({
    id: triggerId,
    type: 'trigger',
    position: { x: 40, y: 140 },
    data: {
      event: 'player.stat_update',
      onDataChange: (data: Record<string, unknown>) => onNodeChange(triggerId, data),
    },
  });

  const connect = (source: string, target: string, sourceType?: string) => {
    const color = getNodeColor(sourceType ?? nodes.find((n) => n.id === source)?.type);
    edges.push({
      id: `e-${source}-${target}`,
      source,
      target,
      animated: true,
      style: { stroke: color, strokeWidth: 2.5 },
      markerEnd: { type: MarkerType.ArrowClosed, color, width: 22, height: 22 },
    });
  };

  const parseExpr = (expr: unknown, parentSourceId: string, parentType: string, depth: number): number[] => {
    if (!expr || typeof expr !== 'object') return [];
    const obj = expr as Record<string, unknown>;

    if (Array.isArray(obj.all) || Array.isArray(obj.any)) {
      const isAll = Array.isArray(obj.all);
      const list = (isAll ? obj.all : obj.any) as unknown[];
      const gateId = nextId('gate');

      // children first: the gate is centered on their rows
      const childYs: number[] = [];
      for (const child of list) {
        childYs.push(...parseExpr(child, gateId, 'logicGate', depth + 1));
      }
      const y = childYs.length > 0
        ? childYs.reduce((a, b) => a + b, 0) / childYs.length
        : leafY();

      nodes.push({
        id: gateId,
        type: 'logicGate',
        position: { x: depthX(depth), y },
        data: {
          gate: isAll ? 'all' : 'any',
          onDataChange: (data: Record<string, unknown>) => onNodeChange(gateId, data),
          onDelete: () => onDeleteNode(gateId),
        },
      });
      connect(parentSourceId, gateId, parentType);
      return [y];
    }

    if ('not' in obj) {
      const gateId = nextId('gate');
      const childYs = parseExpr(obj.not, gateId, 'logicGate', depth + 1);
      const y = childYs.length > 0 ? childYs[0] : leafY();

      nodes.push({
        id: gateId,
        type: 'logicGate',
        position: { x: depthX(depth), y },
        data: {
          gate: 'not',
          onDataChange: (data: Record<string, unknown>) => onNodeChange(gateId, data),
          onDelete: () => onDeleteNode(gateId),
        },
      });
      connect(parentSourceId, gateId, parentType);
      return [y];
    }

    if ('field' in obj) {
      const condId = nextId('cond');
      const y = leafY();
      nodes.push({
        id: condId,
        type: 'condition',
        position: { x: depthX(depth), y },
        data: {
          field: String(obj.field ?? 'kills'),
          op: String(obj.op ?? 'gte'),
          value: obj.value ?? 0,
          onDataChange: (data: Record<string, unknown>) => onNodeChange(condId, data),
          onDelete: () => onDeleteNode(condId),
        },
      });
      connect(parentSourceId, condId, parentType);
      return [y];
    }
    return [];
  };

  parseExpr(json, triggerId, 'trigger', 0);

  // Reward node at the end of the flow
  const maxDepthX = nodes.reduce((max, n) => Math.max(max, n.position.x), 0);
  const actionId = 'action-reward-1';
  nodes.push({
    id: actionId,
    type: 'action',
    position: { x: Math.max(maxDepthX + 340, 750), y: 140 },
    data: {
      actionType: 'badge',
      payload: 'Centurion_Badge',
      amount: 500,
      onDataChange: (data: Record<string, unknown>) => onNodeChange(actionId, data),
      onDelete: () => onDeleteNode(actionId),
    },
  });

  const lastCond = nodes.filter((n) => n.type === 'condition').pop();
  if (lastCond) {
    const color = getNodeColor(lastCond.type);
    edges.push({
      id: `e-${lastCond.id}-${actionId}`,
      source: lastCond.id,
      target: actionId,
      animated: true,
      style: { stroke: color, strokeWidth: 2.5 },
      markerEnd: { type: MarkerType.ArrowClosed, color, width: 22, height: 22 },
    });
  }

  return getLayoutedElements(nodes, edges);
}
