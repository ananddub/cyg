import { useRef } from 'react'
import gsap from 'gsap'
import { useGSAP } from '@gsap/react'
import Reveal from './Reveal.jsx'
import SectionHead from './SectionHead.jsx'

const BARS = [
  {
    name: 'WASM room — full combat logic',
    sub: 'move + shoot + damage + respawn · @60 Hz · p99 2.5 ms @500p',
    val: '~1.5k',
    width: 10,
    cls: 'from-ember/80 to-ember',
    glow: '',
  },
  {
    name: 'Combined room tick (WASM + Rapier3D + delta)',
    sub: '@60 Hz · p99 0.16 ms @100p',
    val: '~5k',
    width: 33,
    cls: 'from-teal-600/80 to-teal-400',
    glow: '',
  },
  {
    name: 'WASM room + inbuilt Rapier2D',
    sub: '@128 Hz · 100–200p rooms · p99 0.08 ms @100p',
    val: '~7k',
    width: 47,
    cls: 'from-oxide-deep to-oxide',
    glow: '',
  },
  {
    name: 'Inbuilt ECS backend',
    sub: 'movement · @128 Hz · p99 0.07 ms @100p',
    val: '~10k',
    width: 67,
    cls: 'from-frost/70 to-frost',
    glow: '',
  },
  {
    name: 'Rapier3D physics step',
    sub: '@60 Hz · 500-body rooms · p99 0.37 ms',
    val: '~15k',
    width: 100,
    cls: 'from-zinc-500 to-zinc-300',
    glow: '',
  },
]

export default function Benchmarks() {
  const root = useRef(null)

  useGSAP(() => {
    gsap.utils.toArray('.bar-fill', root.current).forEach((el) => {
      gsap.fromTo(
        el,
        { width: '0%' },
        {
          width: el.dataset.w + '%',
          duration: 1.4,
          ease: 'power3.out',
          scrollTrigger: { trigger: el, start: 'top 88%', once: true },
        },
      )
    })
  }, { scope: root })

  return (
    <section ref={root} id="benchmarks" className="relative mx-auto max-w-5xl px-5 py-24 lg:px-8 lg:py-32">
      <SectionHead
        kicker="measured, not marketing"
        title={
          <>
            Benchmarks, <span className="text-grad">reproducible.</span>
          </>
        }
        sub={
          <>
            Measured sandboxed in a strict <b className="text-fg">1-core / 2 GB cgroup</b> — every
            published number sits <b className="text-fg">below</b> what the same run measured.
            Reproducible from the repo.
          </>
        }
      />

      <Reveal>
        <div className="space-y-8 rounded-2xl border border-line bg-panel p-6 sm:p-10">
          {BARS.map((b) => (
            <div key={b.name}>
              <div className="mb-2.5 flex flex-wrap items-end justify-between gap-2">
                <div>
                  <div className="font-display text-[15px] font-semibold text-white">{b.name}</div>
                  <div className="mt-0.5 text-[12px] text-sub">{b.sub}</div>
                </div>
                <div className="font-display text-xl font-bold text-white">
                  {b.val} <em className="text-[12px] font-normal not-italic text-sub">CCU/core</em>
                </div>
              </div>
              <div className="h-2.5 overflow-hidden rounded-full bg-ink">
                <div
                  className={`bar-fill h-full rounded-full bg-gradient-to-r ${b.cls} ${b.glow}`}
                  data-w={b.width}
                  style={{ width: 0 }}
                />
              </div>
            </div>
          ))}

          {/* real capacity — what one process actually holds */}
          <div className="mt-2 grid grid-cols-2 gap-px overflow-hidden rounded-xl border border-line bg-line lg:grid-cols-4">
            {[
              ['~4k', 'combat CCU served by one $10/mo VPS @60 Hz'],
              ['100 × 100', 'rooms × players at 128 Hz on a 2-core / 2 GB pod'],
              ['0.76×', 'of the tick budget used at that load'],
              ['93 MB', 'peak RAM at that load — 4.6% of the pod'],
            ].map(([num, label]) => (
              <div key={label} className="bg-panel px-5 py-6 text-center">
                <div className="font-display text-[26px] font-bold text-fg">{num}</div>
                <div className="mt-2 text-[12px] leading-relaxed text-mute">{label}</div>
              </div>
            ))}
          </div>
        </div>
      </Reveal>
    </section>
  )
}
