import { useEffect, useState } from 'react'

const LINKS = [
  ['Features', '#features'],
  ['3D', '#threed'],
  ['Architecture', '#architecture'],
  ['Code', '#code'],
  ['Console', '#console'],
  ['Benchmarks', '#benchmarks'],
  ['Docs', 'https://github.com/ananddub/cyg#readme'],
]

export function Logo({ className = 'h-7 w-7' }) {
  return (
    <svg viewBox="0 0 32 32" className={className} aria-hidden="true">
      <path d="M16 2 29 9.5v13L16 30 3 22.5v-13z" fill="none" stroke="currentColor" strokeWidth="2.2" />
      <circle cx="16" cy="16" r="4" fill="currentColor" />
    </svg>
  )
}

export default function Nav() {
  const [scrolled, setScrolled] = useState(false)
  const [open, setOpen] = useState(false)

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 24)
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])

  return (
    <header
      className={`fixed inset-x-0 top-0 z-50 transition-all duration-300 ${
        scrolled ? 'bg-ink/80 backdrop-blur-xl border-b border-line' : 'bg-transparent border-b border-transparent'
      }`}
    >
      <div className="mx-auto flex h-16 max-w-7xl items-center justify-between px-5 lg:px-8">
        <a href="#top" className="flex items-center gap-2.5 text-oxide">
          <Logo />
          <span className="flex flex-col leading-none">
            <span className="font-display text-[18px] font-bold tracking-tight text-fg">CYG</span>
            <span className="mt-0.5 font-mono text-[9px] uppercase tracking-[0.22em] text-mute">cook your game</span>
          </span>
        </a>

        <nav className="hidden items-center gap-7 lg:flex">
          {LINKS.map(([label, href]) => (
            <a
              key={href}
              href={href}
              {...(href.startsWith('http') ? { target: '_blank', rel: 'noopener' } : {})}
              className="text-[13.5px] font-medium text-sub transition-colors hover:text-fg"
            >
              {label}
            </a>
          ))}
        </nav>

        <div className="flex items-center gap-3">
          <a
            href="https://github.com/ananddub/cyg"
            target="_blank"
            rel="noopener"
            className="btn-ghost btn-sm hidden sm:inline-flex"
          >
            <svg viewBox="0 0 16 16" width="15" height="15" fill="currentColor" aria-hidden="true">
              <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.6 7.6 0 0 1 2-.27c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z" />
            </svg>
            GitHub
          </a>
          <a href="#quickstart" className="btn-primary btn-sm hidden sm:inline-flex">
            Get started
          </a>
          <button
            onClick={() => setOpen(!open)}
            aria-label="menu"
            className="flex h-9 w-9 flex-col items-center justify-center gap-[5px] rounded-lg border border-line lg:hidden"
          >
            <span className={`h-[1.5px] w-4 bg-slate-300 transition-transform ${open ? 'translate-y-[6.5px] rotate-45' : ''}`} />
            <span className={`h-[1.5px] w-4 bg-slate-300 transition-opacity ${open ? 'opacity-0' : ''}`} />
            <span className={`h-[1.5px] w-4 bg-slate-300 transition-transform ${open ? '-translate-y-[6.5px] -rotate-45' : ''}`} />
          </button>
        </div>
      </div>

      {/* mobile menu */}
      {open && (
        <nav className="border-t border-line bg-ink/95 backdrop-blur-xl px-5 py-4 lg:hidden">
          {LINKS.map(([label, href]) => (
            <a
              key={href}
              href={href}
              {...(href.startsWith('http') ? { target: '_blank', rel: 'noopener' } : {})}
              onClick={() => setOpen(false)}
              className="block rounded-lg px-3 py-2.5 text-[15px] font-medium text-sub hover:bg-panel hover:text-fg"
            >
              {label}
            </a>
          ))}
          <a
            href="#quickstart"
            onClick={() => setOpen(false)}
            className="mt-2 block rounded-lg bg-oxide px-3 py-2.5 text-center text-[15px] font-semibold text-ink"
          >
            Get started
          </a>
        </nav>
      )}
    </header>
  )
}
