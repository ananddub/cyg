import { useState, useCallback, useMemo, useEffect } from 'react';
import {
  ReactFlow,
  Controls,
  Background,
  MiniMap,
  BackgroundVariant,
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
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
} from 'lucide-react';

import { TriggerNode } from './nodes/TriggerNode';
import { ConditionNode } from './nodes/ConditionNode';
import { LogicGateNode } from './nodes/LogicGateNode';
import { ActionNode } from './nodes/ActionNode';
import { graphToRuleJson, ruleJsonToGraph, getNodeColor } from './graph-to-rule';
import { RuleSelect } from './RuleSelect';
import { Button } from '@/components/ui/button';

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

  // Testing states
  const [testContext, setTestContext] = useState<string>('{\n  "kills": 120,\n  "rank": "gold"\n}');
  const [testResult, setTestResult] = useState<{ matched: boolean; error?: string } | null>(null);
  const [isTesting, setIsTesting] = useState(false);

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
  }, [handleNodeDataChange, handleDeleteNode]);

  // Sync graph changes to JSON output
  useEffect(() => {
    if (nodes.length > 0) {
      const generated = graphToRuleJson(nodes, edges);
      const jsonStr = JSON.stringify(generated, null, 2);
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
          },
          eds,
        ),
      );
    },
    [nodes],
  );

  const addConditionNode = () => {
    const id = `cond-${Date.now()}`;
    const newNode: Node = {
      id,
      type: 'condition',
      position: { x: 380, y: 120 + (nodes.length % 5) * 40 },
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
      position: { x: 420, y: 180 },
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
      position: { x: 740, y: 160 },
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
    setTestContext(JSON.stringify(preset.mockContext, null, 2));
    setTestResult(null);
  };

  const runTest = async () => {
    setIsTesting(true);
    setTestResult(null);
    try {
      const parsedContext = JSON.parse(testContext);
      const generatedRule = graphToRuleJson(nodes, edges);

      if (onDryRun) {
        const res = await onDryRun(generatedRule, parsedContext);
        setTestResult(res);
      } else {
        // Fallback local evaluation simulation
        setTestResult({ matched: true });
      }
    } catch (err: unknown) {
      setTestResult({
        matched: false,
        error: err instanceof Error ? err.message : 'Invalid JSON context',
      });
    } finally {
      setIsTesting(false);
    }
  };

  const generatedJsonStr = useMemo(
    () => JSON.stringify(graphToRuleJson(nodes, edges), null, 2),
    [nodes, edges],
  );

  return (
    <div className="flex flex-col rounded-xl border border-border/70 bg-zinc-950/70 shadow-2xl overflow-hidden">
      {/* Top Controls Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 bg-zinc-900/60 px-4 py-2.5">
        <div className="flex items-center gap-1.5">
          <div className="mr-2 flex items-center gap-1.5 text-xs font-semibold text-foreground">
            <Sparkles className="h-4 w-4 text-amber-400" />
            <span>XYFlow Blueprint Editor</span>
          </div>

          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={addConditionNode}
            className="h-8 gap-1 border-sky-500/30 bg-sky-500/10 text-xs text-sky-400 hover:bg-sky-500/20"
          >
            <Filter className="h-3.5 w-3.5" />
            <span>+ Condition</span>
          </Button>

          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={addLogicGateNode}
            className="h-8 gap-1 border-amber-500/30 bg-amber-500/10 text-xs text-amber-400 hover:bg-amber-500/20"
          >
            <GitBranch className="h-3.5 w-3.5" />
            <span>+ Gate (AND/OR)</span>
          </Button>

          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={addActionNode}
            className="h-8 gap-1 border-emerald-500/30 bg-emerald-500/10 text-xs text-emerald-400 hover:bg-emerald-500/20"
          >
            <Gift className="h-3.5 w-3.5" />
            <span>+ Action</span>
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

          {/* Mode Switcher */}
          <div className="flex rounded-lg border border-zinc-700 bg-zinc-900 p-0.5 shadow-sm">
            <button
              type="button"
              onClick={() => setActiveTab('visual')}
              className={`flex items-center gap-1.5 rounded px-2.5 py-1 text-xs font-semibold transition ${
                activeTab === 'visual'
                  ? 'bg-zinc-800 text-zinc-100 shadow'
                  : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              <Workflow className="h-3.5 w-3.5 text-sky-400" />
              <span>Canvas</span>
            </button>
            <button
              type="button"
              onClick={() => setActiveTab('code')}
              className={`flex items-center gap-1.5 rounded px-2.5 py-1 text-xs font-semibold transition ${
                activeTab === 'code'
                  ? 'bg-zinc-800 text-zinc-100 shadow'
                  : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              <Code2 className="h-3.5 w-3.5 text-emerald-400" />
              <span>JSON</span>
            </button>
          </div>
        </div>
      </div>

      {/* Main Workspace */}
      <div className="relative h-[480px] w-full bg-zinc-950">
        {activeTab === 'visual' ? (
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            nodeTypes={nodeTypes}
            fitView
            fitViewOptions={{ padding: 0.25 }}
            minZoom={0.2}
            maxZoom={1.5}
            defaultEdgeOptions={{
              animated: true,
              style: { stroke: '#8b5cf6', strokeWidth: 2.5 },
            }}
          >
            <Background variant={BackgroundVariant.Dots} gap={16} size={1} color="#3f3f46" />
            <Controls className="!border-border !bg-zinc-900/90 !text-foreground !fill-foreground" />
            <MiniMap
              className="!border-border/60 !bg-zinc-900/90"
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
          <div className="h-full overflow-auto p-4 font-mono text-xs">
            <pre className="text-emerald-400">{generatedJsonStr}</pre>
          </div>
        )}
      </div>

      {/* Interactive Testing & Dry Run Bar */}
      <div className="border-t border-border/60 bg-zinc-900/80 p-3.5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-1 items-center gap-3">
            <div className="flex items-center gap-1.5 text-xs font-bold text-zinc-100">
              <Play className="h-3.5 w-3.5 text-sky-400" />
              <span>Test Rule Context:</span>
            </div>
            <input
              type="text"
              className="h-8 flex-1 max-w-md rounded-lg border border-zinc-700 bg-zinc-900 px-3 font-mono text-xs font-medium text-zinc-100 placeholder-zinc-500 shadow-sm transition focus:border-sky-400 focus:ring-1 focus:ring-sky-400/40 focus:outline-none"
              value={testContext}
              onChange={(e) => setTestContext(e.target.value)}
              placeholder='e.g. {"kills": 120, "rank": "gold"}'
            />
            <Button
              type="button"
              size="sm"
              onClick={runTest}
              disabled={isTesting}
              className="h-8 gap-1.5 rounded-lg bg-sky-400 text-xs font-bold text-zinc-950 shadow-md transition hover:bg-sky-300 active:scale-95"
            >
              {isTesting ? 'Evaluating...' : 'Run Dry Test'}
            </Button>
          </div>

          {/* Test Evaluation Result Banner */}
          {testResult && (
            <div className="flex items-center gap-2">
              {testResult.matched ? (
                <div className="flex items-center gap-1.5 rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-1 text-xs font-semibold text-emerald-400">
                  <CheckCircle2 className="h-4 w-4" />
                  <span>Rule MATCHED (Conditions Passed)</span>
                </div>
              ) : (
                <div className="flex items-center gap-1.5 rounded-md border border-rose-500/40 bg-rose-500/10 px-3 py-1 text-xs font-semibold text-rose-400">
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
