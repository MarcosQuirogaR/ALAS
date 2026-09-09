import { useEffect, useState } from 'react'
import { withBase } from '../lib/base'

const CAPABILITIES = [
  'Parametric airframe sizing & geometry optimization',
  'Multi-point aerodynamics & transonic section diagnostics (MSES)',
  'Wingbox structural estimation & rib-spacing analysis',
  'Turbofan cycle analysis & engine matching',
  'Mass breakdown, CG envelope & longitudinal stability margins',
  'Flown mission simulation (climb, cruise, descent, reserves)',
  'Stage execution reporting & external solver diagnostics in progress',
  'UAS (unmanned aircraft systems) configurations (work in progress)',
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
      caption: 'Transonic section flow diagnostics (MSES coupled Euler/boundary-layer)',
    },
    {
      src: withBase('demo/cabin.png'),
      caption: 'Cabin and payload arrangement preview',
    },
    {
      src: withBase('demo/mission-route.png'),
      caption: 'Flown mission trajectory, tracking fuel burn and aircraft mass',
    },
  ]

  return (
    <section id="overview" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Overview</p>

        <div className="mt-6 grid gap-x-12 gap-y-8 lg:grid-cols-[1.15fr_1fr]">
          <div>
            <h2 className="font-serif text-[1.85rem] font-semibold leading-[1.2] tracking-[-0.015em] text-fg-strong">
              Coupled multidisciplinary stages for preliminary design
            </h2>
            <p className="mt-5 max-w-[52ch] text-[0.98rem] leading-[1.65] text-fg">
              You specify the operational requirements. ALAS explores the geometric design space
              using preliminary engineering models, evaluating candidate airframes across aerodynamics,
              structures, propulsion, and trajectory simulation. Stage execution diagnostics report
              solver status and convergence directly.
            </p>
            <p className="mt-3 max-w-[52ch] text-[0.86rem] leading-[1.6] text-fg-dim">
              Intended for engineering exploration and stage diagnostics. Sized configurations
              represent preliminary approximations, not flight-certified or manufacturer-validated
              aircraft, with no exact runtime guarantees.
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
