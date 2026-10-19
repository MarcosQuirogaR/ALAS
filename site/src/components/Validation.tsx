import { useEffect, useState } from 'react'
import { withBase } from '../lib/base'

type ValidationFigure = {
  file: string
  title: string
  caption: string
  alt: string
}

type Summary = Record<string, unknown>

type Manifest = {
  figures: ValidationFigure[]
  summary: Summary | null
}

function str(v: unknown): string {
  return typeof v === 'string' ? v : ''
}

/** Accepts a bare array of figures or {figures, summary}; null if unusable. */
function parseManifest(raw: unknown): Manifest | null {
  const list = Array.isArray(raw)
    ? raw
    : raw && typeof raw === 'object' && Array.isArray((raw as { figures?: unknown }).figures)
      ? (raw as { figures: unknown[] }).figures
      : null
  if (!list) return null

  const figures: ValidationFigure[] = []
  for (const item of list) {
    if (!item || typeof item !== 'object') continue
    const o = item as Record<string, unknown>
    const file = str(o.file)
    if (!file) continue
    const title = str(o.title)
    figures.push({
      file,
      title,
      caption: str(o.caption),
      alt: str(o.alt) || title || file,
    })
  }
  if (figures.length === 0) return null

  const summaryRaw = Array.isArray(raw) ? null : (raw as { summary?: unknown }).summary
  const summary =
    summaryRaw && typeof summaryRaw === 'object' && !Array.isArray(summaryRaw)
      ? (summaryRaw as Summary)
      : null
  return { figures, summary }
}

/** Curated headline statistics; anything else in the summary stays on the docs page. */
const HEADLINES: { key: string; label: string; unit: string; note?: (s: Summary) => string }[] = [
  {
    key: 'oew_median_abs_error_pct',
    label: 'Median OEW error',
    unit: ' %',
    note: (s) =>
      `${s.presets_with_published_oew ?? '?'} aircraft, ${s.oew_within_5_pct ?? '?'} within 5 %`,
  },
  {
    key: 'range_mean_abs_error_pct',
    label: 'Payload-range corners',
    unit: ' %',
    note: (s) => `mean, ${s.range_comparisons ?? '?'} chart readings`,
  },
  {
    key: 'cruise_ld_mean_abs_error_pct',
    label: 'Cruise L/D',
    unit: ' %',
    note: () => 'mean vs published estimates',
  },
  {
    key: 'lfl_mean_abs_error_pct',
    label: 'Landing field length',
    unit: ' %',
    note: () => 'weakest result, over-predicted',
  },
]

function summaryRows(summary: Summary | null): { label: string; value: string; note: string }[] {
  if (!summary) return []
  return HEADLINES.filter((h) => typeof summary[h.key] === 'number').map((h) => ({
    label: h.label,
    value: `${summary[h.key]}${h.unit}`,
    note: h.note ? h.note(summary) : '',
  }))
}

/** The landing page shows only the most telling figures; the rest live in the docs. */
const FEATURED: Record<string, string> = {
  'validation-oew.png': 'Model vs published operating empty mass.',
  'validation-scope.png': 'Registered inputs vs independent model outputs.',
}

function firstSentence(t: string): string {
  const m = t.match(/^.*?[.;](\s|$)/)
  const out = (m ? m[0] : t).trim()
  return out.length > 140 ? out.slice(0, 137) + '...' : out
}

function pickFeatured(figures: ValidationFigure[]): ValidationFigure[] {
  const hit = figures.filter((f) => f.file in FEATURED)
  const chosen = hit.length >= 2 ? hit : figures.slice(0, 2)
  return chosen.map((f) => ({ ...f, caption: FEATURED[f.file] ?? firstSentence(f.caption) }))
}

function figureSrc(file: string): string {
  return /^(https?:)?\/\//.test(file) ? file : withBase(`validation/${file.replace(/^\/+/, '')}`)
}

export default function Validation() {
  const [manifest, setManifest] = useState<Manifest | null>(null)

  useEffect(() => {
    let cancelled = false
    fetch(withBase('validation/manifest.json'))
      .then((r) => {
        const type = r.headers.get('content-type') ?? ''
        // A static host may answer a missing file with the HTML shell.
        return r.ok && !type.includes('text/html') ? r.json() : null
      })
      .then((data) => {
        if (!cancelled) setManifest(parseManifest(data))
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [])

  if (!manifest) return null
  const rows = summaryRows(manifest.summary)

  return (
    <section id="validation" className="border-b border-rule">
      <div className="mx-auto max-w-[68rem] px-6 py-16 sm:py-20">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between sm:gap-10">
          <div>
            <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
              Validation
            </h2>
            <p className="mt-2 text-[0.96rem] text-fg-dim">
              Model results for the registered presets compared with published values.
            </p>
          </div>
          <a href={withBase('docs/validation/')} className="prose-link text-[0.92rem]">
            All figures and method →
          </a>
        </div>

        {rows.length > 0 && (
          <dl className="mt-10 grid gap-px border border-rule bg-rule sm:grid-cols-2 lg:grid-cols-4">
            {rows.map((r) => (
              <div key={r.label} className="bg-base p-5">
                <dt className="font-mono text-[0.68rem] uppercase tracking-[0.1em] text-fg-dim">
                  {r.label}
                </dt>
                <dd className="mt-1.5 font-mono text-[1.5rem] text-fg-strong tabular-nums">
                  {r.value}
                </dd>
                {r.note && (
                  <dd className="mt-1 text-[0.78rem] leading-snug text-fg-dim">{r.note}</dd>
                )}
              </div>
            ))}
          </dl>
        )}
        <p className="mt-3 font-mono text-[0.72rem] text-fg-dim">
          Error = (model - published) / published, absolute values.
        </p>

        <div className="mt-10 grid grid-cols-1 gap-8 md:grid-cols-2">
          {pickFeatured(manifest.figures).map((f) => (
            <figure key={f.file} className="min-w-0">
              <a
                href={figureSrc(f.file)}
                className="block border border-rule bg-raised transition-colors hover:border-accent"
              >
                <img
                  src={figureSrc(f.file)}
                  alt={f.alt}
                  loading="lazy"
                  className="h-auto w-full object-contain"
                />
              </a>
              <figcaption className="mt-2.5 text-[0.84rem] leading-snug text-fg-dim">
                {f.caption}
              </figcaption>
            </figure>
          ))}
        </div>
      </div>
    </section>
  )
}
