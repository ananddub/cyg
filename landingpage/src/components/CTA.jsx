import Reveal from './Reveal.jsx'

export default function CTA() {
  return (
    <section className="relative overflow-hidden border-t border-line">
      <Reveal className="relative mx-auto max-w-4xl px-5 py-28 text-center lg:py-36">
        <h2 className="font-display text-4xl font-bold leading-[1.08] text-fg sm:text-[48px]">
          Run your first room <span className="text-grad">today.</span>
        </h2>
        <p className="mx-auto mt-7 max-w-xl text-[15px] leading-relaxed text-sub">
          Open source, Apache-2.0. Self-host on anything from a $10/mo VPS upward —
          one process is the whole stack.
        </p>
        <div className="mt-11 flex flex-wrap items-center justify-center gap-4">
          <a href="https://github.com/ananddub/cyg" target="_blank" rel="noopener" className="btn-primary btn-lg">
            Star on GitHub
          </a>
          <a href="#quickstart" className="btn-ghost btn-lg">
            Read the quick start
          </a>
        </div>
      </Reveal>
    </section>
  )
}
