import { Logo } from './Nav.jsx'

const COLS = [
  {
    h: 'Engine',
    links: [
      ['WASM rooms', '#features'],
      ['Netcode', '#features'],
      ['Physics', '#features'],
      ['Benchmarks', '#benchmarks'],
    ],
  },
  {
    h: 'Platform',
    links: [
      ['Documentation', '/docs.html'],
      ['Web console', '#console'],
      ['Player backend', '#features'],
      ['Architecture', '#architecture'],
      ['Quick start', '#quickstart'],
    ],
  },
  {
    h: 'Project',
    links: [
      ['GitHub', 'https://github.com/ananddub/cyg'],
      ['Apache-2.0 license', 'https://github.com/ananddub/cyg/blob/main/LICENSE'],
    ],
  },
]

export default function Footer() {
  return (
    <footer className="border-t border-line bg-panel/40">
      <div className="mx-auto flex max-w-7xl flex-col gap-12 px-5 py-16 lg:flex-row lg:justify-between lg:px-8">
        <div className="flex items-start gap-3 text-oxide">
          <Logo className="h-8 w-8" />
          <div>
            <b className="font-display text-[16px] font-semibold text-fg">CYG</b>
            <div className="mt-1 text-[12.5px] text-sub">cook your game — the single-binary game server platform</div>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-10 sm:grid-cols-3">
          {COLS.map((col) => (
            <div key={col.h}>
              <h5 className="mb-4 font-mono text-[11px] font-semibold uppercase tracking-widest text-sub">
                {col.h}
              </h5>
              <ul className="space-y-2.5">
                {col.links.map(([label, href]) => (
                  <li key={label}>
                    <a
                      href={href}
                      {...(href.startsWith('http') ? { target: '_blank', rel: 'noopener' } : {})}
                      className="text-[13.5px] text-sub transition-colors hover:text-oxide"
                    >
                      {label}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </div>

      <div className="border-t border-line px-5 py-6 text-center text-[12px] text-sub">
        <div>© {new Date().getFullYear()} CYG · cook your game — written in Rust, served by one binary.</div>
        <div className="mx-auto mt-2 max-w-2xl text-[11px] leading-relaxed text-mute">
          CYG is independent open-source software, not affiliated with or endorsed by the Rust
          Project. Rust is a trademark of the Rust Foundation; the Rust logo is used under{' '}
          <a
            href="https://creativecommons.org/licenses/by/4.0/"
            target="_blank"
            rel="noopener"
            className="underline decoration-line underline-offset-2 hover:text-sub"
          >
            CC-BY
          </a>.
        </div>
      </div>
    </footer>
  )
}
