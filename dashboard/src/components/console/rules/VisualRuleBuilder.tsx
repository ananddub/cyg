import { useState, useCallback, useMemo, useEffect, useRef } from 'react';
import {
  ReactFlow,
  Controls,
  Background,
  MiniMap,
  BackgroundVariant,
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
  MarkerType,
  getBezierPath,
  Position,
  useReactFlow,
  type Node,
  type Edge,
  type OnNodesChange,
  type OnEdgesChange,
  type OnConnect,
} from '@xyflow/react';
import {
  Filter,
  GitBranch,
  Gift,
  Play,
  Code2,
  Workflow,
  Sparkles,
  CheckCircle2,
  XCircle,
  Maximize2,
  Minimize2,
  LayoutGrid,
  Unplug,
  GitBranchPlus,
  Filter as FilterPlus,
  Loader2,
} from 'lucide-react';

import { TriggerNode } from './nodes/TriggerNode';
import { ConditionNode } from './nodes/ConditionNode';
import { LogicGateNode } from './nodes/LogicGateNode';
import { ActionNode } from './nodes/ActionNode';
import { graphToRuleJson, ruleJsonToGraph, getNodeColor, getLayoutedElements, NODE_SIZE } from './graph-to-rule';
import { RuleSelect } from './RuleSelect';
import { Button } from '@/components/ui/button';
import { LiveJsonEditor } from './LiveJsonEditor';
import { useTheme } from '@/lib/theme';

const PRESETS = {
  centurion: {
    name: 'Centurion (100 Kills + Gold)',
    json: {
      all: [
        { field: 'kills', op: 'gte', value: 100 },
        { field: 'rank', op: 'eq', value: 'gold' },
      ],
    },
    mockContext: { kills: 120, rank: 'gold' },
  },
  firstBlood: {
    name: 'First Blood (1 Kill)',
    json: {
      field: 'kills',
      op: 'gte',
      value: 1,
    },
    mockContext: { kills: 1 },
  },
  highRoller: {
    name: 'High Roller (Score >= 50,000)',
    json: {
      all: [
        { field: 'score', op: 'gte', value: 50000 },
        { field: 'vip_tier', op: 'gte', value: 1 },
      ],
    },
    mockContext: { score: 75000, vip_tier: 2 },
  },
  survivor: {
    name: 'Survivor (Health >= 90 & Win)',
    json: {
      all: [
        { field: 'health', op: 'gte', value: 90 },
        { field: 'won', op: 'eq', value: 'true' },
      ],
    },
    mockContext: { health: 95, won: 'true' },
  },
};

interface VisualRuleBuilderProps {
  initialJson?: string;
  onChange: (jsonString: string) => void;
  onDryRun?: (body: unknown, context: unknown) => Promise<{ matched: boolean; error?: string }>;
}

interface Packet {
  d: string;
  color: string;
}

/** A glowing packet travelling along the active edge (n8n-style flow feel). */
function TraversalOverlay({ packet }: { packet: Packet | null }) {
  const { getViewport } = useReactFlow();
  if (!packet) return null;
  const vp = getViewport();
  return (
    <div className="pointer-events-none absolute inset-0 z-[105]">
      <svg className="h-full w-full overflow-visible">
        <g transform={`translate(${vp.x},${vp.y}) scale(${vp.zoom})`}>
          <circle
            r="5"
            fill={packet.color}
            style={{
              filter: `drop-shadow(0 0 5px ${packet.color}) drop-shadow(0 0 12px ${packet.color})`,
            }}
          >
            <animateMotion dur="0.5s" path={packet.d} fill="freeze" />
          </circle>
        </g>
      </svg>
    </div>
  );
}

/** Local rule evaluation — powers per-node pass/fail during traversal. */
function evalComparison(
  c: { field: string; op: string; value: unknown },
  ctx: Record<string, unknown>,
): boolean {
  const actual = ctx[c.field];
  const v = c.value;
  switch (c.op) {
    case 'gte': return Number(actual) >= Number(v);
    case 'gt': return Number(actual) > Number(v);
    case 'lte': return Number(actual) <= Number(v);
    case 'lt': return Number(actual) < Number(v);
    case 'eq': return String(actual) === String(v);
    case 'ne': return String(actual) !== String(v);
    case 'contains':
      return Array.isArray(actual)
        ? (actual as unknown[]).includes(v)
        : String(actual ?? '').includes(String(v));
    case 'in':
      return Array.isArray(v) ? (v as unknown[]).includes(actual) : String(actual) === String(v);
    default: return false;
  }
}

function evalExpr(expr: unknown, ctx: Record<string, unknown>): boolean {
  if (!expr || typeof expr !== 'object') return true;
  const obj = expr as Record<string, unknown>;
  if ('field' in obj) return evalComparison(obj as { field: string; op: string; value: unknown }, ctx);
  if (Array.isArray(obj.all)) return (obj.all as unknown[]).every((e) => evalExpr(e, ctx));
  if (Array.isArray(obj.any)) return (obj.any as unknown[]).some((e) => evalExpr(e, ctx));
  if ('not' in obj) return !evalExpr(obj.not, ctx);
  if ('value' in obj) return Boolean(obj.value);
  return true;
}

/** Evaluation order: trigger → gates → conditions → action (BFS over edges). */
function traversalOrder(nodes: Node[], edges: Edge[]): string[] {
  const start = nodes.find((n) => n.type === 'trigger') ?? nodes[0];
  if (!start) return [];
  const order: string[] = [];
  const visited = new Set<string>();
  const queue = [start.id];
  while (queue.length > 0) {
    const id = queue.shift()!;
    if (visited.has(id)) continue;
    visited.add(id);
    order.push(id);
    for (const e of edges) {
      if (e.source === id) queue.push(e.target);
    }
  }
  return order;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const ANIM_CSS = `
@keyframes ctx-in { from { opacity: 0; transform: scale(.92) translateY(-4px); } to { opacity: 1; transform: scale(1) translateY(0); } }
.ctx-in { animation: ctx-in .12s ease-out; transform-origin: top left; }
@keyframes result-in { from { opacity: 0; transform: translateY(6px) scale(.96); } to { opacity: 1; transform: translateY(0) scale(1); } }
.result-in { animation: result-in .28s cubic-bezier(.2,.9,.3,1.25); }
@keyframes node-pulse { 0%, 100% { box-shadow: 0 0 0 3px rgba(56,189,248,.75); } 50% { box-shadow: 0 0 0 7px rgba(56,189,248,.25); } }
.react-flow__node.test-active > div { animation: node-pulse .55s ease-in-out infinite; border-radius: 14px; }
.react-flow__node.test-pass > div { box-shadow: 0 0 0 3px rgba(16,185,129,.85) !important; border-radius: 14px; transition: box-shadow .2s; }
.react-flow__node.test-fail > div { box-shadow: 0 0 0 3px rgba(244,63,94,.85) !important; border-radius: 14px; transition: box-shadow .2s; }
.react-flow__edge.test-edge-active path.react-flow__edge-path { stroke-width: 4.5 !important; }
`;

export function VisualRuleBuilder({
  initialJson,
  onChange,
  onDryRun,
}: VisualRuleBuilderProps) {
  const [activeTab, setActiveTab] = useState<'visual' | 'code'>('visual');
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [selectedPreset, setSelectedPreset] = useState<string>('');
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [packet, setPacket] = useState<Packet | null>(null);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; nodeId: string } | null>(null);
  const workspaceRef = useRef<HTMLDivElement>(null);

  // ESC exits fullscreen
  useEffect(() => {
    if (!isFullscreen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setIsFullscreen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [isFullscreen]);

  // Testing states
  const [testContext, setTestContext] = useState<string>('{\n  "kills": 120,\n  "rank": "gold"\n}');
  const [testResult, setTestResult] = useState<{ matched: boolean; error?: string } | null>(null);
  const [isTesting, setIsTesting] = useState(false);

  // JSON editor code state
  const [codeJson, setCodeJson] = useState('');

  const nodeTypes = useMemo(
    () => ({
      trigger: TriggerNode,
      condition: ConditionNode,
      logicGate: LogicGateNode,
      action: ActionNode,
    }),
    [],
  );

  const handleNodeDataChange = useCallback(
    (nodeId: string, partial: Record<string, unknown>) => {
      setNodes((nds) =>
        nds.map((n) => {
          if (n.id === nodeId) {
            return {
              ...n,
              data: {
                ...n.data,
                ...partial,
              },
            };
          }
          return n;
        }),
      );
    },
    [],
  );

  const handleDeleteNode = useCallback((nodeId: string) => {
    setNodes((nds) => nds.filter((n) => n.id !== nodeId));
    setEdges((eds) => eds.filter((e) => e.source !== nodeId && e.target !== nodeId));
  }, []);

  const handleApplyJsonToCanvas = useCallback(
    (newJsonStr?: string) => {
      const toApply = newJsonStr ?? codeJson;
      try {
        const parsed = JSON.parse(toApply);
        const graph = ruleJsonToGraph(parsed, handleNodeDataChange, handleDeleteNode);
        setNodes(graph.nodes);
        setEdges(graph.edges);
        onChange(toApply);
      } catch {
        // ignore invalid JSON
      }
    },
    [codeJson, handleNodeDataChange, handleDeleteNode, onChange],
  );

  const switchToVisual = () => {
    handleApplyJsonToCanvas(codeJson);
    setActiveTab('visual');
  };

  // Initialize from initialJson or default preset
  useEffect(() => {
    let parsed: unknown = PRESETS.centurion.json;
    if (initialJson) {
      try {
        parsed = JSON.parse(initialJson);
      } catch {
        parsed = PRESETS.centurion.json;
      }
    }
    const graph = ruleJsonToGraph(parsed, handleNodeDataChange, handleDeleteNode);
    setNodes(graph.nodes);
    setEdges(graph.edges);
    setCodeJson(JSON.stringify(parsed, null, 2));
  }, [handleNodeDataChange, handleDeleteNode]);

  // Sync graph changes to JSON output
  useEffect(() => {
    if (nodes.length > 0) {
      const generated = graphToRuleJson(nodes, edges);
      const jsonStr = JSON.stringify(generated, null, 2);
      setCodeJson(jsonStr);
      onChange(jsonStr);
    }
  }, [nodes, edges, onChange]);

  const onNodesChange: OnNodesChange = useCallback(
    (changes) => setNodes((nds) => applyNodeChanges(changes, nds)),
    [],
  );

  const onEdgesChange: OnEdgesChange = useCallback(
    (changes) => setEdges((eds) => applyEdgeChanges(changes, eds)),
    [],
  );

  const onConnect: OnConnect = useCallback(
    (connection) => {
      const sourceNode = nodes.find((n) => n.id === connection.source);
      const strokeColor = getNodeColor(sourceNode?.type);
      setEdges((eds) =>
        addEdge(
          {
            ...connection,
            animated: true,
            style: { stroke: strokeColor, strokeWidth: 2.5 },
            markerEnd: { type: MarkerType.ArrowClosed, color: strokeColor, width: 22, height: 22 },
          },
          eds,
        ),
      );
    },
    [nodes],
  );

  /** First free spot in a column — new nodes never land on top of an existing one. */
  const freePos = (colX: number, nodesNow: Node[]) => {
    const column = nodesNow.filter((n) => Math.abs(n.position.x - colX) < 170);
    const maxY = column.reduce((m, n) => Math.max(m, n.position.y), 40);
    return { x: colX, y: maxY + 210 };
  };

  /** Right-click on a node → attach-ahead / detach menu. */
  const onNodeContextMenu = useCallback((event: React.MouseEvent, node: Node) => {
    event.preventDefault();
    const bounds = workspaceRef.current?.getBoundingClientRect();
    if (!bounds) return;
    setCtxMenu({
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
      nodeId: node.id,
    });
  }, []);

  /** Spawn a node ahead of (connected from) the right-clicked node. */
  const attachFrom = (type: 'condition' | 'logicGate') => {
    if (!ctxMenu) return;
    const src = nodes.find((n) => n.id === ctxMenu.nodeId);
    if (!src) return setCtxMenu(null);

    const id = `${type}-${Date.now()}`;
    const newNode: Node = {
      id,
      type,
      position: { x: src.position.x + 360, y: src.position.y + 60 },
      data:
        type === 'condition'
          ? {
              field: 'kills',
              op: 'gte',
              value: 10,
              onDataChange: (data: Record<string, unknown>) => handleNodeDataChange(id, data),
              onDelete: () => handleDeleteNode(id),
            }
          : {
              gate: 'all',
              onDataChange: (data: Record<string, unknown>) => handleNodeDataChange(id, data),
              onDelete: () => handleDeleteNode(id),
            },
    };

    const color = getNodeColor(src.type);
    setNodes((nds) => [...nds, newNode]);
    setEdges((eds) => [
      ...eds,
      {
        id: `e-${src.id}-${id}`,
        source: src.id,
        target: id,
        animated: true,
        style: { stroke: color, strokeWidth: 2.5 },
        markerEnd: { type: MarkerType.ArrowClosed, color, width: 22, height: 22 },
      },
    ]);
    setCtxMenu(null);
  };

  /** Akela — cut every edge touching the right-clicked node. */
  const detachNode = () => {
    if (!ctxMenu) return;
    const id = ctxMenu.nodeId;
    setEdges((eds) => eds.filter((e) => e.source !== id && e.target !== id));
    setCtxMenu(null);
  };

  /** dagre re-layout of the whole graph — one click, everything separated. */
  const arrangeLayout = () => {
    const layouted = getLayoutedElements(nodes, edges);
    setNodes(layouted.nodes);
    setEdges(layouted.edges);
  };

  const addConditionNode = () => {
    const id = `cond-${Date.now()}`;
    const newNode: Node = {
      id,
      type: 'condition',
      position: freePos(380, nodes),
      data: {
        field: 'kills',
        op: 'gte',
        value: 10,
        onDataChange: (data: Record<string, unknown>) => handleNodeDataChange(id, data),
        onDelete: () => handleDeleteNode(id),
      },
    };
    setNodes((nds) => [...nds, newNode]);
  };

  const addLogicGateNode = () => {
    const id = `gate-${Date.now()}`;
    const newNode: Node = {
      id,
      type: 'logicGate',
      position: freePos(420, nodes),
      data: {
        gate: 'all',
        onDataChange: (data: Record<string, unknown>) => handleNodeDataChange(id, data),
        onDelete: () => handleDeleteNode(id),
      },
    };
    setNodes((nds) => [...nds, newNode]);
  };

  const addActionNode = () => {
    const id = `act-${Date.now()}`;
    const newNode: Node = {
      id,
      type: 'action',
      position: freePos(740, nodes),
      data: {
        actionType: 'coins',
        payload: 'coins',
        amount: 250,
        onDataChange: (data: Record<string, unknown>) => handleNodeDataChange(id, data),
        onDelete: () => handleDeleteNode(id),
      },
    };
    setNodes((nds) => [...nds, newNode]);
  };

  const loadPreset = (presetKey: keyof typeof PRESETS) => {
    const preset = PRESETS[presetKey];
    const graph = ruleJsonToGraph(preset.json, handleNodeDataChange, handleDeleteNode);
    setNodes(graph.nodes);
    setEdges(graph.edges);
    setCodeJson(JSON.stringify(preset.json, null, 2));
    setTestContext(JSON.stringify(preset.mockContext, null, 2));
    setTestResult(null);
  };

  const runTest = async () => {
    setIsTesting(true);
    setTestResult(null);
    // clear any leftover test classes
    setNodes((nds) => nds.map((n) => ({ ...n, className: undefined })));
    setEdges((eds) => eds.map((e) => ({ ...e, className: undefined })));

    let parsedContext: unknown;
    try {
      parsedContext = JSON.parse(testContext);
    } catch {
      setTestResult({ matched: false, error: 'Invalid JSON context' });
      setIsTesting(false);
      return;
    }

    const generatedRule = graphToRuleJson(nodes, edges);
    const ctxObj = parsedContext as Record<string, unknown>;
    const finalMatched = evalExpr(generatedRule, ctxObj);

    const dryPromise = onDryRun
      ? onDryRun(generatedRule, parsedContext)
      : Promise.resolve({ matched: finalMatched });

    // visual evaluation: walk through the nodes, trigger first, stop at a fatal failure
    const order = traversalOrder(nodes, edges);
    const nodeMap = new Map(nodes.map((n) => [n.id, n]));
    let stoppedAt: string | null = null;

    const markNode = (id: string, cls?: string) => {
      setNodes((nds) => nds.map((n) => (n.id === id ? { ...n, className: cls } : n)));
    };

    for (let i = 0; i < order.length; i++) {
      const id = order[i];
      const node = nodeMap.get(id);
      if (!node) continue;

      // fire a packet along the edge that feeds this node
      if (i > 0) {
        const srcNode = nodeMap.get(order[i - 1]);
        if (srcNode) {
          const sw = NODE_SIZE[srcNode.type ?? ''] ?? { w: 280, h: 180 };
          const th = NODE_SIZE[node.type ?? ''] ?? { w: 280, h: 180 };
          const [d] = getBezierPath({
            sourceX: srcNode.position.x + sw.w,
            sourceY: srcNode.position.y + sw.h / 2,
            targetX: node.position.x,
            targetY: node.position.y + th.h / 2,
            sourcePosition: Position.Right,
            targetPosition: Position.Left,
          });
          setPacket({ d, color: getNodeColor(srcNode.type) });
        }
      }

      markNode(id, 'test-active');
      setEdges((eds) =>
        eds.map((e) => ({
          ...e,
          className: e.source === id ? 'test-edge-active' : undefined,
        })),
      );
      await sleep(i === 0 ? 260 : 380);

      // evaluate this node as soon as we land on it
      if (node.type === 'condition') {
        const data = node.data as { field?: string; op?: string; value?: unknown };
        const passed = evalComparison(
          { field: data.field ?? '', op: data.op ?? 'eq', value: data.value },
          ctxObj,
        );

        if (passed) {
          markNode(id, 'test-pass');
        } else {
          markNode(id, 'test-fail');
          if (!finalMatched) {
            // fatal — stop right here, this is the node that broke the rule
            stoppedAt = `Failed at "${data.field} ${data.op} ${JSON.stringify(data.value)}" (actual: ${JSON.stringify(ctxObj[(data.field ?? '') as string])})`;
            setPacket(null);
            setEdges((eds) => eds.map((e) => ({ ...e, className: undefined })));
            break;
          }
          // not fatal (another branch recovers) — keep the red mark, continue
        }
      } else if (node.type !== 'trigger') {
        markNode(id, undefined);
      }
    }
    setPacket(null);

    try {
      const res = await dryPromise;
      if (res.matched) {
        setNodes((nds) =>
          nds.map((n) => (order.includes(n.id) && n.className !== 'test-fail' ? { ...n, className: 'test-pass' } : n)),
        );
      }
      setEdges((eds) => eds.map((e) => ({ ...e, className: undefined })));
      setTestResult(
        stoppedAt && !res.matched ? { matched: false, error: stoppedAt } : res,
      );
      // clear the flash after a beat
      setTimeout(() => {
        setNodes((nds) => nds.map((n) => ({ ...n, className: undefined })));
      }, 2200);
    } catch (err: unknown) {
      setTestResult({
        matched: false,
        error: err instanceof Error ? err.message : 'Dry run failed',
      });
      setNodes((nds) => nds.map((n) => ({ ...n, className: undefined })));
      setEdges((eds) => eds.map((e) => ({ ...e, className: undefined })));
      setPacket(null);
    } finally {
      setIsTesting(false);
    }
  };

  const mode = useTheme();
  const isDark = mode === 'dark';

  return (
    <div
      className={`flex flex-col overflow-hidden rounded-xl border border-border bg-card shadow-xl ${
        isFullscreen ? 'fixed inset-0 z-[100] rounded-none' : ''
      }`}
    >
      <style>{ANIM_CSS}</style>
      {/* Top Controls Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-muted/40 px-4 py-2.5">
        <div className="flex items-center gap-1.5">
          <div className="mr-2 flex items-center gap-1.5 text-xs font-semibold text-foreground">
            <Sparkles className="h-4 w-4 text-amber-500 dark:text-amber-400" />
            <span>Visual Rule Builder</span>
          </div>

          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={addConditionNode}
            className="h-8 gap-1 border-sky-500/30 bg-sky-500/10 text-xs text-sky-600 dark:text-sky-400 hover:bg-sky-500/20"
          >
            <Filter className="h-3.5 w-3.5" />
            <span>+ Condition</span>
          </Button>

          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={addLogicGateNode}
            className="h-8 gap-1 border-amber-500/30 bg-amber-500/10 text-xs text-amber-600 dark:text-amber-400 hover:bg-amber-500/20"
          >
            <GitBranch className="h-3.5 w-3.5" />
            <span>+ Gate (AND/OR)</span>
          </Button>

          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={addActionNode}
            className="h-8 gap-1 border-emerald-500/30 bg-emerald-500/10 text-xs text-emerald-600 dark:text-emerald-400 hover:bg-emerald-500/20"
          >
            <Gift className="h-3.5 w-3.5" />
            <span>+ Action</span>
          </Button>

          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={arrangeLayout}
            title="Auto-arrange the whole graph (dagre layout)"
            className="h-8 gap-1 border-violet-500/30 bg-violet-500/10 text-xs text-violet-600 dark:text-violet-400 hover:bg-violet-500/20"
          >
            <LayoutGrid className="h-3.5 w-3.5" />
            <span>Arrange</span>
          </Button>
        </div>

        <div className="flex items-center gap-2.5">
          {/* Preset templates */}
          <div className="w-64">
            <RuleSelect
              placeholder="Load Preset Template..."
              options={Object.entries(PRESETS).map(([key, p]) => ({
                value: key,
                label: p.name,
              }))}
              value={selectedPreset}
              accent="default"
              onValueChange={(key) => {
                if (key) {
                  loadPreset(key as keyof typeof PRESETS);
                  setSelectedPreset(key);
                }
              }}
            />
          </div>

          {/* Fullscreen toggle */}
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => setIsFullscreen((v) => !v)}
            className="h-8 w-8 p-0"
            title={isFullscreen ? 'Exit fullscreen (Esc)' : 'Fullscreen'}
          >
            {isFullscreen ? <Minimize2 className="h-3.5 w-3.5" /> : <Maximize2 className="h-3.5 w-3.5" />}
          </Button>

          {/* Mode Switcher */}
          <div className="flex rounded-lg border border-border bg-muted/80 p-0.5 shadow-xs">
            <button
              type="button"
              onClick={switchToVisual}
              className={`flex items-center gap-1.5 rounded px-2.5 py-1 text-xs font-semibold transition ${
                activeTab === 'visual'
                  ? 'bg-card text-foreground shadow-xs'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              <Workflow className="h-3.5 w-3.5 text-sky-500 dark:text-sky-400" />
              <span>Canvas</span>
            </button>
            <button
              type="button"
              onClick={() => setActiveTab('code')}
              className={`flex items-center gap-1.5 rounded px-2.5 py-1 text-xs font-semibold transition ${
                activeTab === 'code'
                  ? 'bg-card text-foreground shadow-xs'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              <Code2 className="h-3.5 w-3.5 text-emerald-500 dark:text-emerald-400" />
              <span>JSON</span>
            </button>
          </div>
        </div>
      </div>

      {/* Main Workspace */}
      <div
        ref={workspaceRef}
        className={`relative w-full bg-muted/20 dark:bg-zinc-950 ${
          isFullscreen ? 'min-h-0 flex-1' : 'h-[480px]'
        }`}
      >
        {activeTab === 'visual' ? (
          <ReactFlow
            key={isFullscreen ? 'fs' : 'normal'}
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeContextMenu={onNodeContextMenu}
            onPaneClick={() => setCtxMenu(null)}
            onMoveStart={() => setCtxMenu(null)}
            nodeTypes={nodeTypes}
            colorMode={isDark ? 'dark' : 'light'}
            fitView
            fitViewOptions={{ padding: 0.25 }}
            minZoom={0.2}
            maxZoom={1.5}
            defaultEdgeOptions={{
              animated: true,
              style: { stroke: '#8b5cf6', strokeWidth: 2.5 },
              markerEnd: { type: MarkerType.ArrowClosed, color: '#8b5cf6', width: 22, height: 22 },
            }}
          >
            <Background variant={BackgroundVariant.Dots} gap={16} size={1} color={isDark ? '#3f3f46' : '#cbd5e1'} />
            <TraversalOverlay packet={packet} />
            <Controls />
            <MiniMap
              className="!border-border/60 !bg-card/90"
              nodeColor={(node) => {
                switch (node.type) {
                  case 'trigger':
                    return '#8b5cf6';
                  case 'condition':
                    return '#0284c7';
                  case 'logicGate':
                    return '#f59e0b';
                  case 'action':
                    return '#10b981';
                  default:
                    return '#71717a';
                }
              }}
            />
          </ReactFlow>
        ) : (
          <LiveJsonEditor
            value={codeJson}
            onChange={(val) => {
              setCodeJson(val);
              try {
                const parsed = JSON.parse(val);
                const graph = ruleJsonToGraph(parsed, handleNodeDataChange, handleDeleteNode);
                setNodes(graph.nodes);
                setEdges(graph.edges);
                onChange(val);
              } catch {
                // partial typing, don't update canvas yet
              }
            }}
            onApplyToCanvas={() => handleApplyJsonToCanvas(codeJson)}
          />
        )}

        {/* right-click context menu — clamped inside the workspace, animated */}
        {ctxMenu && activeTab === 'visual' && (
          <div
            className="ctx-in absolute z-[110] w-60 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl shadow-black/25"
            style={{
              left: Math.min(ctxMenu.x, (workspaceRef.current?.clientWidth ?? 800) - 248),
              top: Math.min(ctxMenu.y, (workspaceRef.current?.clientHeight ?? 480) - 172),
            }}
          >
            <div className="border-b border-border/60 bg-muted/40 px-3.5 py-2 text-[10px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
              Node actions
            </div>
            <div className="p-1.5">
              <button
                type="button"
                onClick={() => attachFrom('condition')}
                className="group flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left transition hover:bg-sky-500/10"
              >
                <span className="flex h-7 w-7 items-center justify-center rounded-md bg-sky-500/15 text-sky-600 dark:text-sky-400">
                  <FilterPlus className="h-3.5 w-3.5" />
                </span>
                <span>
                  <span className="block text-xs font-semibold text-foreground">Attach condition ahead</span>
                  <span className="block text-[10px] text-muted-foreground">New comparison, connected from here</span>
                </span>
              </button>
              <button
                type="button"
                onClick={() => attachFrom('logicGate')}
                className="group flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left transition hover:bg-amber-500/10"
              >
                <span className="flex h-7 w-7 items-center justify-center rounded-md bg-amber-500/15 text-amber-600 dark:text-amber-400">
                  <GitBranchPlus className="h-3.5 w-3.5" />
                </span>
                <span>
                  <span className="block text-xs font-semibold text-foreground">Attach gate ahead</span>
                  <span className="block text-[10px] text-muted-foreground">AND / OR / NOT branch, connected</span>
                </span>
              </button>
              <button
                type="button"
                onClick={detachNode}
                className="group flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left transition hover:bg-rose-500/10"
              >
                <span className="flex h-7 w-7 items-center justify-center rounded-md bg-rose-500/15 text-rose-600 dark:text-rose-400">
                  <Unplug className="h-3.5 w-3.5" />
                </span>
                <span>
                  <span className="block text-xs font-semibold text-foreground">Detach (akela)</span>
                  <span className="block text-[10px] text-muted-foreground">Cut every edge to this node</span>
                </span>
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Interactive Testing & Dry Run Bar */}
      <div className="border-t border-border bg-muted/40 p-3.5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-1 items-center gap-3">
            <div className="flex items-center gap-1.5 text-xs font-bold text-foreground">
              <Play className="h-3.5 w-3.5 text-sky-500 dark:text-sky-400" />
              <span>Test Rule Context:</span>
            </div>
            <input
              type="text"
              className="h-8 flex-1 max-w-md rounded-lg border border-input bg-background px-3 font-mono text-xs font-medium text-foreground placeholder:text-muted-foreground shadow-xs transition focus:border-ring focus:ring-1 focus:ring-ring focus:outline-none"
              value={testContext}
              onChange={(e) => setTestContext(e.target.value)}
              placeholder='e.g. {"kills": 120, "rank": "gold"}'
            />
            <Button
              type="button"
              size="sm"
              onClick={runTest}
              disabled={isTesting}
              className="h-8 gap-1.5 rounded-lg bg-sky-500 hover:bg-sky-400 dark:bg-sky-400 dark:hover:bg-sky-300 text-xs font-bold text-white dark:text-zinc-950 shadow-xs transition active:scale-95 disabled:opacity-80"
            >
              {isTesting ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  <span>Evaluating…</span>
                </>
              ) : (
                <>
                  <Play className="h-3.5 w-3.5" />
                  <span>Run Dry Test</span>
                </>
              )}
            </Button>
          </div>

          {/* Test Evaluation Result Banner */}
          {testResult && (
            <div className="result-in flex items-center gap-2" key={String(testResult.matched) + String(testResult.error)}>
              {testResult.matched ? (
                <div className="flex items-center gap-1.5 rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-1 text-xs font-semibold text-emerald-600 dark:text-emerald-400">
                  <CheckCircle2 className="h-4 w-4" />
                  <span>Rule MATCHED (Conditions Passed)</span>
                </div>
              ) : (
                <div className="flex items-center gap-1.5 rounded-md border border-rose-500/40 bg-rose-500/10 px-3 py-1 text-xs font-semibold text-rose-600 dark:text-rose-400">
                  <XCircle className="h-4 w-4" />
                  <span>
                    {testResult.error ? `Error: ${testResult.error}` : 'NO MATCH (Conditions Failed)'}
                  </span>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
