// The XenolithGraph rule canvas. Same contract as the old React Flow builder —
// `initialJson` in, live rule JSON out through `onChange` — but the editing
// surface is a WebGL node editor with typed pins and in-node widgets, so an
// operator edits a condition where the condition lives instead of in a panel.
//
// The graph shape is deliberately the same one `graph-to-rule.ts` exported from
// the xyflow canvas: trigger → (gate | condition)* → action*, exported through
// the same RuleExpression tree the server evaluates. The editor library is a
// view; the rule document is the contract.

import { useCallback, useEffect, useRef, useState } from 'react';
import { XenolithEditor, type GraphSnapshot, type Node, type NodeId, type PinId, type WidgetSpec } from '@xenolithengine/graph-editor';

interface XenoRuleBuilderProps {
  initialJson?: string;
  onChange: (jsonString: string) => void;
  onDryRun?: (body: unknown, context: unknown) => Promise<{ matched: boolean; error?: string }>;
}

/** The built-in triggers the engine fires itself; everything else is a game
    event the pump mirrors, so the trigger widget offers both. */
const BUILTIN_EVENTS = [
  { label: 'Purchase completed', value: 'purchase' },
  { label: 'Wallet changed', value: 'wallet_change' },
  { label: 'Player level up', value: 'level_up' },
  { label: 'Match ended', value: 'match_end' },
  { label: 'Leaderboard score submitted', value: 'leaderboard_submit' },
  { label: 'Player created', value: 'player_create' },
];

const OPERATORS = ['gte', 'gt', 'eq', 'ne', 'lte', 'lt', 'in', 'contains'].map((op) => ({
  label: op,
  value: op,
}));

/** Column each node type lands in when a rule is loaded — the same left-to-right
    reading order the old canvas auto-layout produced. */
const COLUMN_X: Record<string, number> = {
  trigger: 0,
  gate: 340,
  condition: 340,
  action: 720,
};
const ROW_Y = 190;

interface CardParam {
  name: string;
  type: 'number' | 'string' | 'bool';
  default: string;
}

function readCardParams(node?: { state?: Record<string, unknown> }): CardParam[] {
  const raw = node?.state?.params;
  return Array.isArray(raw) ? (raw as CardParam[]) : [];
}

/** Rebuild a card's pins + widgets from its params: exec pins first, then one
    named data-out pin per param, then an inline default editor per param. */
function rebuildParamPins(editor: XenolithEditor, nodeId: NodeId) {
  const node = editor.getNode(nodeId);
  if (!node) return;
  const execIn = node.pins.find((p) => p.kind === 'exec' && p.direction === 'in');
  const execOut = node.pins.find((p) => p.kind === 'exec' && p.direction === 'out');
  const params = readCardParams(node);
  const pins = [];
  if (execIn) pins.push({ ...execIn });
  if (execOut) pins.push({ ...execOut });
  for (const p of params) {
    // setNodePins keeps an existing pin's id when its label matches; a fresh
    // label mints a fresh id, and edges to removed pins are pruned.
    pins.push({ id: `pin-${nodeId}-${p.name}` as PinId, kind: 'data' as never, direction: 'out' as never, type: p.type, label: p.name, multiple: false });
  }
  editor.setNodePins(nodeId, pins);

  const base = BASE_WIDGETS[node.type] ?? [];
  const widgets = [...base];
  const state: Record<string, unknown> = {};
  for (const p of params) {
    // Name, type and default — all three editable on the card and in the
    // sidebar. The pin's label is the name; the widget keys carry it so a
    // rename rekeys the trio.
    widgets.push({ type: 'text', id: `pname_${p.name}`, label: 'name', key: `pname_${p.name}`, placeholder: 'var_name', showInSidebar: true, freeFloating: true, inNodeBody: false });
    widgets.push({
      type: 'combo',
      id: `ptype_${p.name}`,
      label: 'type',
      key: `ptype_${p.name}`,
      showInSidebar: true,
      freeFloating: true,
      inNodeBody: false,
      values: [
        { label: 'number', value: 'number' },
        { label: 'string', value: 'string' },
        { label: 'bool', value: 'bool' },
      ],
    });
    widgets.push({ type: 'text', id: `pdefault_${p.name}`, label: `${p.name} =`, key: `pdefault_${p.name}`, placeholder: p.type, showInSidebar: true, freeFloating: true, inNodeBody: false });
    state[`pname_${p.name}`] = p.name;
    state[`ptype_${p.name}`] = p.type;
    state[`pdefault_${p.name}`] = p.default;
  }
  editor.setNodeWidgets(nodeId, widgets);
  editor.setNodeState(nodeId, state);
}

/** Mint the next param on a card: `param_N` typed number, default 0. */
function addCardParam(editor: XenolithEditor, nodeId: NodeId) {
  const node = editor.getNode(nodeId);
  if (!node) return;
  const params = readCardParams(node);
  let index = params.length + 1;
  while (params.some((p) => p.name === `param_${index}`)) index += 1;
  const next = [...params, { name: `param_${index}`, type: 'number' as const, default: '0' }];
  editor.setNodeState(nodeId, { params: next });
  rebuildParamPins(editor, nodeId);
}

/** Node schemas live in the editor registry, which is per-editor-instance, so
    every new editor registers its own copies. */

/** Each card's own fields, minus the dynamic param defaults which
    rebuildParamPins appends. Shared by schema registration and pin rebuilds so
    a param edit never wipes the card's base widgets. */
const BASE_WIDGETS: Record<string, WidgetSpec[]> = {};
function regBase(type: string, widgets: WidgetSpec[]) {
  BASE_WIDGETS[type] = widgets;
  return widgets;
}

function registerSchemas(editor: XenolithEditor) {
  const execIn = { kind: 'exec' as const, direction: 'in' as const, type: 'exec', label: 'In' };
  const execOut = { kind: 'exec' as const, direction: 'out' as const, type: 'exec', label: 'Out' };

  editor.registry.register({
    type: 'rule.trigger',
    title: 'Trigger',
    category: 'Events',
    description: 'The event that fires this rule',
    glyph: { icon: 'zap' },
    pins: [execOut],
    widgets: regBase('rule.trigger', [
      {
        type: 'combo',
        id: 'event-kind',
        label: 'Event',
        key: 'eventKind',
        showInSidebar: true,
        freeFloating: true,
        inNodeBody: false,
        values: [
          ...BUILTIN_EVENTS,
          { label: 'Custom game event…', value: '__custom__' },
        ],
      },
      {
        type: 'text',
        id: 'event-name',
        label: 'Event name',
        key: 'event',
        showInSidebar: true,
        freeFloating: true,
        inNodeBody: false,
        placeholder: 'boss_died, combo_3x',
        // The custom field only means something when the operator picked
        // "Custom game event…" — a built-in needs no name.
        displayOptions: { show: (state) => state.eventKind === '__custom__' },
      },
    ]),
  });

  editor.registry.register({
    type: 'rule.condition',
    title: 'Condition',
    category: 'Functions',
    description: 'Compare a stat field against a value or variable',
    glyph: { icon: 'filter' },
    // "Var" data-in: a Parameter node wires into it and the comparison reads
    // that parameter instead of a typed literal.
    pins: [execIn, execOut, { kind: 'data', direction: 'in', type: 'any', label: 'Var' }],
    widgets: regBase('rule.condition', [
      { type: 'text', id: 'field', label: 'Field', key: 'field', placeholder: 'kills, rank, level', showInSidebar: true, freeFloating: true, inNodeBody: false },
      { type: 'combo', id: 'op', label: 'Operator', key: 'op', values: OPERATORS, showInSidebar: true, freeFloating: true, inNodeBody: false },
      {
        type: 'combo',
        id: 'value-kind',
        label: 'Compare to',
        key: 'valueKind',
        showInSidebar: true,
        freeFloating: true,
        inNodeBody: false,
        values: [
          { label: 'Value', value: 'literal' },
          { label: 'Variable', value: 'var' },
        ],
      },
      {
        type: 'text',
        id: 'value',
        label: 'Value',
        key: 'value',
        showInSidebar: true,
        freeFloating: true,
        inNodeBody: false,
        placeholder: '10',
        displayOptions: { show: (state) => (state.valueKind ?? 'literal') === 'literal' },
      },
      {
        type: 'text',
        id: 'value-var',
        label: 'Variable',
        key: 'valueVar',
        showInSidebar: true,
        freeFloating: true,
        inNodeBody: false,
        placeholder: 'min_kills',
        displayOptions: { show: (state) => (state.valueKind ?? 'literal') === 'var' },
      },
    ]),
  });

  editor.registry.register({
    type: 'rule.gate',
    title: 'Logic Gate',
    category: 'Flow Control',
    description: 'AND / OR / NOT over the nodes it feeds',
    glyph: { icon: 'git-branch' },
    pins: [execIn, execOut],
    widgets: regBase('rule.gate', [
      {
        type: 'combo',
        id: 'gate',
        label: 'Gate',
        key: 'gate',
        showInSidebar: true,
        freeFloating: true,
        inNodeBody: false,
        values: [
          { label: 'AND (all)', value: 'all' },
          { label: 'OR (any)', value: 'any' },
          { label: 'NOT', value: 'not' },
        ],
      },
    ]),
  });

  editor.registry.register({
    type: 'rule.action',
    title: 'Action',
    category: 'Actions',
    description: 'What a matching rule does',
    glyph: { icon: 'gift' },
    pins: [execIn],
    widgets: regBase('rule.action', [
      {
        type: 'combo',
        id: 'action-type',
        label: 'Type',
        key: 'actionType',
        showInSidebar: true,
        freeFloating: true,
        inNodeBody: false,
        values: [
          { label: 'Coins (currency)', value: 'coins' },
          { label: 'Skin / item', value: 'skin' },
          { label: 'Badge', value: 'badge' },
          { label: 'Notify', value: 'event' },
        ],
      },
      { type: 'text', id: 'payload', label: 'Target', key: 'payload', placeholder: 'coins / item id / badge / subject', showInSidebar: true, freeFloating: true, inNodeBody: false },
      { type: 'number', id: 'amount', label: 'Amount', key: 'amount', min: 0, step: 1, showInSidebar: true, freeFloating: true, inNodeBody: false },
    ]),
  });
}

/** The id of a condition's data-in Var pin, if it has one. */
function varPinId(node: Readonly<Node>): string | undefined {
  return node.pins.find((p) => p.kind === 'data' && p.direction === 'in' && p.label === 'Var')?.id;
}

/**
 * The snapshot → rule tree walk, with the same semantics `graphToRuleJson` had
 * on the xyflow canvas: a gate's children are exactly the nodes its exec-out
 * feeds; the root gate is the one the trigger points at; an unconnected node
 * never smuggles itself into the document.
 */
export function snapshotToRuleDocument(
  snap: GraphSnapshot,
): Record<string, unknown> {
  const byId = new Map(snap.nodes.map((n) => [n.id, n]));
  const trigger = snap.nodes.find((n) => n.type === 'rule.trigger');
  const gates = snap.nodes.filter((n) => n.type === 'rule.gate');

  const childrenOf = (id: string): GraphSnapshot['nodes'] =>
    snap.edges
      .filter((e) => e.from.node === id)
      .map((e) => byId.get(e.to.node))
      .filter((n): n is GraphSnapshot['nodes'][number] => Boolean(n));

  const toComparison = (node: GraphSnapshot['nodes'][number]): Record<string, unknown> => {
    const state = node.state as Record<string, unknown>;
    const comparison: Record<string, unknown> = {
      field: String(state.field || 'kills'),
      op: String(state.op || 'gte'),
    };
    if ((state.valueKind ?? 'literal') === 'var' && state.valueVar) {
      comparison.var = String(state.valueVar);
    } else {
      const raw = state.value ?? 0;
      // The text widget hands the value back as a string; a rule that compares
      // numbers needs the number back, so a cleanly numeric string parses.
      comparison.value =
        typeof raw === 'string' && raw.trim() !== '' && !Number.isNaN(Number(raw))
          ? Number(raw)
          : raw;
    }
    return comparison;
  };

  const buildFromGate = (gate: GraphSnapshot['nodes'][number], seen: Set<string>): Record<string, unknown> => {
    if (seen.has(gate.id)) return { value: true };
    seen.add(gate.id);
    const gateType = String((gate.state as Record<string, unknown>).gate || 'all');
    const exprs = childrenOf(gate.id)
      .filter((child) => child.type === 'rule.condition' || child.type === 'rule.gate')
      .map((child) =>
        child.type === 'rule.condition'
          ? toComparison(child)
          : buildFromGate(child, seen),
      );
    if (gateType === 'not') return { not: exprs[0] ?? { value: true } };
    if (gateType === 'any') return { any: exprs.length > 0 ? exprs : [{ value: true }] };
    return { all: exprs.length > 0 ? exprs : [{ value: true }] };
  };

  // Reachability from the trigger decides which actions belong to the rule —
  // an action box nothing feeds is a draft, not a reward.
  const reachable = new Set<string>();
  if (trigger) {
    const walk = (id: string) => {
      for (const child of childrenOf(id)) {
        if (!reachable.has(child.id)) {
          reachable.add(child.id);
          walk(child.id);
        }
      }
    };
    walk(trigger.id);
  }

  const document: Record<string, unknown> = {};

  // Trigger: a built-in selection or a custom event name.
  if (trigger) {
    const state = trigger.state as Record<string, unknown>;
    const event = state.eventKind === '__custom__' ? String(state.event ?? '') : String(state.eventKind ?? '');
    if (event) document.trigger = event;
  }

  // Variables: every var-mode condition contributes a name with a 0 default —
  // the JSON editor is where an operator tunes the actual number.
  const variables: Record<string, unknown> = {};
  for (const node of snap.nodes) {
    if (node.type !== 'rule.condition') continue;
    const state = node.state as Record<string, unknown>;
    if ((state.valueKind ?? 'literal') === 'var' && state.valueVar) {
      const name = String(state.valueVar);
      variables[name] = variables[name] ?? 0;
    }
  }
  // Card params: every node's own `[+] name = default` list — the card's pins
  // are the source of truth for its variables.
  const paramTypes: Record<string, string> = {};
  for (const node of snap.nodes) {
    for (const param of readCardParams(node)) {
      if (!param.name) continue;
      const raw = String(node.state[`pdefault_${param.name}`] ?? param.default ?? '').trim();
      let value: unknown = raw === '' ? 0 : raw;
      if (param.type === 'number') value = raw === '' || Number.isNaN(Number(raw)) ? 0 : Number(raw);
      else if (param.type === 'bool') value = raw === 'true';
      variables[param.name] = value;
      paramTypes[param.name] = param.type;
    }
  }
  if (Object.keys(variables).length > 0) document.variables = variables;
  if (Object.keys(paramTypes).length > 0) document.paramTypes = paramTypes;

  // The expression tree.
  const conditions = snap.nodes.filter((n) => n.type === 'rule.condition');
  if (gates.length > 0) {
    const fedByGate = new Set(
      snap.edges.filter((e) => gates.some((g) => g.id === e.from.node)).map((e) => e.to.node),
    );
    const rootGates = gates.filter((g) => !fedByGate.has(g.id));
    const triggerTarget = trigger ? childrenOf(trigger.id).find((n) => n.type === 'rule.gate') : undefined;
    const root =
      (triggerTarget && rootGates.find((g) => g.id === triggerTarget.id)) ?? rootGates[0] ?? gates[0];
    const rootExpr = buildFromGate(root, new Set());
    // buildFromGate returns the gate's own shape, which becomes the body's
    // top level directly — all/any/not all live at the same level as trigger.
    for (const key of ['all', 'any', 'not'] as const) {
      if (key in rootExpr) document[key] = rootExpr[key];
    }
  } else if (conditions.length > 1) {
    document.all = conditions.map(toComparison);
  } else if (conditions.length === 1) {
    Object.assign(document, toComparison(conditions[0]));
  } else {
    document.value = true;
  }

  // Actions, in graph order.
  const actions: Record<string, unknown>[] = [];
  for (const node of snap.nodes) {
    if (node.type !== 'rule.action') continue;
    if (reachable.size > 0 && !reachable.has(node.id)) continue;
    const state = node.state as Record<string, unknown>;
    const payload = String(state.payload ?? '').trim();
    const amount = Number(state.amount ?? 0);
    switch (state.actionType) {
      case 'coins':
        if (payload) actions.push({ type: 'currency', currency: payload, amount });
        break;
      case 'skin':
        if (payload) actions.push({ type: 'item', item_id: payload, quantity: amount || 1 });
        break;
      case 'badge':
        if (payload) actions.push({ type: 'badge', badge: payload });
        break;
      case 'event':
        if (payload) actions.push({ type: 'notify', subject: payload, body: '' });
        break;
    }
  }
  if (actions.length > 0) document.actions = actions;

  return document;
}

/** Row position per node index when spawning from the toolbar. */
function spawnPosition(type: string, index: number) {
  const x = COLUMN_X[type] ?? 340;
  return { x: x + (index % 3) * 40, y: 60 + index * ROW_Y };
}

export function XenoRuleBuilder({ initialJson, onChange, onDryRun }: XenoRuleBuilderProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<XenolithEditor | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const storedVariables = useRef<Record<string, unknown>>({});
  // The React mirror of storedVariables — the panel edits the ref (the export
  // source) and this state (the rows), so both stay one object apart.
  const [vars, setVars] = useState<{ name: string; value: unknown }[]>([]);
  const syncVars = useCallback(() => {
    setVars(
      Object.entries(storedVariables.current).map(([name, value]) => ({ name, value })),
    );
  }, []);
  const [dryContext, setDryContext] = useState('{ "kills": 120, "rank": "gold" }');
  const [dryResult, setDryResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const spawnCount = useRef(0);

  /** Export the live graph through the parent's onChange. Debounced a beat so
      a combo of edits lands as one JSON write. */
  const exportTimer = useRef<number | null>(null);
  const scheduleExport = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) return;
    if (exportTimer.current !== null) window.clearTimeout(exportTimer.current);
    exportTimer.current = window.setTimeout(() => {
      const doc = snapshotToRuleDocument(editor.graphSnapshot());
      // Stored variable defaults survive every export: the graph knows names,
      // the JSON editor owns the tuned values.
      const variables = { ...storedVariables.current, ...((doc.variables as Record<string, unknown>) ?? {}) };
      if (Object.keys(variables).length > 0) doc.variables = variables;
      else delete doc.variables;
      onChangeRef.current(JSON.stringify(doc, null, 2));
      syncVars();
    }, 150);
  }, [syncVars]);

  /** Parameter panel writes. Every edit lands in storedVariables (the export
      source) and re-exports, so the rule body and the panel never drift. */
  const setVarValue = useCallback((name: string, raw: string) => {
    if (!(name in storedVariables.current)) return;
    const parsed = raw.trim() === '' || Number.isNaN(Number(raw)) ? raw : Number(raw);
    storedVariables.current[name] = parsed;
    syncVars();
    scheduleExport();
  }, [syncVars, scheduleExport]);

  const renameVar = useCallback((oldName: string, newName: string) => {
    const clean = newName.toLowerCase().replace(/[^a-z0-9_]/g, '_').replace(/^_+/, '');
    if (!clean || clean === oldName || clean in storedVariables.current) return;
    const rebuilt: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(storedVariables.current)) {
      rebuilt[key === oldName ? clean : key] = value;
    }
    storedVariables.current = rebuilt;
    // Conditions referencing the old name follow the rename.
    const editor = editorRef.current;
    if (editor) {
      for (const node of editor.graphNodes()) {
        if (node.type !== 'rule.condition') continue;
        if (node.state.valueKind === 'var' && node.state.valueVar === oldName) {
          editor.setNodeState(node.id, { valueVar: clean });
        }
      }
    }
    syncVars();
    scheduleExport();
  }, [syncVars, scheduleExport]);

  const addVar = useCallback(() => {
    let index = Object.keys(storedVariables.current).length + 1;
    while (`param_${index}` in storedVariables.current) index += 1;
    storedVariables.current[`param_${index}`] = 0;
    syncVars();
    scheduleExport();
  }, [syncVars, scheduleExport]);

  const removeVar = useCallback((name: string) => {
    delete storedVariables.current[name];
    syncVars();
    scheduleExport();
  }, [syncVars, scheduleExport]);

  /** Load a stored rule JSON onto the canvas. */
  const loadRuleJson = useCallback((json: unknown, editor: XenolithEditor) => {
    const parsed = (json ?? {}) as Record<string, unknown>;
    storedVariables.current =
      parsed.variables && typeof parsed.variables === 'object' && !Array.isArray(parsed.variables)
        ? (parsed.variables as Record<string, unknown>)
        : {};

    editor.clear();
    spawnCount.current = 0;
    const rows: { id: string; type: string; y: number }[] = [];
    const add = (type: string, state: Record<string, unknown>): NodeId => {
      const node = editor.insertNode(type, spawnPosition(type, rows.length));
      if (!node) throw new Error(`the ${type} node type did not instantiate`);
      editor.setNodeState(node.id, state);
      rows.push({ id: node.id, type, y: rows.length });
      return node.id;
    };

    // Trigger first — the rule always reads from it.
    const triggerState: Record<string, unknown> = { eventKind: 'purchase' };
    const triggerEvent = typeof parsed.trigger === 'string' ? parsed.trigger : '';
    const builtin = BUILTIN_EVENTS.find((e) => e.value === triggerEvent);
    if (builtin) {
      triggerState.eventKind = builtin.value;
    } else if (triggerEvent) {
      triggerState.eventKind = '__custom__';
      triggerState.event = triggerEvent;
    }
    const triggerId = add('rule.trigger', triggerState);

    let leafIndex = 0;
    const lastConditionIds: NodeId[] = [];

    const addExpr = (expr: unknown, parentId: NodeId): void => {
      if (!expr || typeof expr !== 'object') return;
      const obj = expr as Record<string, unknown>;
      if (Array.isArray(obj.all) || Array.isArray(obj.any)) {
        const gateId = add('rule.gate', { gate: Array.isArray(obj.all) ? 'all' : 'any' });
        editor.connect({ source: parentId, target: gateId });
        for (const child of ((obj.all ?? obj.any) as unknown[])) addExpr(child, gateId);
        return;
      }
      if ('not' in obj) {
        const gateId = add('rule.gate', { gate: 'not' });
        editor.connect({ source: parentId, target: gateId });
        addExpr(obj.not, gateId);
        return;
      }
      if ('field' in obj) {
        const isVar = typeof obj.var === 'string' && obj.var !== '';
        const condId = add('rule.condition', {
          field: String(obj.field ?? 'kills'),
          op: String(obj.op ?? 'gte'),
          valueKind: isVar ? 'var' : 'literal',
          value: isVar ? '' : (obj.value ?? 0),
          valueVar: isVar ? String(obj.var) : '',
        });
        editor.connect({ source: parentId, target: condId });
        lastConditionIds.push(condId);
        leafIndex += 1;
      }
    };

    // The stored body carries the tree at top level; the trigger is metadata.
    const { trigger: _t, variables: _v, actions: _a, ...tree } = parsed as Record<string, unknown>;
    addExpr(Object.keys(tree).length > 0 ? tree : { value: true }, triggerId);

    // One action box per stored action, fed by the last condition so the
    // reachability walk includes it.
    const storedActions = Array.isArray(parsed.actions) ? (parsed.actions as Record<string, unknown>[]) : [];
    for (const action of storedActions) {
      const state: Record<string, unknown> = { actionType: 'coins', payload: '', amount: 0 };
      if (action.type === 'currency') {
        state.actionType = 'coins';
        state.payload = action.currency ?? '';
        state.amount = action.amount ?? 0;
      } else if (action.type === 'item') {
        state.actionType = 'skin';
        state.payload = action.item_id ?? '';
        state.amount = action.quantity ?? 1;
      } else if (action.type === 'badge') {
        state.actionType = 'badge';
        state.payload = action.badge ?? '';
      } else if (action.type === 'notify') {
        state.actionType = 'event';
        state.payload = action.subject ?? '';
      }
      const actionId = add('rule.action', state);
      const feedFrom = lastConditionIds[lastConditionIds.length - 1] ?? triggerId;
      editor.connect({ source: feedFrom, target: actionId });
    }

    // Saved variables come back as the trigger card's own param pins —
    // Unreal-style, the card carries what it offers to map.
    const storedTypes = (parsed.paramTypes ?? {}) as Record<string, string>;
    const cardParams = Object.entries(storedVariables.current).map(([name, value]) => ({
      name,
      type: (storedTypes[name] ?? 'number') as 'number' | 'string' | 'bool',
      default: String(value ?? ''),
    }));
    if (cardParams.length > 0) {
      editor.setNodeState(triggerId, { params: cardParams });
      rebuildParamPins(editor, triggerId);
    }

    editor.autoLayout({ direction: 'LR', fit: false });
    editor.fitView({ padding: 40, maxZoom: 1 });
    scheduleExport();
  }, [scheduleExport, syncVars]);

  /** Editor lifecycle. `initialJson` changes from the parent are applied, not
      re-derived: the parent holds the JSON we exported. */
  useEffect(() => {
    let disposed = false;
    let editor: XenolithEditor | null = null;
    const unsubs: (() => void)[] = [];

    async function boot() {
      if (!hostRef.current) return;
      editor = await XenolithEditor.init(hostRef.current, {
        minimap: true,
        controls: true,
        background: '#0b0b12',
      });
      if (disposed) {
        editor.destroy();
        return;
      }
      editorRef.current = editor;
      // Non-null alias: TS cannot narrow across the closures below.
      const ed: XenolithEditor = editor;
      registerSchemas(ed);
      // UE Blueprint wire colors: typed data wires tint themselves, so a
      // number param runs green and a bool runs red — mismatches readable
      // at a glance.
      ed.types.register({ id: 'number', color: '#9ef0a0' });
      ed.types.register({ id: 'string', color: '#f39bf9' });
      ed.types.register({ id: 'bool', color: '#e07a7a' });
      ed.types.register({ id: 'any', color: '#c8c8c8' });
      // UE Blueprint node tints: events red, flow control gray, functions
      // blue, actions green — the graph reads by color before by title.
      ed.setCategoryPalette({
        Events: '#c0392b',
        'Flow Control': '#95a5a6',
        Functions: '#3b82f6',
        Actions: '#2ecc71',
        Variables: '#2ecc71',
      });
      // A "zap" glyph ships with the built-in Feather set; register ours the
      // same way so every schema's glyph resolves.
      ed.icons.register('zap', '<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>');
      ed.icons.register('filter', '<polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3"/>');
      ed.icons.register('git-branch', '<line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>');
      ed.icons.register('gift', '<polyline points="20 12 20 22 4 22 4 12"/><rect x="2" y="7" width="20" height="5"/><line x1="12" y1="22" x2="12" y2="7"/><path d="M12 7H7.5a2.5 2.5 0 0 1 0-5C11 2 12 7 12 7z"/><path d="M12 7h4.5a2.5 2.5 0 0 0 0-5C13 2 12 7 12 7z"/>');

      // The docked palette: every registered node type listed on the left,
      // dragged onto the canvas to spawn. On a narrow window it eats the
      // canvas, so wide screens only.
      if (hostRef.current.clientWidth >= 1100) {
        ed.setPaletteSidebar(true);
      }

      // Right-click on empty canvas builds where the operator pointed: one
      // entry per rule node, placed at the click, no trip to the toolbar.
      const canvasAdd = (label: string, type: string) =>
        ed.contextMenu.registerCanvasItem({
          label,
          onSelect: (target) => {
            editorRef.current?.insertNode(type, target.worldPosition);
          },
        });
      unsubs.push(canvasAdd('Add Condition here', 'rule.condition'));
      unsubs.push(canvasAdd('Add Gate here', 'rule.gate'));
      unsubs.push(canvasAdd('Add Action here', 'rule.action'));
      unsubs.push(
        ed.contextMenu.registerCanvasItem({
          label: 'Auto-arrange',
          hint: 'Layered left-to-right layout',
          onSelect: () => editorRef.current?.autoLayout({ direction: 'LR', fit: true }),
        }),
      );

      // Right-click on a node: the next node spawns already wired to this
      // one, just right of it, and its editor opens. Plus Blueprint-style
      // params: the card mints its own named pins.
      const afterAdd = (label: string, kind: string) =>
        ed.contextMenu.registerNodeItem({
          label,
          onSelect: ({ nodeId }) => {
            const source = ed.getNode(nodeId);
            const at = source
              ? { x: source.position.x + 340, y: source.position.y - 60 }
              : { x: 400, y: 100 };
            const next = ed.insertNode(`rule.${kind}`, at);
            if (!next) return;
            ed.connect({ source: nodeId, target: next.id });
            ed.openSidebar(next.id);
          },
        });
      unsubs.push(afterAdd('Add Condition after', 'condition'));
      unsubs.push(afterAdd('Add Gate after', 'gate'));
      unsubs.push(afterAdd('Add Action after', 'action'));
      unsubs.push(
        ed.contextMenu.registerNodeItem({
          label: 'Add Param  [+] name = default',
          onSelect: ({ nodeId }) => {
            addCardParam(ed, nodeId);
          },
        }),
      );
      unsubs.push(
        ed.contextMenu.registerNodeItem({
          label: 'Remove last Param',
          when: (t) => readCardParams(ed.getNode(t.nodeId)).length > 0,
          onSelect: ({ nodeId }) => {
            const node = ed.getNode(nodeId);
            if (!node) return;
            const params = [...readCardParams(node)];
            params.pop();
            ed.setNodeState(nodeId, { params });
            rebuildParamPins(ed, nodeId);
          },
        }),
      );
      unsubs.push(
        ed.contextMenu.registerNodeItem({
          label: 'Arrange from here',
          hint: 'Layered left-to-right layout',
          onSelect: () => ed.autoLayout({ direction: 'LR', fit: true }),
        }),
      );

      // A data wire from any card's param pin into a condition's Var flips
      // that comparison to variable mode with the pin's name — the wire IS
      // the variable binding.
      unsubs.push(
        ed.on('edge:connected', ({ edge }) => {
          const target = ed.getNode(edge.to.node);
          if (!target || target.type !== 'rule.condition') return;
          if (edge.to.pin !== varPinId(target)) return;
          const source = ed.getNode(edge.from.node);
          if (!source) return;
          const pin = source.pins.find((p) => p.id === edge.from.pin);
          const name = pin?.label?.trim();
          if (name) ed.setNodeState(edge.to.node, { valueKind: 'var', valueVar: name });
        }),
      );
      // Editing a param's name / type / default updates the card's param
      // list — the export carries whatever the card now shows. A rename also
      // re-binds every condition in the graph that referenced the old name,
      // wired or not, so nothing keeps a stale variable.
      unsubs.push(
        ed.on('widget:changed', ({ nodeId, widgetId, value }) => {
          const kind = widgetId.startsWith('pname_')
            ? 'name'
            : widgetId.startsWith('ptype_')
              ? 'type'
              : widgetId.startsWith('pdefault_')
                ? 'default'
                : '';
          if (!kind) return;
          const node = ed.getNode(nodeId);
          if (!node) return;
          const params = readCardParams(node);
          const oldKey = widgetId.slice(kind.length + 1);
          const param = params.find((p) => p.name === oldKey);
          if (!param) return;
          const trimmed = String(value ?? '').trim();
          if (kind === 'name') {
            const renamed = trimmed.toLowerCase().replace(/[^a-z0-9_]/g, '_').replace(/^_+/, '');
            if (!renamed || renamed === oldKey || params.some((p) => p.name === renamed)) return;
            param.name = renamed;
            // Re-key the card's own state for this param trio.
            const patch: Record<string, unknown> = {
              params,
              [`pname_${renamed}`]: renamed,
              [`pdefault_${renamed}`]: param.default,
              [`ptype_${renamed}`]: param.type,
            };
            ed.setNodeState(nodeId, patch);
            // Every condition that read the old name now reads the new one.
            for (const other of ed.graphNodes()) {
              if (other.type !== 'rule.condition') continue;
              if (other.state.valueVar === oldKey) {
                ed.setNodeState(other.id, { valueVar: renamed });
              }
            }
          } else if (kind === 'type') {
            const type = trimmed === 'string' || trimmed === 'bool' ? trimmed : 'number';
            param.type = type;
            ed.setNodeState(nodeId, { params });
          } else {
            param.default = trimmed;
            ed.setNodeState(nodeId, { params });
          }
          rebuildParamPins(ed, nodeId);
        }),
      );

      // One event covers every mutation — widget, node, edge — so the rule
      // document stays in sync no matter how the operator edited the graph.
      unsubs.push(ed.on('graph:changed', scheduleExport));


      // Ek click = sidebar editor. Card par koi edit widget nahi — sirf
      // title aur pins; sab fields sidebar me milte hain, including param
      // name/type/default, jo rename par wired conditions bhi update karta hai.
      unsubs.push(
        ed.on('node:click', ({ nodeId }) => {
          ed.openSidebar(nodeId);
        }),
      );

      loadRuleJson(initialJson ? safeParse(initialJson) : {}, ed);
    }
    void boot();

    return () => {
      disposed = true;
      for (const unsub of unsubs) unsub();
      if (exportTimer.current !== null) window.clearTimeout(exportTimer.current);
      editorRef.current?.destroy();
      editorRef.current = null;
    };
    // initialJson is deliberately read once: the parent's later writes are our
    // own exports echoed back, and reloading those would fight the operator.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const addNode = useCallback((type: string) => {
    const editor = editorRef.current;
    if (!editor) return;
    // Toolbar spawns land where the operator is looking, not in a corner:
    // the world point under the viewport centre.
    const viewport = editor.viewport;
    const centre = editor.screenToWorld({
      x: window.innerWidth / 2,
      y: window.innerHeight / 2,
    });
    void viewport;
    editor.insertNode(type, centre);
  }, []);

  const arrange = useCallback(() => {
    editorRef.current?.autoLayout({ direction: 'LR', fit: true });
  }, []);

  const runDryTest = useCallback(async () => {
    const editor = editorRef.current;
    if (!editor || !onDryRun) return;
    setTesting(true);
    setDryResult(null);
    try {
      const body = snapshotToRuleDocument(editor.graphSnapshot());
      let context: unknown;
      try {
        context = JSON.parse(dryContext);
      } catch {
        setDryResult('Test context is not valid JSON');
        return;
      }
      const result = await onDryRun(body, context);
      setDryResult(result.error ? `Error: ${result.error}` : result.matched ? '✓ Rule matched' : '✗ Rule did not match');
    } finally {
      setTesting(false);
    }
  }, [dryContext, onDryRun]);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => addNode('rule.condition')}
          className="rounded-md border border-sky-500/40 bg-sky-500/10 px-2.5 py-1 text-xs font-medium text-sky-600 transition hover:bg-sky-500/20 dark:text-sky-300"
        >
          + Condition
        </button>
        <button
          type="button"
          onClick={() => addNode('rule.gate')}
          className="rounded-md border border-amber-500/40 bg-amber-500/10 px-2.5 py-1 text-xs font-medium text-amber-600 transition hover:bg-amber-500/20 dark:text-amber-300"
        >
          + Gate (AND/OR)
        </button>
        <button
          type="button"
          onClick={() => addNode('rule.action')}
          className="rounded-md border border-emerald-500/40 bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-600 transition hover:bg-emerald-500/20 dark:text-emerald-300"
        >
          + Action
        </button>
        <button
          type="button"
          onClick={arrange}
          className="rounded-md border border-border bg-muted/40 px-2.5 py-1 text-xs font-medium text-foreground transition hover:bg-muted"
        >
          Arrange
        </button>
        <span className="text-xs text-muted-foreground">
          Drag from a node&apos;s Out pin to the next node&apos;s In pin · right-click for the node menu
        </span>
      </div>

      <div
        ref={hostRef}
        className="h-[420px] lg:h-[560px] w-full overflow-hidden rounded-lg border border-border"
      />

      {/* Parameters: the named values the rule's variable comparisons read.
          These land in the rule body as `variables` and travel with the draft
          to the database — tuning one is an edit here, not a republish. */}
      <div className="rounded-lg border border-violet-500/30 bg-violet-500/5 p-3">
        <div className="flex items-center justify-between">
          <span className="text-xs font-bold uppercase tracking-wider text-violet-600 dark:text-violet-300">
            Parameters (Variables)
          </span>
          <button
            type="button"
            onClick={addVar}
            className="rounded-md border border-violet-500/40 bg-violet-500/10 px-2.5 py-1 text-xs font-medium text-violet-600 transition hover:bg-violet-500/20 dark:text-violet-300"
          >
            + Parameter
          </button>
        </div>
        {vars.length === 0 ? (
          <p className="mt-2 text-xs text-muted-foreground">
            None yet. Set a condition&apos;s compare-to to &ldquo;Variable&rdquo; on the canvas, or add one here —
            it lands in the rule body as <span className="font-mono">variables</span>.
          </p>
        ) : (
          <div className="mt-2 space-y-1.5">
            {vars.map((v) => (
              <div key={v.name} className="flex items-center gap-2">
                <input
                  type="text"
                  className="h-8 w-44 rounded-md border border-input bg-background px-2.5 text-xs font-mono text-foreground"
                  value={v.name}
                  onChange={(e) => renameVar(v.name, e.target.value)}
                />
                <span className="text-xs text-muted-foreground">=</span>
                <input
                  type="text"
                  className="h-8 w-28 rounded-md border border-input bg-background px-2.5 text-xs font-mono text-foreground"
                  value={String(v.value ?? '')}
                  onChange={(e) => setVarValue(v.name, e.target.value)}
                />
                <button
                  type="button"
                  onClick={() => removeVar(v.name)}
                  className="rounded-md border border-destructive/40 px-2 py-1 text-xs text-destructive transition hover:bg-destructive/10"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <input
          type="text"
          className="h-8 w-full max-w-md rounded-md border border-input bg-background px-2.5 text-xs font-mono text-foreground shadow-xs"
          value={dryContext}
          onChange={(e) => setDryContext(e.target.value)}
          placeholder='Test rule context — e.g. {"kills": 120, "rank": "gold"}'
        />
        <button
          type="button"
          onClick={runDryTest}
          disabled={testing || !onDryRun}
          className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
        >
          {testing ? 'Testing…' : 'Run Dry Test'}
        </button>
        {dryResult && (
          <span className={`text-xs font-medium ${dryResult.startsWith('✓') ? 'text-emerald-500' : 'text-amber-500'}`}>
            {dryResult}
          </span>
        )}
      </div>
    </div>
  );
}

function safeParse(json: string | undefined): unknown {
  if (!json) return {};
  try {
    return JSON.parse(json);
  } catch {
    return {};
  }
}
