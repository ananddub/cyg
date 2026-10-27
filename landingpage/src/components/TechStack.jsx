import {
  siRust, siWebassembly, siWebrtc, siAssemblyscript,
  siPostgresql, siRedis, siDocker, siKubernetes,
} from 'simple-icons'
import Reveal from './Reveal.jsx'

const LOGOS = [
  { icon: siRust, name: 'Rust' },
  { icon: siWebassembly, name: 'WebAssembly' },
  { icon: siWebrtc, name: 'WebRTC' },
  { icon: siAssemblyscript, name: 'AssemblyScript' },
  { icon: siPostgresql, name: 'PostgreSQL' },
  { icon: siRedis, name: 'Redis' },
  { icon: siDocker, name: 'Docker' },
  { icon: siKubernetes, name: 'Kubernetes' },
]

/* no official logo — styled text sigils */
const SIGILS = ['Rapier2D/3D', 'QUIC', 'WebTransport', 'Axum', 'ECS', 'str0m', 'wasmtime']

function Icon({ icon, name }) {
  return (
    <div className="group flex flex-col items-center gap-2.5" title={name}>
      <svg
        viewBox="0 0 24 24"
        className="h-7 w-7 fill-mute transition-all duration-300 group-hover:scale-110 group-hover:fill-oxide"
        aria-label={name}
      >
        <path d={icon.path} />
      </svg>
      <span className="font-mono text-[10.5px] tracking-wider text-mute transition-colors group-hover:text-sub">
        {name}
      </span>
    </div>
  )
}

export default function TechStack() {
  return (
    <section className="relative border-b border-line">
      <div className="mx-auto max-w-7xl px-5 py-14 lg:px-8">
        <Reveal>
          <div className="eyebrow mb-9 justify-center">Built on proven technology</div>
          <div className="flex flex-wrap items-start justify-center gap-x-10 gap-y-7">
            {LOGOS.map((l) => (
              <Icon key={l.name} {...l} />
            ))}
          </div>
          <div className="mt-7 flex flex-wrap items-center justify-center gap-2.5">
            {SIGILS.map((s) => (
              <span
                key={s}
                className="rounded-md border border-line bg-panel px-2.5 py-1 font-mono text-[11px] text-mute transition-colors hover:border-oxide/40 hover:text-oxide"
              >
                {s}
              </span>
            ))}
          </div>
        </Reveal>
      </div>
    </section>
  )
}
