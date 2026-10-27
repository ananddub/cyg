import { siTypescript, siUnity, siUnrealengine, siGodotengine, siCplusplus } from 'simple-icons'
import Reveal from './Reveal.jsx'
import SectionHead from './SectionHead.jsx'

/* ── mini visuals living inside the bento cards ─────────────── */

/** WASM card: live-looking code inset + hot-reload badge */
function WasmVisual() {
  return (
    <div className="mt-6 flex flex-wrap items-stretch gap-4">
      <div className="min-w-[240px] flex-1 rounded-xl border border-line bg-ink/80 p-4 font-mono text-[12px] leading-[1.9]">
        <div><span className="text-frost">onTick</span>(dt: f64): i64 {'{'}</div>
        <div className="pl-4 text-sub">Rapier2D.step(dt);</div>
        <div className="pl-4"><span className="text-ember">return</span> <span className="text-fg">this</span>.commitState();</div>
        <div>{'}'}</div>
      </div>
      <div className="flex flex-col justify-center gap-2.5">
        <span className="chip !text-[11.5px]"><span className="pulse-dot" /> hot-reload on save</span>
        <span className="chip !text-[11.5px]">per-room sandbox</span>
        <span className="chip !text-[11.5px]">zero-GC guest</span>
      </div>
    </div>
  )
}

/** Netcode card: two animated lanes — fast discrete + slow reliable */
function LaneVisual() {
  return (
    <div className="mt-6 space-y-5">
      <div>
        <div className="mb-2 flex items-center justify-between font-mono text-[10.5px] uppercase tracking-[0.18em] text-mute">
          <span>game-unreliable</span><span className="text-oxide">128 Hz</span>
        </div>
        <div className="lane-track"><i /><i style={{ animationDelay: '0.29s' }} /><i style={{ animationDelay: '0.58s' }} /><i style={{ animationDelay: '0.87s' }} /></div>
      </div>
      <div>
        <div className="mb-2 flex items-center justify-between font-mono text-[10.5px] uppercase tracking-[0.18em] text-mute">
          <span>game-reliable</span><span className="text-ember">ordered</span>
        </div>
        <div className="lane-track amber"><i /><i style={{ animationDelay: '1.05s' }} /></div>
      </div>
    </div>
  )
}

/** BaaS card: cloud of capability tags */
function TagVisual() {
  const tags = ['accounts', 'MFA', 'friends', 'groups', 'chat', 'parties', 'presence', 'storage', 'leaderboards', 'tournaments', 'IAP', 'inventory', 'matchmaker']
  return (
    <div className="mt-6 flex flex-wrap gap-1.5">
      {tags.map((t) => (
        <span key={t} className="rounded-md border border-line bg-panel-2 px-2 py-1 font-mono text-[10.5px] text-sub">
          {t}
        </span>
      ))}
    </div>
  )
}

/** Console card: animated EQ bars */
function EqVisual() {
  const bars = [0, 1, 2, 3, 4, 5, 6, 7]
  return (
    <div className="mt-6 flex h-16 items-end gap-1.5">
      {bars.map((i) => (
        <div
          key={i}
          className="eq-bar flex-1 rounded-t bg-gradient-to-t from-oxide-deep/60 to-oxide"
          style={{ height: `${38 + ((i * 37) % 55)}%`, animationDelay: `${i * 0.14}s` }}
        />
      ))}
    </div>
  )
}

/** SDK card: platform badges with real logos */
function PlatformVisual() {
  const platforms = [
    { icon: siTypescript, name: 'Web / TS' },
    { icon: siUnity, name: 'Unity' },
    { icon: siUnrealengine, name: 'Unreal' },
    { icon: siGodotengine, name: 'Godot' },
    { icon: siCplusplus, name: 'Native C++' },
  ]
  return (
    <div className="flex flex-wrap items-center gap-2.5">
      {platforms.map((p) => (
        <span key={p.name} className="flex items-center gap-2 rounded-lg border border-line bg-panel-2 px-3.5 py-2 font-display text-[13px] font-semibold text-fg">
          <svg viewBox="0 0 24 24" className="h-4 w-4 fill-oxide" aria-hidden="true">
            <path d={p.icon.path} />
          </svg>
          {p.name}
        </span>
      ))}
    </div>
  )
}

/* ── bento grid ─────────────────────────────────────────────── */

const cardCls =
  'group relative h-full overflow-hidden rounded-2xl border border-line bg-panel p-7 transition-colors duration-300 hover:border-oxide/40'

export default function Features() {
  return (
    <section id="features" className="relative mx-auto max-w-7xl px-5 py-24 lg:px-8 lg:py-32">
      <SectionHead
        kicker="why cyg"
        title={
          <>
            Everything in the box.
            <br />
            <span className="text-grad">Nothing to assemble.</span>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-6">
        {/* WASM rooms — big */}
        <Reveal className="sm:col-span-2 lg:col-span-4">
          <div className={cardCls}>
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3 className="font-display text-xl font-semibold text-fg">Authoritative WASM rooms</h3>
                <p className="mt-3 max-w-lg text-[13.5px] leading-relaxed text-sub">
                  Write rooms in AssemblyScript — extend <code className="code-inline">Room</code>, mutate
                  typed reactive state, <code className="code-inline">commitState()</code>. A trapping
                  room fails alone — crashes and bad memory accesses stay inside the room while the
                  host keeps running.
                </p>
              </div>
            </div>
            <WasmVisual />
          </div>
        </Reveal>

        {/* netcode */}
        <Reveal delay={0.08} className="lg:col-span-2">
          <div className={cardCls}>
            <div className="flex items-start justify-between gap-4">
              <h3 className="font-display text-xl font-semibold text-fg">Dual-lane netcode</h3>
            </div>
            <p className="mt-3 text-[13.5px] leading-relaxed text-sub">
              WebRTC for browsers, raw UDP for native — same QUIC-tier wire: per-frame sequence,
              reorder-safe, SACK resends, sendmmsg + UDP-GSO fan-out.
            </p>
            <LaneVisual />
            <div className="mt-5 rounded-lg border border-frost/25 bg-frost/5 px-3 py-2 font-mono text-[11px] leading-relaxed text-sub">
              <span className="text-frost">+ QUIC / WebTransport</span> — real fourth lane via{' '}
              <span className="text-oxide">quic_port</span>, same contract: streams = reliable,
              datagrams = newest-wins
            </div>
          </div>
        </Reveal>

        {/* physics */}
        <Reveal className="lg:col-span-2">
          <div className={cardCls}>
            <div className="flex items-start justify-between gap-4">
              <h3 className="font-display text-xl font-semibold text-fg">Server-side physics</h3>
            </div>
            <p className="mt-3 text-[13.5px] leading-relaxed text-sub">
              Rapier2D/3D, navmesh A*, deterministic RNG, lag-compensated raycasts — permission-gated
              per game.
            </p>
            <div className="mt-6 flex items-center gap-2 font-mono text-[11px] text-mute">
              <span className="text-oxide">30+</span> sys_* host imports
            </div>
          </div>
        </Reveal>

        {/* BaaS */}
        <Reveal delay={0.08} className="lg:col-span-2">
          <div className={cardCls}>
            <div className="flex items-start justify-between gap-4">
              <h3 className="font-display text-xl font-semibold text-fg">Complete player backend</h3>
            </div>
            <p className="mt-3 text-[13.5px] leading-relaxed text-sub">
              The whole Nakama checklist, built in — no second service to run.
            </p>
            <TagVisual />
          </div>
        </Reveal>

        {/* console */}
        <Reveal delay={0.16} className="lg:col-span-2">
          <div className={cardCls}>
            <div className="flex items-start justify-between gap-4">
              <h3 className="font-display text-xl font-semibold text-fg">Web console</h3>
            </div>
            <p className="mt-3 text-[13.5px] leading-relaxed text-sub">
              Live telemetry, WASM studio with rollback, map uploads, bans that kick in realtime.
            </p>
            <EqVisual />
          </div>
        </Reveal>

        {/* SDK — wide */}
        <Reveal className="sm:col-span-2 lg:col-span-6">
          <div className={`${cardCls} flex flex-col gap-6 lg:flex-row lg:items-center lg:justify-between`}>
            <div className="max-w-xl">
              <div className="flex items-center gap-4">
                <h3 className="font-display text-xl font-semibold text-fg">Reactive client SDK</h3>
              </div>
              <p className="mt-3 text-[13.5px] leading-relaxed text-sub">
                Colyseus-shaped, no alien syntax. Descriptor-driven decode at join:{' '}
                <code className="code-inline">onStateChange</code>,{' '}
                <code className="code-inline">player.listen(&quot;x&quot;)</code>,{' '}
                <code className="code-inline">onAdd/onRemove</code>,{' '}
                <code className="code-inline">room.request()</code> RPC.
              </p>
            </div>
            <PlatformVisual />
          </div>
        </Reveal>
      </div>
    </section>
  )
}
