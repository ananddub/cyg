import { useEffect, useState } from 'react'
import Reveal from './Reveal.jsx'
import SectionHead from './SectionHead.jsx'

/* typed lines: cmd = typed char-by-char, out = printed instantly */
const SCRIPT = [
  { type: 'cmd', text: 'curl -fsSL https://raw.githubusercontent.com/ananddub/cyg/main/scripts/install.sh | sh' },
  { type: 'out', text: '✓ cyg-gaming installed (latest release binary)', cls: 'text-oxide' },
  { type: 'cmd', text: 'cyg-gaming init my-game && cd my-game' },
  { type: 'out', text: '✓ scaffolded room.ts · schema.ts · SDK · website', cls: 'text-oxide' },
  { type: 'cmd', text: 'cyg-gaming doctor' },
  { type: 'out', text: '✓ config.toml  ✓ schema  ✓ ABI exports', cls: 'text-oxide' },
  { type: 'cmd', text: 'cyg-gaming run' },
  { type: 'out', text: '✓ room.ts → room_logic.wasm (41 kB) · schema descriptor emitted', cls: 'text-oxide' },
  { type: 'out', text: '▶ game data   0.0.0.0:7779  (WebRTC · UDP · QUIC · 128 Hz)', cls: 'text-frost' },
  { type: 'out', text: '▶ control     0.0.0.0:8124  (console → /dashboard/)', cls: 'text-frost' },
  { type: 'out', text: '▶ rooms: 1 active · 0 clients · embedded SurrealDB ready', cls: 'text-ember' },
]

function useTerminal() {
  const [lines, setLines] = useState([])
  const [current, setCurrent] = useState('')

  useEffect(() => {
    let cancelled = false
    const timers = []
    const wait = (ms) => new Promise((res) => timers.push(setTimeout(res, ms)))

    async function run() {
      while (!cancelled) {
        setLines([])
        setCurrent('')
        await wait(700)

        for (const line of SCRIPT) {
          if (cancelled) return
          if (line.type === 'cmd') {
            for (let i = 1; i <= line.text.length; i++) {
              if (cancelled) return
              setCurrent(line.text.slice(0, i))
              await wait(26 + Math.random() * 30)
            }
            await wait(350)
            setLines((ls) => [...ls, line])
            setCurrent('')
          } else {
            await wait(240)
            setLines((ls) => [...ls, line])
          }
        }

        await wait(5200)
      }
    }
    run()

    return () => {
      cancelled = true
      timers.forEach(clearTimeout)
    }
  }, [])

  return { lines, current }
}

const STEPS = [
  {
    n: '01',
    title: 'Install the binary',
    body: (
      <>
        One line: <code className="text-oxide">curl … install.sh | sh</code> — or{' '}
        <code className="text-oxide">docker pull dubeyanand/cyg-gaming</code>, or{' '}
        <code className="text-oxide">just up</code> from source. SurrealDB is embedded, nothing else
        to install.
      </>
    ),
  },
  {
    n: '02',
    title: 'Scaffold a game',
    body: (
      <>
        <code className="text-oxide">cyg-gaming init my-game</code> generates{' '}
        <code className="text-oxide">room.ts</code>, <code className="text-oxide">schema.ts</code>,
        the SDK and a playable website. <code className="text-oxide">cyg-gaming doctor</code> validates
        config, schema and ABI.
      </>
    ),
  },
  {
    n: '03',
    title: 'Run it',
    body: (
      <>
        <code className="text-oxide">cyg-gaming run</code> compiles your room to WASM and serves game
        data, signaling, console and website — one process, production-ready.
      </>
    ),
  },
]

export default function Quickstart() {
  const { lines, current } = useTerminal()

  return (
    <section id="quickstart" className="relative border-t border-line bg-panel/30">
      <div className="mx-auto max-w-7xl px-5 py-24 lg:px-8 lg:py-32">
        <SectionHead
          kicker="quick start"
          title={
            <>
              Zero to a running room in <span className="text-grad">three commands.</span>
            </>
          }
        />

        <div className="grid items-start gap-8 lg:grid-cols-2">
          {/* terminal */}
          <Reveal>
            <div className="overflow-hidden rounded-2xl border border-line bg-[#0a0a0c] shadow-2xl">
              <div className="flex items-center gap-2 border-b border-line bg-panel-2/70 px-5 py-3.5">
                <span className="h-2.5 w-2.5 rounded-full bg-line" />
                <span className="h-2.5 w-2.5 rounded-full bg-line" />
                <span className="h-2.5 w-2.5 rounded-full bg-line" />
                <span className="ml-3 font-mono text-[12px] text-mute">~/games/my-game</span>
              </div>
              <div className="min-h-[320px] p-5 font-mono text-[13px] leading-[1.9]">
                {lines.map((l, i) =>
                  l.type === 'cmd' ? (
                    <div key={i}>
                      <span className="mr-2 text-oxide">$</span>
                      <span className="text-sub">{l.text}</span>
                    </div>
                  ) : (
                    <div key={i} className={l.cls}>{l.text}</div>
                  ),
                )}
                <div>
                  <span className="mr-2 text-oxide">$</span>
                  <span className="text-sub">{current}</span>
                  <span className="term-caret" />
                </div>
              </div>
            </div>
          </Reveal>

          {/* steps */}
          <Reveal delay={0.12}>
            <div className="space-y-7">
              {STEPS.map((s) => (
                <div key={s.n} className="flex gap-5">
                  <div className="font-display text-[26px] font-bold text-oxide/40">{s.n}</div>
                  <div>
                    <h4 className="font-display text-[16px] font-semibold text-white">{s.title}</h4>
                    <p className="mt-1.5 text-[13.5px] leading-relaxed text-sub [&>code]:rounded [&>code]:bg-panel-2 [&>code]:px-1 [&>code]:py-0.5 [&>code]:font-mono [&>code]:text-[12px]">
                      {s.body}
                    </p>
                  </div>
                </div>
              ))}


            </div>
          </Reveal>
        </div>
      </div>
    </section>
  )
}
