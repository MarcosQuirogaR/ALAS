import { withBase } from '../lib/base'

export default function Documentation() {
  const entries = [
    {
      href: withBase('docs/installation/'),
      title: 'Installation',
      body: 'Setup, optional data, source builds.',
    },
    {
      href: withBase('docs/user-guide/'),
      title: 'User guide',
      body: 'Interface, Sandbox Mode, optimizer, results.',
    },
    {
      href: withBase('docs/external-tools/'),
      title: 'External tools guide',
      body: 'AVL, VSPAERO, MSES, OpenFOAM, Nastran.',
    },
    {
      href: withBase('docs/meet-ave/'),
      title: 'Worked example (AVE)',
      body: 'Reference twin-aisle through the full pipeline.',
    },
    {
      href: withBase('docs/architecture/'),
      title: 'Pipeline architecture',
      body: 'Crates, solver contracts, execution flow.',
    },
    {
      href: withBase('docs/reference/formulas/'),
      title: 'Formulas & reference',
      body: 'Aerodynamic, structural and propulsion methods.',
    },
    {
      href: withBase('docs/design-space-and-optimizer/'),
      title: 'Design space & optimizer',
      body: 'Variables, constraints, L-SHADE search.',
    },
    {
      href: withBase('docs/validation/'),
      title: 'Validation',
      body: 'Presets vs published data, with residuals.',
    },
    {
      href: withBase('docs/gallery/'),
      title: 'Figure gallery',
      body: 'Figures from a full AVE run.',
    },
  ]

  return (
    <section id="documentation" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-16 sm:py-20">
        <p className="section-mark">Documentation</p>

        <div className="mt-6 flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between sm:gap-10">
          <div>
            <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
              Documentation
            </h2>
            <p className="mt-2 text-[0.96rem] text-fg-dim">Guides, methods and reference.</p>
          </div>
          <a href={withBase('docs/')} className="prose-link text-[0.92rem]">
            Documentation index →
          </a>
        </div>

        <div className="mt-10 grid grid-cols-1 gap-px border border-rule bg-rule sm:grid-cols-2 lg:grid-cols-3">
          {entries.map((e) => (
            <a
              key={e.href}
              href={e.href}
              className="group bg-base p-5 transition-colors hover:bg-raised"
            >
              <h3 className="text-[0.98rem] font-bold text-fg-strong transition-colors group-hover:text-accent-bright">
                {e.title}
              </h3>
              <p className="mt-1.5 text-[0.86rem] leading-snug text-fg-dim">{e.body}</p>
            </a>
          ))}
        </div>
      </div>
    </section>
  )
}
