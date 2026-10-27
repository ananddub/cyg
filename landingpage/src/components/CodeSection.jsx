import { useMemo, useState } from 'react'
import hljs from 'highlight.js/lib/core'
import typescript from 'highlight.js/lib/languages/typescript'
import javascript from 'highlight.js/lib/languages/javascript'
import Reveal from './Reveal.jsx'
import SectionHead from './SectionHead.jsx'

hljs.registerLanguage('typescript', typescript)
hljs.registerLanguage('javascript', javascript)

const TABS = [
  {
    id: 'server',
    label: 'src/room.ts',
    tag: 'server',
    lang: 'typescript',
    code: `export class Arena extends Room {
    onCreate(max: i32): void {
        this.setState(this.s);
        Rapier2D.init(0.0, 0.0);            // zero-gravity arena
        this.spawnCoin();
    }

    onInputPacket(id: i64, input: InputReader): void {
        const p = this.s.players.get(id);
        if (isHeld(input.readU8(), Key.W)) {
            Rapier2D.applyImpulse(p.body, 0.0, -MOVE_FORCE);
        }
    }

    onTick(dt: f64): i64 {
        Rapier2D.step(dt);
        this.syncBodies();                  // physics → schema
        return this.commitState();          // deltas go out on both lanes
    }
}`,
  },
  {
    id: 'client',
    label: 'client.js',
    tag: 'browser',
    lang: 'javascript',
    code: `// No codegen in the browser — the descriptor arrives at join.
const client = new CYG.Client(location.origin);
const room = await client.joinOrCreate("game_room");

room.onStateChange((state) => {
    render(state.players.get(room.playerId).x);
});

const callbacks = CYG.getStateCallbacks(room);
callbacks.onAdd("players", (player) => {
    player.listen("score", (v) => hud(v));   // reactive field listener
});

// request/response RPC over the reliable lane
const reply = await room.request("login", { user });`,
  },
  {
    id: 'schema',
    label: 'src/schema.ts',
    tag: 'state',
    lang: 'typescript',
    code: `// schema.ts — declare state once; codegen emits typed
// server classes AND the browser descriptor.
const Player = schema({
    pid:   t.int64(),
    x:     t.number(),
    y:     t.number(),
    score: t.number(),
}, "Player");

export const State = schema({
    players: t.map(Player),
    coinX:   t.number(),
    coinY:   t.number(),
}, "State");`,
  },
]

export default function CodeSection() {
  const [active, setActive] = useState('server')

  const highlighted = useMemo(() => {
    const out = {}
    for (const tab of TABS) {
      out[tab.id] = hljs.highlight(tab.code, { language: tab.lang }).value
    }
    return out
  }, [])

  return (
    <section id="code" className="relative mx-auto max-w-5xl px-5 py-24 lg:px-8 lg:py-32">
      <SectionHead
        kicker="developer experience"
        title={
          <>
            Colyseus-shaped code.
            <br />
            <span className="text-grad">Rust-grade performance.</span>
          </>
        }
      />

      <Reveal>
        <div className="overflow-hidden rounded-2xl border border-line bg-panel shadow-2xl">
          <div className="flex flex-wrap items-center border-b border-line bg-panel-2/70">
            {TABS.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActive(tab.id)}
                className={`flex items-center gap-2 border-b-2 px-5 py-3.5 font-mono text-[12.5px] transition-colors ${
                  active === tab.id
                    ? 'border-oxide bg-panel text-white'
                    : 'border-transparent text-sub hover:text-sub'
                }`}
              >
                {tab.label}
                <span className="rounded-full border border-line bg-ink px-2 py-0.5 text-[10px] tracking-wide text-sub">
                  {tab.tag}
                </span>
              </button>
            ))}
          </div>

          {TABS.map((tab) => (
            <div key={tab.id} className={active === tab.id ? 'block' : 'hidden'}>
              <pre className="overflow-x-auto p-6">
                <code
                  className="hljs"
                  dangerouslySetInnerHTML={{ __html: highlighted[tab.id] }}
                />
              </pre>
            </div>
          ))}
        </div>
      </Reveal>
    </section>
  )
}
