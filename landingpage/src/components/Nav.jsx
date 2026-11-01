import { useEffect, useState } from 'react'

const LINKS = [
  ['Features', '#features'],
  ['3D', '#threed'],
  ['Architecture', '#architecture'],
  ['Code', '#code'],
  ['Console', '#console'],
  ['Benchmarks', '#benchmarks'],
  ['Docs', '/docs.html'],
]

export function Logo({ className = 'h-7 w-7' }) {
  return (
    <svg viewBox="284.00 279.50 679.00 679.00" className={className} aria-hidden="true">
      <path d="M 893.50,445.50 Q 897.00,449.00 884.00,439.50 Q 890.00,442.00 872.50,437.50 Q 878.00,437.00 862.00,440.50 Q 867.00,438.00 820.50,478.00 Q 857.00,443.00 593.50,661.50 Q 784.00,513.00 400.50,813.00 Q 403.00,810.00 396.50,819.50 Q 398.00,816.00 395.00,830.00 Q 395.00,823.00 397.00,841.50 Q 395.00,837.00 406.50,853.50 Q 399.00,846.00 427.00,871.50 Q 414.00,861.00 451.50,889.50 Q 440.00,882.00 477.50,904.50 Q 463.00,897.00 501.50,916.00 Q 492.00,912.00 533.00,926.50 Q 511.00,920.00 570.50,935.50 Q 555.00,933.00 599.00,939.00 Q 586.00,938.00 625.50,940.00 Q 612.00,940.00 655.50,938.50 Q 639.00,940.00 687.50,934.00 Q 672.00,937.00 716.50,927.00 Q 703.00,931.00 753.00,913.00 Q 730.00,923.00 789.50,895.00 Q 776.00,903.00 816.00,877.50 Q 803.00,887.00 836.50,861.50 Q 829.00,868.00 860.00,838.00 Q 844.00,855.00 890.50,800.00 Q 876.00,821.00 916.50,755.00 Q 905.00,779.00 932.00,719.00 Q 928.00,731.00 939.50,692.00 Q 936.00,707.00 945.00,659.50 Q 943.00,677.00 947.00,621.50 Q 947.00,642.00 945.50,586.50 Q 947.00,601.00 941.50,559.00 Q 944.00,572.00 930.50,520.50 Q 939.00,546.00 909.50,472.00 Q 922.00,495.00 893.50,445.50 Z M 833.50,569.50 Q 831.00,565.00 838.00,581.50 Q 836.00,574.00 841.50,601.50 Q 840.00,589.00 842.50,629.50 Q 843.00,614.00 839.00,661.00 Q 842.00,645.00 830.50,692.50 Q 836.00,677.00 817.00,723.00 Q 825.00,708.00 798.00,752.50 Q 809.00,738.00 775.50,778.00 Q 787.00,767.00 748.00,800.00 Q 764.00,789.00 717.00,818.00 Q 732.00,811.00 686.50,829.50 Q 702.00,825.00 655.50,836.00 Q 671.00,834.00 622.00,837.50 Q 640.00,838.00 587.50,833.50 Q 604.00,837.00 560.00,825.50 Q 571.00,830.00 540.50,814.00 Q 549.00,821.00 528.50,801.00 Q 532.00,807.00 524.00,787.50 Q 525.00,795.00 525.00,774.00 Q 523.00,780.00 531.50,763.00 Q 527.00,768.00 592.50,715.00 Q 536.00,758.00 722.00,614.50 Q 649.00,672.00 801.00,555.00 Q 795.00,557.00 811.50,553.50 Q 807.00,553.00 820.00,556.00 Q 816.00,554.00 827.50,561.50 Q 824.00,558.00 833.50,569.50 Z M 811.50,360.00 Q 825.00,371.00 787.00,342.00 Q 798.00,349.00 762.00,328.00 Q 776.00,335.00 734.50,316.00 Q 748.00,321.00 707.50,307.50 Q 721.00,311.00 678.00,301.50 Q 694.00,304.00 631.50,298.50 Q 662.00,299.00 578.00,301.50 Q 601.00,298.00 543.50,308.00 Q 555.00,305.00 514.00,317.50 Q 532.00,311.00 474.00,335.50 Q 496.00,324.00 434.00,360.00 Q 452.00,347.00 397.00,391.50 Q 416.00,373.00 367.50,423.50 Q 378.00,410.00 349.00,449.50 Q 357.00,437.00 336.00,471.50 Q 341.00,462.00 324.50,496.50 Q 331.00,481.00 311.50,536.00 Q 318.00,512.00 303.00,574.50 Q 305.00,560.00 300.50,608.00 Q 301.00,589.00 303.00,653.50 Q 300.00,627.00 309.00,692.00 Q 306.00,680.00 317.00,718.50 Q 312.00,704.00 332.50,753.50 Q 322.00,733.00 346.00,776.50 Q 343.00,774.00 354.00,781.00 Q 349.00,779.00 366.50,782.00 Q 359.00,783.00 404.50,757.00 Q 374.00,781.00 437.50,729.50 Q 435.00,733.00 440.00,721.00 Q 440.00,726.00 433.00,706.00 Q 440.00,716.00 421.50,687.00 Q 426.00,696.00 413.00,665.50 Q 417.00,678.00 407.00,639.00 Q 409.00,653.00 405.00,613.00 Q 405.00,625.00 406.00,592.50 Q 405.00,601.00 412.50,563.00 Q 407.00,584.00 425.50,526.00 Q 418.00,542.00 443.00,495.50 Q 433.00,510.00 466.00,467.50 Q 453.00,481.00 496.50,441.50 Q 479.00,454.00 532.00,420.50 Q 514.00,429.00 559.50,409.00 Q 550.00,412.00 581.00,403.50 Q 569.00,406.00 602.00,400.00 Q 593.00,401.00 624.00,399.00 Q 611.00,399.00 655.00,402.00 Q 637.00,399.00 683.00,408.50 Q 673.00,405.00 700.00,415.50 Q 693.00,412.00 716.00,425.50 Q 707.00,419.00 733.00,440.50 Q 725.00,432.00 746.00,456.00 Q 741.00,449.00 754.00,464.50 Q 751.00,463.00 761.50,466.00 Q 757.00,466.00 768.00,465.00 Q 766.00,466.00 799.00,435.00 Q 770.00,464.00 830.00,401.50 Q 828.00,406.00 832.50,393.00 Q 832.00,397.00 832.50,386.00 Q 833.00,389.00 828.50,377.00 Q 832.00,383.00 811.50,360.00 Z" fill="currentColor" fillRule="evenodd" />
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
