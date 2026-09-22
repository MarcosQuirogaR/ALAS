import { withBase } from '../lib/base'

export default function Documentation() {
  const entries = [
    {
      href: withBase('docs/installation/'),
      title: 'Installation & setup',
      body: 'Binary status, environment setup, optional data packages, and developer source builds.',
    },
    {
      href: withBase('docs/user-guide/'),
      title: 'User guide',
      body: 'Desktop interface controls, configuration inputs, optimizer settings, and result inspection.',
    },
    {
      href: withBase('docs/external-tools/'),
      title: 'External tools guide',
      body: 'Integration and status diagnostics for Athena AVL, OpenVSP/VSPAERO, MSES, and MSC Nastran.',
    },
    {
      href: withBase('docs/meet-ave/'),
      title: 'Worked example (AVE)',
      body: 'Reference twin-aisle transport evaluated through coupled sizing, aerodynamics, and structures.',
    },
    {
      href: withBase('docs/architecture/'),
      title: 'Pipeline architecture',
      body: 'Crate layering, solver interface contracts, execution flow, and stage status handling.',
    },
    {
      href: withBase('docs/reference/formulas/'),
      title: 'Formulas & reference',
      body: 'Aerodynamic polar methods, wingbox sizing equations, propulsion cycles, and configuration schemas.',
    },
  ]

  return (
    <section id="documentation" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-16 sm:py-20">
        <p className="section-mark">Documentation</p>

        <div className="mt-6 flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
          <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
            Technical documentation
          </h2>
          <a href={withBase('docs/')} className="prose-link text-[0.92rem]">
            Documentation index →
          </a>
        </div>

        <div className="mt-10 grid grid-cols-1 gap-px border border-rule bg-rule sm:grid-cols-2 lg:grid-cols-3">
          {entries.map((e) => (
            <a
              key={e.href}
              href={e.href}
              className="group bg-base p-6 transition-colors hover:bg-raised"
            >
              <h3 className="text-[0.98rem] font-bold text-fg-strong transition-colors group-hover:text-accent-bright">
                {e.title}
              </h3>
              <p className="mt-2 text-[0.86rem] leading-[1.6] text-fg-dim">{e.body}</p>
            </a>
          ))}
        </div>
      </div>
    </section>
  )
}
