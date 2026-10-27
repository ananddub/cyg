import { useState } from 'react'
import Reveal from './Reveal.jsx'
import SectionHead from './SectionHead.jsx'

/* real console captures from docs/screenshots — no mock, no fake data */
const SHOTS = [
  {
    id: 'telemetry',
    label: 'Live telemetry',
    src: '/console-overview.png',
    caption:
      'Tick-time traces, tick distribution histogram, transport mix and queue depths — one live feed, scoped to this game only.',
  },
  {
    id: 'studio',
    label: 'WASM studio',
    src: '/console-wasm-modules.png',
    caption:
      'Upload a module, verify its ABI, activate or roll it back — without ever touching the server.',
  },
]

export default function ConsolePreview() {
  const [active, setActive] = useState('telemetry')
  const shot = SHOTS.find((s) => s.id === active)

  return (
    <section id="console" className="relative border-t border-line bg-panel/30">
      <div className="mx-auto max-w-7xl px-5 py-24 lg:px-8 lg:py-32">
        <SectionHead
          kicker="web console"
          title={
            <>
              Run your fleet from <span className="text-grad">the dashboard.</span>
            </>
          }
          sub={
            <>
              Live telemetry, rooms &amp; clients, WASM studio with rollback, map uploads, per-game
              tenancy everywhere. Served by the binary itself at{' '}
              <code className="code-inline">/dashboard/</code>.
            </>
          }
        />

        <Reveal>
          {/* minimal view switcher — no fake chrome */}
          <div className="mb-8 flex justify-center gap-2">
            {SHOTS.map((s) => (
              <button
                key={s.id}
                onClick={() => setActive(s.id)}
                className={`rounded-full border px-4.5 py-2 text-[13px] font-medium transition-all ${
                  active === s.id
                    ? 'border-oxide/40 bg-oxide/10 text-oxide'
                    : 'border-line text-sub hover:border-zinc-600 hover:text-fg'
                }`}
              >
                {s.label}
              </button>
            ))}
          </div>

          {/* the product, presented straight — border, deep shadow, soft brand glow */}
          <div className="relative">
            <div
              className="pointer-events-none absolute -inset-x-10 -bottom-14 h-48 bg-oxide/10 blur-[110px]"
              aria-hidden="true"
            />
            <div className="relative overflow-hidden rounded-xl border border-line shadow-[0_48px_140px_-40px_rgba(0,0,0,0.95)]">
              <img
                src={shot.src}
                alt={`CYG console — ${shot.label}`}
                className="block w-full"
                loading="lazy"
              />
            </div>
          </div>

          <p className="mx-auto mt-7 max-w-2xl text-center text-[13.5px] leading-relaxed text-mute">
            {shot.caption}
          </p>
        </Reveal>
      </div>
    </section>
  )
}
