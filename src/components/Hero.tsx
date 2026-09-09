import { withBase } from '../lib/base'
import { useReleases } from '../lib/useRelease'

export default function Hero() {
  const releases = useReleases(1)
  const latest = releases?.[0]

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

      <div className="mx-auto max-w-[68rem] px-6 pb-20 pt-24 sm:pb-28 sm:pt-32 lg:pb-36 lg:pt-40">
        <div className="flex flex-wrap items-center gap-3">
          <p className="section-mark">Aircraft preliminary design</p>
          <span className="inline-flex items-center gap-1.5 border border-rule-strong bg-raised/80 px-2.5 py-0.5 font-mono text-[0.7rem] text-fg-dim">
            <span className="h-1.5 w-1.5 rounded-full bg-accent" />
            <span>Release candidate under verification · Latest public binary: v1.0.0 (2026-07-29)</span>
          </span>
        </div>

        <h1 className="mt-6 max-w-[24ch] font-serif text-[2.5rem] font-semibold leading-[1.12] tracking-[-0.02em] text-fg-strong sm:text-[3.3rem]">
          Preliminary airframe sizing, optimization, and multi-disciplinary analysis.
        </h1>

        <p className="mt-7 max-w-[48ch] text-[1.06rem] leading-[1.62] text-fg">
          ALAS evaluates aircraft configurations against mission requirements using preliminary
          engineering models across aerodynamics, wingbox structures, turbofan cycle propulsion, and
          trajectory simulation. Built for design-space exploration; not flight certification.
        </p>

        <div className="mt-10 flex flex-col gap-4 sm:flex-row sm:items-center">
          <a
            href={withBase('#download')}
            className="inline-flex items-center justify-center gap-3 bg-accent px-7 py-4 font-semibold text-base transition-colors hover:bg-accent-bright"
          >
            Downloads &amp; Releases
            {latest && (
              <span className="font-mono text-[0.72rem] font-normal opacity-85">
                {latest.tag}
              </span>
            )}
          </a>

          <a
            href={withBase('docs/')}
            className="inline-flex items-center justify-center border border-rule-strong px-7 py-4 font-semibold text-fg-strong transition-colors hover:border-accent hover:text-accent-bright"
          >
            Read the documentation
          </a>
        </div>

        <p className="mt-6 font-mono text-[0.72rem] uppercase tracking-[0.13em] text-fg-dim">
          Free · Open source (AGPL-3.0) · Native Rust binary · Windows &amp; Linux
        </p>
      </div>
    </section>
  )
}
