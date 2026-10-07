import { useState } from 'react'
import { formatDate, PUBLISHED, RELEASES_URL, releaseUrl } from '../lib/releases'

export default function ReleaseNotes() {
  const [open, setOpen] = useState<string | null>(PUBLISHED[0]?.tag ?? null)

  return (
    <section id="releases" className="border-b border-rule bg-raised/40">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
          <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
            Release history
          </h2>
          <a href={RELEASES_URL} className="prose-link text-[0.92rem]">
            Releases on GitHub →
          </a>
        </div>

        <ol className="mt-10 border-t border-rule">
          {PUBLISHED.map((r) => {
            const isOpen = open === r.tag
            return (
              <li
                key={r.tag}
                className="grid gap-x-10 gap-y-3 border-b border-rule py-6 sm:grid-cols-[10rem_1fr]"
              >
                <div>
                  <div className="font-mono text-[0.9rem] text-accent">{r.tag}</div>
                  <div className="mt-1 font-mono text-[0.72rem] text-fg-dim">
                    {formatDate(r.published)}
                  </div>
                </div>
                <div>
                  <h3 className="text-[1rem] font-semibold text-fg-strong">{r.headline}</h3>
                  {isOpen && (
                    <ul className="mt-2 list-disc space-y-1 pl-5 marker:text-accent">
                      {r.details.map((l, i) => (
                        <li key={i} className="text-[0.9rem] leading-[1.5] text-fg-dim">
                          {l}
                        </li>
                      ))}
                    </ul>
                  )}
                  <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1 text-[0.82rem]">
                    <button
                      type="button"
                      aria-expanded={isOpen}
                      onClick={() => setOpen(isOpen ? null : r.tag)}
                      className="prose-link cursor-pointer"
                    >
                      {isOpen ? 'Hide details' : 'Show details'}
                    </button>
                    <a href={releaseUrl(r.tag)} className="prose-link">
                      Full notes on GitHub →
                    </a>
                  </div>
                </div>
              </li>
            )
          })}
        </ol>
      </div>
    </section>
  )
}
