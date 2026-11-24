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
  type Node,
  type Edge,
  type OnNodesChange,
  type OnEdgesChange,
  type OnConnect,
} from '@xyflow/react';
import {
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
  Filter as FilterPlus,
  Loader2,
} from 'lucide-react';

import { CompareNode } from './nodes/CompareNode';
import type { CardParam, ConditionNodeData, TriggerNodeData } from './types';
import { BranchNode } from './nodes/BranchNode';
import { PARAM_TYPE_COLORS, TriggerNode } from './nodes/TriggerNode';
import { ConditionNode } from './nodes/ConditionNode';
import { LogicGateNode } from './nodes/LogicGateNode';
import { ActionNode } from './nodes/ActionNode';
import {
  graphToRuleDocument,
  graphToRuleJson,
  ruleJsonToGraph,
} from './graph-to-rule';
import { RuleSelect } from './RuleSelect';
import { Button } from '@/components/ui/button';
import { LiveJsonEditor } from './LiveJsonEditor';

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
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; nodeId: string } | null>(null);

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
      compare: CompareNode,
      branch: BranchNode,
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
      // Trigger ke params badle (naam/type/value) — to uske param pins se
      // wired conditions ki binding turant sync: naam + color realtime.
      if ('params' in partial && Array.isArray(partial.params)) {
        const params = partial.params as CardParam[];
        // Ek pass me sab sync: wire color, Condition binding, Compare A/B
        // binding — naam/type/value jo bhi badla, sab turant.
        setEdges((eds) => {
          const paramEdges: { edge: Edge; uid: string; param: CardParam }[] = [];
          const next = eds.map((edge) => {
            if (edge.source !== nodeId || !edge.sourceHandle?.startsWith('param-')) return edge;
            const uid = edge.sourceHandle.slice('param-'.length);
            const param = params.find((p) => p.uid === uid);
            if (!param) return edge;
            const color = PARAM_TYPE_COLORS[param.type] ?? '#c8c8c8';
            paramEdges.push({ edge, uid, param });
            return {
              ...edge,
              style: { ...edge.style, stroke: color, strokeWidth: 1.5 },
              markerEnd: { type: MarkerType.ArrowClosed, color, width: 16, height: 16 },
            };
          });
          for (const { edge, uid, param } of paramEdges) {
            const color = PARAM_TYPE_COLORS[param.type] ?? '#c8c8c8';
            const side = edge.targetHandle === 'b' ? 'b' : 'a';
            setNodes((nds2) =>
              nds2.map((n) => {
                if (n.id !== edge.target) return n;
                // Compare: A ya B side pe wired param ka naam + color.
                if (n.type === 'compare' && edge.targetHandle === side) {
                  const d = n.data as unknown as Record<string, unknown>;
                  if (side === 'b' ? d.bvar === undefined : (d as { avar?: string }).avar === undefined) return n;
                  return {
                    ...n,
                    data: {
                      ...d,
                      [side === 'b' ? 'bvar' : 'avar']: param.name,
                      [side === 'b' ? 'bcolor' : 'acolor']: color,
                    },
                  };
                }
                // Condition: var binding ka naam + color.
                if (n.type === 'condition') {
                  const d = n.data as unknown as ConditionNodeData;
                  if (d.valueKind !== 'var' || d.boundUid !== uid) return n;
                  return {
                    ...n,
                    data: { ...d, valueVar: param.name, varColor: color, boundUid: uid },
                  };
                }
                return n;
              }),
            );
          }
          return next;
        });
      }
    },
    [],
  );
  const handleDeleteNode = useCallback((nodeId: string) => {
    setNodes((nds) => nds.filter((n) => n.id !== nodeId));
    setEdges((eds) => eds.filter((e) => e.source !== nodeId && e.target !== nodeId));
  }, []);

  // Variable defaults the graph does not know about. `ruleJsonToGraph` draws
  // the var references but not the tuned defaults, so they are remembered
  // here and re-merged on save — reopening a rule never resets an operator's
  // `min_kills: 10` back to the graph's 0.
  const storedVariables = useRef<Record<string, unknown>>({});
  const rememberVariables = useCallback((parsed: unknown) => {
    const vars = (parsed as Record<string, unknown> | null)?.['variables'];
    storedVariables.current =
      vars && typeof vars === 'object' && !Array.isArray(vars)
        ? (vars as Record<string, unknown>)
        : {};
  }, []);

  const handleApplyJsonToCanvas = useCallback(
    (newJsonStr?: string) => {
      const toApply = newJsonStr ?? codeJson;
      try {
        const parsed = JSON.parse(toApply);
        rememberVariables(parsed);
        const graph = ruleJsonToGraph(parsed, handleNodeDataChange, handleDeleteNode);
        setNodes(graph.nodes);
        setEdges(graph.edges);
        onChange(toApply);
      } catch {
        // ignore invalid JSON
      }
    },
    [codeJson, handleNodeDataChange, handleDeleteNode, onChange, rememberVariables],
  );

  // Initialize: initialJson hai to wo, warna KHALI canvas + ek khali Trigger
  // node — default logic apne aap na aaye (operator zero se banaye).
  useEffect(() => {
    if (initialJson) {
      try {
        const parsed = JSON.parse(initialJson);
        rememberVariables(parsed);
        const graph = ruleJsonToGraph(parsed, handleNodeDataChange, handleDeleteNode);
        setNodes(graph.nodes);
        setEdges(graph.edges);
        setCodeJson(JSON.stringify(parsed, null, 2));
        return;
      } catch {
        // fall through to empty
      }
    }
    const id = `trigger-${Date.now()}`;
    setNodes([
      {
        id,
        type: 'trigger',
        position: { x: 40, y: 140 },
        data: {
          event: 'purchase',
          params: [],
          onDataChange: (data: Record<string, unknown>) => handleNodeDataChange(id, data),
          onDelete: () => handleDeleteNode(id),
        },
      } as Node,
    ]);
    setEdges([]);
    setCodeJson(JSON.stringify({ value: true }, null, 2));
    onChange(JSON.stringify({ value: true }, null, 2));
  }, [handleNodeDataChange, handleDeleteNode, rememberVariables]);

  // Sync graph changes to JSON output — trigger/actions merged in, variables
  // stored defaults preserved.
  useEffect(() => {
    if (nodes.length > 0) {
      const expression = graphToRuleJson(nodes, edges);
      const document = graphToRuleDocument(nodes, edges);
      const variables = { ...storedVariables.current, ...((document.variables as Record<string, unknown>) ?? {}) };
      const documentWithVariables =
        Object.keys(variables).length > 0 ? { ...document, variables } : document;
      const generated =
        expression && typeof expression === 'object' && !Array.isArray(expression)
          ? { ...expression, ...documentWithVariables }
          : { ...documentWithVariables, value: true };
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
      const source = nodes.find((n) => n.id === connection.source);
      // Param wire ka color type batata hai (UE Blueprint jaisa).
      let wireColor = '#94a3b8';
      if (source?.type === 'trigger' && connection.sourceHandle?.startsWith('param-')) {
        const paramUid = connection.sourceHandle.slice('param-'.length);
        const triggerData = source.data as unknown as TriggerNodeData;
        const param = triggerData.params?.find((p) => p.uid === paramUid);
        const paramName = param?.name ?? '';
        const paramType = param?.type ?? 'number';
        wireColor = PARAM_TYPE_COLORS[paramType] ?? '#c8c8c8';
        const target = nodes.find((n) => n.id === connection.target);
        if (target?.type === 'condition' && connection.targetHandle === 'var') {
          handleNodeDataChange(connection.target, {
            valueKind: 'var',
            valueVar: paramName,
            varColor: wireColor,
            boundUid: paramUid,
          });
        }
        // Branch condition wire lagi — flag set karo taki card pe hint dikhe.
        if (target?.type === 'branch' && connection.targetHandle === 'condition') {
          handleNodeDataChange(connection.target, { conditionWired: true });
        }
        // Compare node ke A aur B dono pe param pass — data wire hi binding.
        if (target?.type === 'compare' && connection.targetHandle === 'b') {
          handleNodeDataChange(connection.target, {
            bvar: paramName,
            bcolor: wireColor,
          });
        }
        if (target?.type === 'compare' && connection.targetHandle === 'a') {
          handleNodeDataChange(connection.target, {
            avar: paramName,
            acolor: wireColor,
          });
        }
      }
      setEdges((eds) =>
        addEdge(
          {
            ...connection,
            style: { stroke: wireColor, strokeWidth: 1.5 },
            markerEnd: { type: MarkerType.ArrowClosed, color: wireColor, width: 16, height: 16 },
          },
          eds,
        ),
      );
    },
    [nodes, handleNodeDataChange],
  );

  /** Naye nodes ke liye khali jagah — existing nodes ke right-most edge ke
      aage, taki naya node hamesha dikhe. */
  const freePos = (baseX: number, nds: Node[]) => {
    const maxX = nds.reduce((max, n) => Math.max(max, n.position.x), 0);
    return { x: baseX === 420 ? maxX + 360 : baseX, y: 140 };
  };

  const addConditionNode = () => {
    const id = `cond-${Date.now()}`;
    const newNode: Node = {
      id,
      type: 'condition',
      position: freePos(420, nodes),
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

  const addBranchNode = () => {
    const id = `branch-${Date.now()}`;
    const newNode: Node = {
      id,
      type: 'branch',
      position: freePos(420, nodes),
      data: {
        onDataChange: (data: Record<string, unknown>) => handleNodeDataChange(id, data),
        onDelete: () => handleDeleteNode(id),
      },
    };
    setNodes((nds) => [...nds, newNode]);
  };

  const addCompareNode = () => {
    const id = `compare-${Date.now()}`;
    const newNode: Node = {
      id,
      type: 'compare',
      position: freePos(420, nodes),
      data: {
        a: 'kills',
        op: 'gte',
        bval: 10,
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
      position: freePos(420, nodes),
      data: {
        actionType: 'coins',
        payload: 'coins',
        amount: 100,
        onDataChange: (data: Record<string, unknown>) => handleNodeDataChange(id, data),
        onDelete: () => handleDeleteNode(id),
      },
    };
    setNodes((nds) => [...nds, newNode]);
  };

  const arrangeLayout = () => {
    const expression = graphToRuleJson(nodes, edges);
    void expression;
    // dagre auto-layout — original order me wapas fit
    const graph = ruleJsonToGraph(safeParseJson(codeJson), handleNodeDataChange, handleDeleteNode);
    setNodes(graph.nodes);
    setEdges(graph.edges);
  };

  const runTest = useCallback(async () => {
    if (!onDryRun) return;
    setIsTesting(true);
    setTestResult(null);
    try {
      const body: unknown = JSON.parse(codeJson);
      let context: unknown;
      try {
        context = JSON.parse(testContext);
      } catch {
        setTestResult({ matched: false, error: 'Test context is not valid JSON' });
        return;
      }
      const result = await onDryRun(body, context);
      setTestResult(result);
    } catch {
      setTestResult({ matched: false, error: 'Rule JSON is not valid' });
    } finally {
      setIsTesting(false);
    }
  }, [codeJson, testContext, onDryRun]);

  const fullscreenRef = useRef<HTMLDivElement>(null);
  const toggleFullscreen = () => {
    if (!isFullscreen && fullscreenRef.current) {
      fullscreenRef.current.requestFullscreen?.();
      setIsFullscreen(true);
    } else if (document.fullscreenElement) {
      document.exitFullscreen?.();
      setIsFullscreen(false);
    } else {
      setIsFullscreen(false);
    }
  };

  // context menu (node pe right-click): delete node
  const ctxTarget = ctxMenu ? nodes.find((n) => n.id === ctxMenu.nodeId) : null;

  return (
    <div ref={fullscreenRef} className={isFullscreen ? 'fixed inset-0 z-[120] bg-background p-4' : ''}>
      <div className="space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <span className="flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-foreground">
            <Sparkles className="h-3.5 w-3.5 text-primary" />
            Visual Rule Builder
          </span>
          <Button type="button" size="sm" variant="outline" onClick={addConditionNode} className="h-8 gap-1">
            <FilterPlus className="h-3.5 w-3.5" />
            <span>+ Condition</span>
          </Button>
          <Button type="button" size="sm" variant="outline" onClick={addBranchNode} className="h-8 gap-1">
            <span>+ Branch</span>
          </Button>
          <Button type="button" size="sm" variant="outline" onClick={addCompareNode} className="h-8 gap-1">
            <span>+ Compare</span>
          </Button>
          <Button type="button" size="sm" variant="outline" onClick={addActionNode} className="h-8 gap-1">
            <Gift className="h-3.5 w-3.5" />
            <span>+ Action</span>
          </Button>
          <Button type="button" size="sm" variant="outline" onClick={arrangeLayout} className="h-8 gap-1">
            <LayoutGrid className="h-3.5 w-3.5" />
            <span>Arrange</span>
          </Button>
          <span className="text-xs text-muted-foreground">
            Drag from a node&apos;s Out pin to the next node&apos;s In pin · param pin se Condition ke Var pin pe wire
          </span>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <RuleSelect
            options={Object.entries(PRESETS).map(([key, preset]) => ({ value: key, label: preset.name }))}
            value={selectedPreset}
            placeholder="Load Preset Template..."
            className="max-w-64"
            onValueChange={(val) => {
              setSelectedPreset(val);
              const preset = PRESETS[val as keyof typeof PRESETS];
              if (preset) {
                setTestContext(JSON.stringify(preset.mockContext, null, 2));
                const jsonStr = JSON.stringify(preset.json, null, 2);
                rememberVariables(preset.json);
                const graph = ruleJsonToGraph(preset.json, handleNodeDataChange, handleDeleteNode);
                setNodes(graph.nodes);
                setEdges(graph.edges);
                setCodeJson(jsonStr);
                onChange(jsonStr);
              }
            }}
          />
          <Button type="button" size="sm" variant="outline" onClick={toggleFullscreen} className="h-8 gap-1">
            {isFullscreen ? <Minimize2 className="h-3.5 w-3.5" /> : <Maximize2 className="h-3.5 w-3.5" />}
          </Button>
          <button
            type="button"
            onClick={() => setActiveTab('visual')}
            className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold transition ${
              activeTab === 'visual' ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            <Workflow className="h-3.5 w-3.5" /> Canvas
          </button>
          <button
            type="button"
            onClick={() => {
              setActiveTab('code');
              handleApplyJsonToCanvas();
            }}
            className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold transition ${
              activeTab === 'code' ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            <Code2 className="h-3.5 w-3.5" /> JSON
          </button>
        </div>

        {activeTab === 'code' ? (
          <div className="space-y-2">
            <LiveJsonEditor
              value={codeJson}
              onChange={(val) => {
                setCodeJson(val);
                onChange(val);
              }}
              onApplyToCanvas={() => handleApplyJsonToCanvas()}
            />
            <Button type="button" size="sm" variant="outline" onClick={() => handleApplyJsonToCanvas()}>
              Update Canvas
            </Button>
          </div>
        ) : (
          <div
            className="h-[560px] w-full overflow-hidden rounded-lg border border-border bg-background"
            onContextMenu={(e) => e.preventDefault()}
          >
            <style>{`.react-flow__edge.selected path.react-flow__edge-path { stroke: #f87171 !important; stroke-width: 2.5 !important; } .react-flow__edge:hover path.react-flow__edge-path { stroke-width: 2.5 !important; }`}</style>
            <ReactFlow
              nodes={nodes}
              edges={edges}
              nodeTypes={nodeTypes}
              onNodesChange={onNodesChange}
              onEdgesChange={onEdgesChange}
              onConnect={onConnect}
              onNodeContextMenu={(event, node) => {
                event.preventDefault();
                setCtxMenu({ x: event.clientX, y: event.clientY, nodeId: node.id });
              }}
              onPaneClick={() => setCtxMenu(null)}
              deleteKeyCode={['Backspace', 'Delete']}
              fitView
              fitViewOptions={{ padding: 0.25 }}
              minZoom={0.2}
              maxZoom={1.5}
              defaultEdgeOptions={{
                style: { stroke: '#94a3b8', strokeWidth: 1.5 },
                markerEnd: { type: MarkerType.ArrowClosed, color: '#94a3b8', width: 16, height: 16 },
              }}
            >
              <Background variant={BackgroundVariant.Dots} gap={16} size={1} color="#3f3f46" />
              <Controls />
              <MiniMap className="!border-border/60 !bg-card/90" pannable />
            </ReactFlow>
            {ctxMenu && ctxTarget && (
              <div
                className="ctx-in fixed z-[130] min-w-40 rounded-lg border border-border bg-popover/98 p-1 shadow-2xl"
                style={{ left: ctxMenu.x, top: ctxMenu.y }}
              >
                <button
                  type="button"
                  onClick={() => {
                    handleDeleteNode(ctxMenu.nodeId);
                    setCtxMenu(null);
                  }}
                  className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs font-semibold text-destructive transition hover:bg-destructive/10"
                >
                  <Unplug className="h-3.5 w-3.5" /> Delete node
                </button>
              </div>
            )}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs font-medium text-muted-foreground">Test Rule Context:</span>
          <input
            type="text"
            className="h-8 min-w-64 flex-1 rounded-lg border border-input bg-background px-2.5 text-xs font-mono text-foreground shadow-xs"
            value={testContext}
            onChange={(e) => setTestContext(e.target.value)}
            placeholder='e.g. {"kills": 120, "rank": "gold"}'
          />
          <Button
            type="button"
            size="sm"
            onClick={runTest}
            disabled={isTesting}
            className="h-8 gap-1.5"
          >
            {isTesting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
            Run Dry Test
          </Button>
          {testResult && (
            <span
              className={`flex items-center gap-1.5 text-xs font-semibold ${
                testResult.error
                  ? 'text-amber-500'
                  : testResult.matched
                    ? 'text-emerald-500'
                    : 'text-rose-500'
              }`}
            >
              {testResult.error ? (
                <XCircle className="h-3.5 w-3.5" />
              ) : testResult.matched ? (
                <CheckCircle2 className="h-3.5 w-3.5" />
              ) : (
                <XCircle className="h-3.5 w-3.5" />
              )}
              {testResult.error ? testResult.error : testResult.matched ? 'Rule matched' : 'Rule did not match'}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

function safeParseJson(json: string | undefined): unknown {
  if (!json) return {};
  try {
    return JSON.parse(json);
  } catch {
    return {};
  }
}
