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
    src: '/ALAS-site/demo/transonic.png',
    caption: 'Transonic flow around the wing section',
  },
  {
    src: '/ALAS-site/demo/cabin.png',
    caption: 'Cabin and payload layout',
  },
  {
    src: '/ALAS-site/demo/mission-route.png',
    caption: 'Flown mission, coloured by aircraft mass',
  },
]

export default function Overview() {
  return (
    <section id="overview" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Overview</p>

        <div className="mt-6 grid gap-x-12 gap-y-8 lg:grid-cols-[1.15fr_1fr]">
          <div>
            <h2 className="font-serif text-[1.85rem] font-semibold leading-[1.2] tracking-[-0.015em] text-fg-strong">
              One run, from requirements to a checked design
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
              <div className="border border-rule">
                <img
                  src={f.src}
                  alt={f.caption}
                  loading="lazy"
                  className="aspect-[3/2] w-full object-contain"
                />
              </div>
              <figcaption className="mt-2.5 text-[0.78rem] leading-snug text-fg-dim">
                {f.caption}
              </figcaption>
            </figure>
          ))}
        </div>
      </div>
    </section>
  )
}
