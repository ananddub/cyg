import { useRef } from 'react'
import gsap from 'gsap'
import { useGSAP } from '@gsap/react'

/**
 * Scroll-triggered reveal wrapper — fades/slides children in once
 * when they enter the viewport.
 */
export default function Reveal({ children, delay = 0, y = 30, className = '' }) {
  const ref = useRef(null)

  useGSAP(() => {
    gsap.from(ref.current, {
      opacity: 0,
      y,
      duration: 0.95,
      delay,
      ease: 'power3.out',
      scrollTrigger: {
        trigger: ref.current,
        start: 'top 86%',
        once: true,
      },
    })
  }, { scope: ref })

  return (
    <div ref={ref} className={className}>
      {children}
    </div>
  )
}
