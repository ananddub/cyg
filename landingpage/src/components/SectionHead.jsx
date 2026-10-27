import Reveal from './Reveal.jsx'

export default function SectionHead({ kicker, title, sub }) {
  return (
    <Reveal className="mx-auto mb-16 max-w-3xl text-center">
      <div className="eyebrow mb-5 justify-center">{kicker}</div>
      <h2 className="font-display text-3xl font-bold leading-[1.12] text-fg sm:text-[40px]">
        {title}
      </h2>
      {sub && <p className="mx-auto mt-5 max-w-2xl text-[15.5px] leading-relaxed text-sub">{sub}</p>}
    </Reveal>
  )
}
