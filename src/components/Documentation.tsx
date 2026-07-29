const ENTRIES = [
  {
    href: '/ALAS-site/docs/installation/',
    title: 'Installation',
    body: 'Getting it running, the optional extras, and the one-time setup for mission analysis.',
  },
  {
    href: '/ALAS-site/docs/user-guide/',
    title: 'User guide',
    body: 'Every page, control and action in the application, with a suggested first session.',
  },
  {
    href: '/ALAS-site/docs/meet-ave/',
    title: 'Worked example',
    body: 'One aircraft carried through the whole pipeline, with its results explained.',
  },
  {
    href: '/ALAS-site/docs/architecture/',
    title: 'How it works inside',
    body: 'The internals: layering, fidelity levels, and what happens during a run.',
  },
]

export default function Documentation() {
  return (
    <section id="documentation" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Documentation</p>

        <div className="mt-6 flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
          <h2 className="font-serif text-[1.85rem] font-semibold leading-[1.2] tracking-[-0.015em] text-fg-strong">
            Learn how to use it
          </h2>
          <a href="/ALAS-site/docs/" className="prose-link text-[0.92rem]">
            Browse the full documentation →
          </a>
        </div>

        <div className="mt-10 grid grid-cols-1 gap-px border border-rule bg-rule sm:grid-cols-2">
          {ENTRIES.map((e) => (
            <a
              key={e.href}
              href={e.href}
              className="group bg-base p-7 transition-colors hover:bg-raised"
            >
              <h3 className="text-[1.02rem] font-semibold text-fg-strong transition-colors group-hover:text-accent-bright">
                {e.title}
              </h3>
              <p className="mt-2 text-[0.9rem] leading-[1.6] text-fg-dim">{e.body}</p>
            </a>
          ))}
        </div>
      </div>
    </section>
  )
}
