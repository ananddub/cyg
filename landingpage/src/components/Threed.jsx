import { lazy, Suspense, useMemo, useRef } from 'react'
import gsap from 'gsap'
import { useGSAP } from '@gsap/react'
import hljs from 'highlight.js/lib/core'
import ini from 'highlight.js/lib/languages/ini'
import Reveal from './Reveal.jsx'

hljs.registerLanguage('ini', ini)

const ArenaScene = lazy(() => import('../three/ArenaScene.jsx'))

const POINTS = [
  {
    h: 'Rapier3D runs on the host',
    p: 'Rigid bodies, colliders and raycasts execute inside the authoritative tick — clients only render the state they receive.',
  },
  {
    h: 'Cook your maps for instant boot',
    p: (
      <>
        <code className="code-inline">cyg-gaming cook arena.glb → arena.oxcl</code> compiles 3D collision
        meshes ahead of time; upload meshes, arenas or navmeshes straight from the console.
      </>
    ),
  },
  {
    h: 'Navmesh A* + spatial AOI',
    p: 'Pathfinding and interest management ship inbuilt — dense worlds stay light because clients only get what they can see.',
  },
  {
    h: 'Lag-compensated raycasts',
    p: 'Shots register against where players actually were on the authoritative tick — not where they appear now.',
  },
]

/* illustrative 3D-arena config — real config keys, real capabilities */
const CONFIG_SAMPLE = `name = "my-arena"

tick_rate   = 30
max_clients = 64

width  = 1000.0   # arena bounds
height = 1000.0

[rollback]
enabled          = true
max_replay_ticks = 32`

export default function Threed() {
  const root = useRef(null)
  const configHtml = useMemo(() => hljs.highlight(CONFIG_SAMPLE, { language: 'ini' }).value, [])

  useGSAP(() => {
    gsap.from('.threed-el', {
      opacity: 0,
      y: 24,
      duration: 0.8,
      stagger: 0.1,
      ease: 'power3.out',
      scrollTrigger: { trigger: root.current, start: 'top 75%', once: true },
    })
  }, { scope: root })

  return (
    <section ref={root} id="threed" className="relative border-t border-line">
      <div className="mx-auto max-w-7xl px-5 py-24 lg:px-8 lg:py-32">
        <div className="grid items-center gap-12 lg:grid-cols-2 lg:gap-16">
          {/* copy */}
          <div className="min-w-0">
            <div className="threed-el eyebrow mb-5">3D worlds</div>
            <h2 className="threed-el font-display text-3xl font-bold leading-[1.12] text-fg sm:text-[40px]">
              Full 3D worlds,
              <br />
              <span className="text-grad">served authoritative.</span>
            </h2>
            <p className="threed-el mt-5 max-w-lg text-[15.5px] leading-relaxed text-sub">
              The same binary hosts full 3D games — host-side physics, cooked collision meshes and
              navmesh pathfinding run inside the tick loop, so every jump, shot and collision is
              decided by the server.
            </p>

            <div className="mt-9 space-y-6">
              {POINTS.map((pt, i) => (
                <div key={pt.h} className="threed-el flex gap-4">
                  <div className="font-mono text-[11px] font-semibold text-oxide/70">
                    {String(i + 1).padStart(2, '0')}
                  </div>
                  <div>
                    <h4 className="font-display text-[15.5px] font-semibold text-fg">{pt.h}</h4>
                    <p className="mt-1 text-[13.5px] leading-relaxed text-sub">{pt.p}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* live 3D arena with floating config card */}
          <div className="threed-el relative min-w-0 lg:pb-10">
            <div
              className="pointer-events-none absolute -inset-8 rounded-full bg-oxide/5 blur-[100px]"
              aria-hidden="true"
            />
            <div className="relative h-[420px] overflow-hidden rounded-2xl border border-line bg-panel shadow-[0_32px_100px_-32px_rgba(0,0,0,0.9)] sm:h-[520px]">
              <Suspense fallback={null}>
                <ArenaScene />
              </Suspense>
              {/* corner label */}
              <div className="absolute left-4 top-4 rounded-md border border-line bg-ink/80 px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.2em] text-mute backdrop-blur">
                live 3d arena · host tick
              </div>
              {/* legend — what the scene demonstrates (top-right, clear of the config card) */}
              <div className="absolute right-4 top-4 flex flex-col items-end gap-2">
                <span className="rounded-md border border-line bg-ink/80 px-2.5 py-1 font-mono text-[10px] text-oxide backdrop-blur">
                  orb pickup → +1 score
                </span>
                <span className="rounded-md border border-line bg-ink/80 px-2.5 py-1 font-mono text-[10px] text-ember backdrop-blur">
                  lag-compensated raycast
                </span>
                <span className="rounded-md border border-line bg-ink/80 px-2.5 py-1 font-mono text-[10px] text-frost backdrop-blur">
                  fall off → respawn
                </span>
              </div>
            </div>

            {/* floating config card — overlaps the arena's bottom edge */}
            <div className="relative z-10 mx-5 -mt-16 lg:absolute lg:-bottom-16 lg:left-10 lg:mt-0 lg:w-[360px]">
              <div className="overflow-hidden rounded-xl border border-line bg-panel/95 shadow-[0_24px_70px_-16px_rgba(0,0,0,0.95)] backdrop-blur">
                <div className="flex items-center justify-between border-b border-line bg-panel-2/60 px-4 py-2.5">
                  <span className="font-mono text-[11.5px] text-sub">config.toml</span>
                  <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-mute">a 3d arena</span>
                </div>
                <pre className="overflow-x-auto p-4">
                  <code className="hljs !text-[11.5px]" dangerouslySetInnerHTML={{ __html: configHtml }} />
                </pre>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
