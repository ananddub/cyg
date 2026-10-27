import { lazy, Suspense, useRef } from 'react'
import gsap from 'gsap'
import { useGSAP } from '@gsap/react'

const SceneCanvas = lazy(() => import('../three/Scene.jsx'))

const CHIPS = [
  ['~7k', ' CCU / core @128 Hz'],
  ['0.08 ms', ' p99 tick'],
  ['93 MB', ' RAM on a 2-core pod'],
]

export default function Hero() {
  const root = useRef(null)

  useGSAP(() => {
    /* entrance */
    const tl = gsap.timeline({ defaults: { ease: 'power3.out' } })
    tl.from('.hero-el', { opacity: 0, y: 28, duration: 0.9, stagger: 0.11, delay: 0.2 })
    tl.from('.hero-chip', { opacity: 0, y: 14, duration: 0.6, stagger: 0.08 }, '-=0.4')

    /* scroll parallax — content lifts & fades, scene zooms in slightly */
    gsap.to('.hero-inner', {
      yPercent: -12,
      opacity: 0.15,
      ease: 'none',
      scrollTrigger: { trigger: root.current, start: 'top top', end: 'bottom top', scrub: true },
    })
    gsap.to('.hero-scene', {
      scale: 1.1,
      ease: 'none',
      scrollTrigger: { trigger: root.current, start: 'top top', end: 'bottom top', scrub: true },
    })
  }, { scope: root })

  return (
    <section ref={root} id="top" className="relative flex min-h-[100svh] items-center justify-center overflow-hidden">
      {/* WebGL backdrop — lazy so three.js code-splits out of the entry chunk */}
      <div className="hero-scene absolute inset-0">
        <Suspense fallback={null}>
          <SceneCanvas />
        </Suspense>
      </div>
      {/* readability vignette + bottom fade into next section */}
      <div className="hero-vignette pointer-events-none absolute inset-0" />
      <div className="pointer-events-none absolute inset-x-0 bottom-0 h-40 bg-gradient-to-b from-transparent to-ink" />

      <div className="hero-inner pointer-events-none relative z-10 mx-auto max-w-4xl px-5 pb-20 pt-32 text-center">
        <div className="hero-el chip mb-8 !py-1.5 text-[12px] font-medium">
          <span className="pulse-dot" />
          Open-source · Apache-2.0 · Self-hosted
        </div>

        <h1 className="hero-el hero-title font-display text-[42px] font-bold leading-[1.06] text-fg sm:text-6xl lg:text-[72px]">
          Your entire multiplayer backend.
          <br />
          <span className="text-grad">One Rust binary.</span>
        </h1>

        <p className="hero-el hero-sub-shadow mx-auto mt-7 max-w-2xl text-[16px] leading-relaxed text-sub sm:text-[17.5px]">
          Authoritative WASM game rooms, WebRTC&nbsp;·&nbsp;UDP&nbsp;·&nbsp;QUIC netcode, server-side physics
          and a complete player backend — what Colyseus&nbsp;+&nbsp;Nakama do together, CYG does alone.
          On a $10/mo VPS.
        </p>

        <div className="hero-el pointer-events-auto mt-10 flex flex-wrap items-center justify-center gap-4">
          <a href="#quickstart" className="btn-primary btn-lg group">
            Start building
            <svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.8" className="transition-transform group-hover:translate-x-0.5">
              <path d="M2 8h11M9 3.5 13.5 8 9 12.5" />
            </svg>
          </a>
          <a href="https://github.com/ananddub/cyg" target="_blank" rel="noopener" className="btn-ghost btn-lg">
            View on GitHub
          </a>
        </div>

        <div className="mt-14 flex flex-wrap items-center justify-center gap-3">
          {CHIPS.map(([num, label]) => (
            <div key={label} className="hero-chip chip">
              <b className="font-semibold text-oxide">{num}</b>
              {label}
            </div>
          ))}
        </div>
      </div>

      {/* scroll hint */}
      <div className="absolute bottom-10 left-1/2 z-10 -translate-x-1/2">
        <div className="flex h-9 w-[22px] items-start justify-center rounded-full border border-line pt-1.5">
          <div className="wheel-dot h-1.5 w-1.5 rounded-full bg-oxide" />
        </div>
      </div>
    </section>
  )
}
