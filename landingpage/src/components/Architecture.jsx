import Reveal from './Reveal.jsx'
import SectionHead from './SectionHead.jsx'

const CONTROL = [
  ['matchmake + seat tokens', false],
  ['/ws/signal', true, ' — WebRTC signaling'],
  ['/v2/*', true, ' player API (BaaS)'],
  ['/v2/console/*', true, ' + web dashboard'],
  ['statics — game website + SDK', false],
]

const HOST = [
  ['WASM room runtime (hot-reload)', false],
  ['Rapier2D/3D physics', false],
  ['ECS backend', false],
  ['AOI / spatial index', false],
  ['WebRTC · UDP · QUIC data plane', false],
]

function Column({ title, tag, items, accent }) {
  return (
    <div className="rounded-xl border border-line bg-panel-2/60 p-6">
      <h4 className="font-display text-[15px] font-semibold text-white">
        {title}{' '}
        <span className={`ml-1 font-mono text-[11.5px] font-medium ${accent}`}>{tag}</span>
      </h4>
      <ul className="mt-4 space-y-2.5">
        {items.map(([text, isCode, rest], i) => (
          <li key={i} className="flex items-start gap-2.5 text-[13.5px] text-sub">
            <span className="mt-[7px] h-1 w-1 flex-none rounded-full bg-oxide/70" />
            <span>
              {isCode ? (
                <>
                  <code className="rounded bg-ink px-1.5 py-0.5 font-mono text-[12px] text-oxide">{text}</code>
                  {rest}
                </>
              ) : (
                text
              )}
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}

export default function Architecture() {
  return (
    <section id="architecture" className="relative border-t border-line bg-panel/30">
      <div className="mx-auto max-w-6xl px-5 py-24 lg:px-8 lg:py-32">
        <SectionHead
          kicker="architecture"
          title={
            <>
              One process. <span className="text-grad">Zero sidecars.</span>
            </>
          }
          sub={
            <>
              No Node cluster, no orchestra of services.{' '}
              <code className="rounded bg-panel-2 px-1.5 py-0.5 font-mono text-[12.5px] text-oxide">ox-game run</code>{' '}
              <em className="text-sub">is</em> the production artifact — both planes talk
              through one lock-free bus.
            </>
          }
        />

        <Reveal>
          <div className="overflow-hidden rounded-2xl border border-line bg-panel">
            <div className="flex items-center gap-2.5 border-b border-line bg-panel-2/70 px-6 py-3.5 font-mono text-[12.5px] text-sub">
              <span className="pulse-dot" />
              one process — <code className="text-oxide">cyg-gaming run</code>
            </div>

            <div className="grid gap-4 p-5 sm:p-6 md:grid-cols-2">
              <Column title="Control plane" tag="Axum" items={CONTROL} accent="text-frost" />
              <Column title="Game host loop" tag="pinned sim thread" items={HOST} accent="text-ember" />
            </div>

            {/* connectors into the bus */}
            <div className="mx-5 grid grid-cols-2 sm:mx-6">
              <div className="flex justify-center"><div className="h-6 w-px bg-gradient-to-b from-line to-oxide/50" /></div>
              <div className="flex justify-center"><div className="h-6 w-px bg-gradient-to-b from-line to-oxide/50" /></div>
            </div>

            <div className="mx-5 mb-6 sm:mx-6">
              <div className="bus-track rounded-xl border border-oxide/25 bg-oxide/5 px-6 py-3.5 text-center font-mono text-[12px] tracking-wide text-oxide/90">
                <i /><i /><i /><i /><i />
                lock-free bus — inputs · deltas · reloads · kicks
              </div>
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  )
}
