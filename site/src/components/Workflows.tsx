import { useState } from 'react'
import { withBase } from '../lib/base'

const PRESETS = [
  'A220-300',
  'A320-200',
  'A340-300',
  'A380-800',
  'A400M',
  'ATR72-600',
  'B747-400',
  'B787-9',
  'C919',
  'DC-10-30',
  'E195-E2',
  'AVE',
]

const WORKFLOWS = [
  {
    title: 'Guided preset',
    body: 'Start from one of twelve registered aircraft at its declared MTOW.',
  },
  {
    title: 'Clean-sheet brief',
    body: 'Give payload, range, speed and field limits. ALAS derives the design start.',
  },
  {
    title: 'Sandbox Mode',
    body: 'Edit wing, tails, fuselage and engines in 3D, then run quick or full analysis.',
  },
]

const SANDBOX_FIGURES = [
  {
    file: 'docs/assets/sandbox-estimates-dark.png',
    caption: 'Sandbox: live estimates',
  },
  {
    file: 'docs/assets/sandbox-fuselage-editor-dark.png',
    caption: 'Fuselage section editor',
  },
  {
    file: 'docs/assets/sandbox-full-analysis-summary-dark.png',
    caption: 'Full Analysis results',
  },
]

function Shot({ file, caption }: { file: string; caption: string }) {
  const [failed, setFailed] = useState(false)
  if (failed) return null
  return (
    <figure className="min-w-0">
      <img
        src={withBase(file)}
        alt={caption}
        loading="lazy"
        onError={() => setFailed(true)}
        className="aspect-[3/2] w-full border border-rule bg-raised object-contain"
      />
      <figcaption className="mt-2.5 text-[0.78rem] leading-snug text-fg-dim">{caption}</figcaption>
    </figure>
  )
}

export default function Workflows() {
  return (
    <section id="workflows" className="border-b border-rule bg-raised/40">
      <div className="mx-auto max-w-[68rem] px-6 py-16 sm:py-20">
        <p className="section-mark">Workflows</p>

        <div className="mt-6 flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between sm:gap-10">
          <div>
            <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
              Three ways to start a design
            </h2>
            <p className="mt-2 text-[0.96rem] text-fg-dim">Pick a preset, a brief, or a blank sandbox.</p>
          </div>
          <a href={withBase('docs/user-guide/')} className="prose-link text-[0.92rem]">
            User guide →
          </a>
        </div>

        <div className="mt-10 grid grid-cols-1 gap-px border border-rule bg-rule lg:grid-cols-3">
          {WORKFLOWS.map((w) => (
            <div key={w.title} className="bg-base p-5">
              <h3 className="text-[0.98rem] font-bold text-fg-strong">{w.title}</h3>
              <p className="mt-2 text-[0.88rem] leading-snug text-fg-dim">{w.body}</p>
            </div>
          ))}
        </div>

        <p className="mt-8 font-mono text-[0.68rem] uppercase tracking-[0.15em] text-fg-dim">
          Registered presets
        </p>
        <ul className="mt-3 flex flex-wrap gap-2">
          {PRESETS.map((p) => (
            <li
              key={p}
              className="border border-rule bg-raised px-2.5 py-1 font-mono text-[0.78rem] text-fg"
            >
              {p}
            </li>
          ))}
        </ul>
        <p className="mt-3 text-[0.8rem] text-fg-dim">
          AVE is a twin-aisle reference design, not a real type. The A400M is cargo only.
        </p>

        <div className="mt-10 grid grid-cols-1 gap-6 sm:grid-cols-3">
          {SANDBOX_FIGURES.map((f) => (
            <Shot key={f.file} {...f} />
          ))}
        </div>
      </div>
    </section>
  )
}
