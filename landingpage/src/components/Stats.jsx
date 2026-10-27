import { useRef } from 'react'
import gsap from 'gsap'
import { useGSAP } from '@gsap/react'

const STATS = [
  { value: 7000, suffix: '+', label: 'concurrent players per core', sub: '@128 Hz · WASM + Rapier2D' },
  { value: 0.08, decimals: 2, suffix: ' ms', label: 'p99 tick time', sub: '100-player rooms' },
  { value: 10, suffix: 'k', label: 'CCU per core', sub: 'ECS movement backend' },
  { value: 30, suffix: '+', label: 'sys_* host imports', sub: 'permission-gated per game' },
]

export default function Stats() {
  const root = useRef(null)

  useGSAP(() => {
    gsap.utils.toArray('.stat-num', root.current).forEach((el) => {
      const target = parseFloat(el.dataset.value)
      const decimals = parseInt(el.dataset.decimals || '0', 10)
      const suffix = el.dataset.suffix || ''
      const obj = { v: 0 }
      gsap.to(obj, {
        v: target,
        duration: 2,
        ease: 'power3.out',
        scrollTrigger: { trigger: el, start: 'top 88%', once: true },
        onUpdate() {
          el.textContent =
            decimals > 0
              ? obj.v.toFixed(decimals) + suffix
              : Math.round(obj.v).toLocaleString('en-IN') + suffix
        },
      })
    })
  }, { scope: root })

  return (
    <section ref={root} className="relative border-b border-line">
      <div className="mx-auto grid max-w-7xl grid-cols-2 gap-px lg:grid-cols-4">
        {STATS.map((s, i) => (
          <div
            key={i}
            className="group relative px-6 py-12 text-center transition-colors duration-300 hover:bg-panel/60 lg:py-16"
          >
            <div className="font-display text-[44px] font-bold leading-none tracking-tight text-fg sm:text-6xl">
              <span
                className="stat-num"
                data-value={s.value}
                data-decimals={s.decimals || 0}
                data-suffix={s.suffix}
              >
                0
              </span>
            </div>
            <div className="mt-4 text-[13px] font-medium text-sub">{s.label}</div>
            <div className="mt-1 font-mono text-[11px] tracking-wide text-mute">{s.sub}</div>
            {/* hover accent */}
            <div className="absolute inset-x-8 bottom-0 h-px bg-gradient-to-r from-transparent via-oxide to-transparent opacity-0 transition-opacity duration-300 group-hover:opacity-100" />
          </div>
        ))}
      </div>
    </section>
  )
}
