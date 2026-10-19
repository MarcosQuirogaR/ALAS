import { withBase } from '../lib/base'
import { CURRENT } from '../lib/releases'

export default function Hero() {
  return (
    <section id="top" className="relative isolate overflow-hidden border-b border-rule">
      {/* Render by Antón Ochoa Castro and Aarón Pérez Pardiñas. See
          Acknowledgements. Positioned so the aircraft sits in the right-hand
          third, which the veil deliberately leaves clear. */}
      <img
        src={withBase('brand/hero.jpg')}
        alt=""
        aria-hidden="true"
        className="absolute inset-0 -z-20 h-full w-full object-cover object-[72%_42%]"
      />
      <div aria-hidden="true" className="hero-veil absolute inset-0 -z-10" />
      <div aria-hidden="true" className="hero-fade absolute inset-0 -z-10" />

      <div className="mx-auto max-w-[68rem] px-6 pb-16 pt-20 sm:pb-24 sm:pt-28 lg:pb-32 lg:pt-36">
        <h1 className="max-w-[28ch] text-[2.2rem] font-bold leading-[1.15] tracking-[-0.02em] text-fg-strong sm:text-[3rem]">
          Aircraft Layout, Analysis and Sizing
        </h1>

        <p className="mt-6 max-w-[60ch] text-[1.1rem] leading-[1.5] text-fg">
          Conceptual design and multidisciplinary analysis of transport aircraft: geometry, aerodynamics, structures, propulsion, mass and mission.
        </p>

        <div className="mt-9 flex flex-col gap-3.5 sm:flex-row sm:items-center">
          <a
            href={withBase('#download')}
            className="inline-flex items-center justify-center bg-accent px-6 py-3.5 text-[0.92rem] font-semibold text-base transition-colors hover:bg-accent-bright"
          >
            Download {CURRENT.tag}
          </a>

          <a
            href={withBase('docs/')}
            className="inline-flex items-center justify-center border border-rule-strong px-6 py-3.5 text-[0.92rem] font-semibold text-fg-strong transition-colors hover:border-accent hover:text-accent-bright"
          >
            Documentation
          </a>
        </div>

        <ul className="mt-6 flex flex-wrap gap-x-6 gap-y-1 font-mono text-[0.72rem] uppercase tracking-[0.1em] text-fg-dim">
          <li>Open source, AGPL-3.0-or-later</li>
          <li>Windows and Linux</li>
        </ul>
      </div>
    </section>
  )
}
