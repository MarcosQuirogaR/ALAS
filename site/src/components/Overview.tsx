import { useEffect, useState } from 'react'
import { withBase } from '../lib/base'

const CAPABILITIES = [
  'Parametric geometry sizing and numerical airframe optimization',
  'Lifting-line and vortex-lattice aerodynamics with empirical drag build-up',
  'Optional 2D transonic section diagnostics (via user-supplied MSES)',
  'Wingbox structural sizing and analytical rib-spacing estimation',
  'Turbofan thermodynamic cycle matching and thrust-lapse modeling',
  'Mass breakdown, center-of-gravity envelope, and longitudinal static margin',
  'Trajectory simulation across climb, cruise, descent, and reserves',
  'Stage status tracking with explicit diagnostics for unavailable external tools',
]

type Figure = {
  src: string
  caption: string
}

/** Full-screen preview of one figure, dismissible via backdrop click, the
 *  close button, or Escape. */
function Lightbox({
  figure,
  onClose,
}: {
  figure: Figure
  onClose: () => void
}) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-base/95 p-6"
      onClick={onClose}
    >
      <button
        onClick={onClose}
        aria-label="Close"
        className="absolute right-6 top-6 text-[1.6rem] leading-none text-fg-dim transition-colors hover:text-fg-strong"
      >
        ✕
      </button>
      <figure className="max-h-full max-w-[90rem]" onClick={(e) => e.stopPropagation()}>
        <img
          src={figure.src}
          alt={figure.caption}
          className="max-h-[80vh] w-auto object-contain"
        />
        <figcaption className="mt-3 text-center text-[0.86rem] text-fg-dim">
          {figure.caption}
        </figcaption>
      </figure>
    </div>
  )
}

export default function Overview() {
  const [expanded, setExpanded] = useState<Figure | null>(null)

  const figures: Figure[] = [
    {
      src: withBase('demo/transonic.png'),
      caption: 'Transonic section Mach contours (via external MSES when installed)',
    },
    {
      src: withBase('demo/cabin.png'),
      caption: 'Cabin seating and payload arrangement preview',
    },
    {
      src: withBase('demo/mission-route.png'),
      caption: 'Mission trajectory simulation tracking fuel burn and aircraft mass',
    },
  ]

  return (
    <section id="overview" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-16 sm:py-20">
        <p className="section-mark">Overview</p>

        <div className="mt-6 grid gap-x-12 gap-y-8 lg:grid-cols-[1.15fr_1fr]">
          <div>
            <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
              Coupled multidisciplinary analysis pipeline
            </h2>
            <p className="mt-4 max-w-[52ch] text-[0.96rem] leading-[1.65] text-fg">
              ALAS automates preliminary aircraft sizing around mission requirements: design payload,
              range, cruise speed, and field limits. It evaluates candidate airframes across coupled
              disciplines, tracking mass properties, aerodynamic polars, structural wingbox limits,
              and fuel consumption along simulated flight trajectories.
            </p>
            <p className="mt-3 max-w-[52ch] text-[0.86rem] leading-[1.6] text-fg-dim">
              Calculations provide conceptual estimates for early trade studies. Sized configurations
              are not flight-certified or manufacturer-validated aircraft, and ALAS output has not been
              validated against measured aircraft performance. External solvers require separate
              installation.
            </p>
          </div>

          <ul className="grid grid-cols-1 gap-y-2.5 self-center sm:grid-cols-2 lg:grid-cols-1 lg:gap-y-3">
            {CAPABILITIES.map((c) => (
              <li key={c} className="flex items-baseline gap-3 text-[0.9rem] text-fg">
                <span className="h-1 w-1 shrink-0 translate-y-[-0.2em] bg-accent" />
                {c}
              </li>
            ))}
          </ul>
        </div>

        <div className="mt-14 grid grid-cols-1 gap-6 sm:grid-cols-3">
          {figures.map((f) => (
            <figure key={f.src}>
              <button
                onClick={() => setExpanded(f)}
                aria-label={`Expand: ${f.caption}`}
                className="block w-full cursor-zoom-in border border-rule transition-colors hover:border-accent"
              >
                <img
                  src={f.src}
                  alt={f.caption}
                  loading="lazy"
                  className="aspect-[3/2] w-full object-contain"
                />
              </button>
              <figcaption className="mt-2.5 text-[0.78rem] leading-snug text-fg-dim">
                {f.caption}
              </figcaption>
            </figure>
          ))}
        </div>
      </div>

      {expanded && <Lightbox figure={expanded} onClose={() => setExpanded(null)} />}
    </section>
  )
}
