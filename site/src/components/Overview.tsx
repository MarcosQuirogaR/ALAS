import { useEffect, useState } from 'react'
import { withBase } from '../lib/base'

const CAPABILITIES: { title: string; body: string }[] = [
  { title: 'Geometry', body: 'Parametric sizing, optional nose and upper deck' },
  { title: 'Optimizer', body: 'L-SHADE, deterministic for a given seed' },
  { title: 'Hard MTOW', body: 'Takeoff fuel capped at tank capacity' },
  { title: 'Aerodynamics', body: 'Vortex lattice with drag build-up' },
  { title: 'Structures', body: 'Wingbox sizing, optional Nastran check' },
  { title: 'Mass and CG', body: 'Load and trim sheet, gear, static margin' },
  { title: 'Mission', body: 'Climb, cruise, descent and reserves' },
  { title: 'External solvers', body: 'AVL, VSPAERO, MSES, OpenFOAM, Nastran (optional)' },
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
      src: withBase('docs/assets/ave-threeview-3d-dark.png'),
      caption: 'AVE reference aircraft, sized geometry',
    },
    {
      src: withBase('docs/assets/ave-cabin-payload-dark.png'),
      caption: 'Cabin and payload layout',
    },
    {
      src: withBase('docs/assets/ave-mission-route-dark.png'),
      caption: 'Mission over a SimBrief route, London Heathrow to Dubai',
    },
  ]

  return (
    <section id="overview" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-16 sm:py-20">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between sm:gap-10">
          <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
            Analyses
          </h2>
          <p className="text-[0.96rem] text-fg-dim sm:text-right">
            Payload, range, speed and field requirements are the inputs; the coupled analyses size the aircraft.
          </p>
        </div>

        <ul className="mt-10 grid grid-cols-1 gap-px border border-rule bg-rule sm:grid-cols-2 lg:grid-cols-4">
          {CAPABILITIES.map((c) => (
            <li key={c.title} className="bg-base p-5">
              <h3 className="text-[0.95rem] font-bold text-fg-strong">{c.title}</h3>
              <p className="mt-1.5 text-[0.84rem] leading-snug text-fg-dim">{c.body}</p>
            </li>
          ))}
        </ul>

        <div className="mt-10 grid grid-cols-1 gap-6 sm:grid-cols-3">
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
