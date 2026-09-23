import { withBase } from '../lib/base'

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
        <div className="flex flex-wrap items-center gap-3">
          <p className="section-mark">Aircraft preliminary design</p>
          <span className="inline-flex items-center border border-rule bg-raised px-2.5 py-0.5 font-mono text-[0.72rem] text-fg-dim">
            v1.2.0 · Windows and Linux
          </span>
        </div>

        <h1 className="mt-6 max-w-[28ch] text-[2.2rem] font-bold leading-[1.15] tracking-[-0.02em] text-fg-strong sm:text-[3rem]">
          Aircraft preliminary design and multidisciplinary analysis
        </h1>

        <p className="mt-6 max-w-[50ch] text-[1.02rem] leading-[1.65] text-fg">
          ALAS couples parametric geometry sizing, vortex-lattice aerodynamics, wingbox structural
          estimation, turbofan thermodynamic cycles, and trajectory simulation to evaluate transport
          aircraft against mission requirements.
        </p>

        <div className="mt-9 flex flex-col gap-3.5 sm:flex-row sm:items-center">
          <a
            href={withBase('#download')}
            className="inline-flex items-center justify-center bg-accent px-6 py-3.5 text-[0.92rem] font-semibold text-base transition-colors hover:bg-accent-bright"
          >
            Download v1.2.0
          </a>

          <a
            href={withBase('docs/')}
            className="inline-flex items-center justify-center border border-rule-strong px-6 py-3.5 text-[0.92rem] font-semibold text-fg-strong transition-colors hover:border-accent hover:text-accent-bright"
          >
            Documentation
          </a>
        </div>

        <p className="mt-6 font-mono text-[0.72rem] uppercase tracking-[0.1em] text-fg-dim">
          Open source (AGPL-3.0-or-later) · Native Rust binaries for Windows and Linux
        </p>
      </div>
    </section>
  )
}
