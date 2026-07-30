import { useEffect, useState } from 'react'

const COMPUTES = [
  'Aerodynamics and drag',
  'Transonic section flow',
  'Wingbox structures',
  'Engine cycle',
  'Weight, balance and stability',
  'A complete flown mission',
]

const FIGURES = [
  {
    src: '/demo/transonic.png',
    caption: 'Transonic flow around the wing section',
  },
  {
    src: '/demo/cabin.png',
    caption: 'Cabin and payload layout',
  },
  {
    src: '/demo/mission-route.png',
    caption: 'Flown mission, coloured by aircraft mass',
  },
]

/** Full-screen preview of one figure, dismissible via backdrop click, the
 *  close button, or Escape. */
function Lightbox({
  figure,
  onClose,
}: {
  figure: (typeof FIGURES)[number]
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
  const [expanded, setExpanded] = useState<(typeof FIGURES)[number] | null>(null)

  return (
    <section id="overview" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Overview</p>

        <div className="mt-6 grid gap-x-12 gap-y-8 lg:grid-cols-[1.15fr_1fr]">
          <div>
            <h2 className="font-serif text-[1.85rem] font-semibold leading-[1.2] tracking-[-0.015em] text-fg-strong">
              One run, from requirements to a preliminary design
            </h2>
            <p className="mt-5 max-w-[52ch] text-[1rem] leading-[1.65] text-fg">
              You describe the mission. ALAS searches the geometry against
              it, then re-analyses the winning aircraft properly, so the
              numbers you get back have been checked, not just sized.
            </p>
          </div>

          <ul className="grid grid-cols-1 gap-y-2.5 self-center sm:grid-cols-2 lg:grid-cols-1 lg:gap-y-3">
            {COMPUTES.map((c) => (
              <li key={c} className="flex items-baseline gap-3 text-[0.92rem] text-fg">
                <span className="h-1 w-1 shrink-0 translate-y-[-0.2em] bg-accent" />
                {c}
              </li>
            ))}
          </ul>
        </div>

        <div className="mt-14 grid grid-cols-1 gap-6 sm:grid-cols-3">
          {FIGURES.map((f) => (
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
