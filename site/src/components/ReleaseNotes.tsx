import { formatDate, useReleases } from '../lib/useRelease'

const RELEASES_URL = 'https://github.com/MarcosQuirogaR/ALAS/releases'

// Keep the landing page's release copy local and deliberate. GitHub release
// bodies can contain historical branding or editorial notes that do not belong
// in the current product site.
const RELEASE_SUMMARIES: Record<string, string[]> = {
  'v1.3.2': [
    'Boeing 747-400 upper-deck hump and partial upper deck, with business class upstairs and first class in the nose; the model OEW is +13.8 % over the published 178,755 kg, which exposes a FLOPS over-prediction.',
    'Measured nose geometry on seven presets (A320-200, A220-300, A340-300, A380-800, B787-9, B747-400 and the AVE body, a 777-9 stand-in) from the manufacturers' drawings, and the nose length and tip height of the DC-10; the nose model gains a convex radome and wider parameter ranges.',
    'Longer noses shorten the generic cabin and move the model centre of gravity aft: the A220-300, AVE and B747-400 now report a minimum nose-gear load finding, and static margins stay unrealistic.',
  ],
  'v1.3.1': [
    'Four new presets: Embraer E195-E2 (model OEW within 0.3 % of the 35,700 kg reference), COMAC C919 (-3.1 %, airframe data from a secondary source), Boeing 747-400 (+9.3 %, smoothed upper-deck hump) and Airbus A400M (cargo transport, no passenger cabin).',
    'Parametric nose geometry, opt-in through six nose fields; an unset nose keeps the legacy shape.',
    'Test robustness on CI hosts without navigation data or a writable shared temporary directory.',
  ],
  'v1.3.0': [
    'Hard MTOW as the default for presets, with takeoff fuel capped at usable tank capacity, and hard constraints throughout.',
    'Clean-sheet design from a preset-less brief, a load and trim CG sheet, and an all-evaluation optimization history.',
    'Rotation forward-CG model with a geometry-derived tail authority, and an MSC Nastran wing-box validation.',
  ],
  'v1.2.0': [
    'One optimiser: L-SHADE differential evolution under the epsilon-constrained method, deterministic for a given seed.',
    'Wing and fuselage layout constraints, revised mass, fuel and mission models, and a first-start download of the optional OpenVSP preview runtime.',
    'Windows and Linux packages built and checked by the release workflow from the tagged commit.',
  ],
  'v1.1.0': [
    'Native Rust desktop packages for Windows and Linux.',
    'Portable archives include a release manifest and a sibling SHA-256 file.',
  ],
}

export default function ReleaseNotes() {
  const releases = useReleases(3)

  return (
    <section id="releases" className="border-b border-rule bg-raised/40">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Release notes</p>

        <div className="mt-6 flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
          <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
            Release history
          </h2>
          <a href={RELEASES_URL} className="prose-link text-[0.92rem]">
            Releases on GitHub →
          </a>
        </div>

        {releases === null ? (
          <p className="mt-10 text-[0.92rem] text-fg-dim">
            Release notes are published on{' '}
            <a href={RELEASES_URL} className="prose-link">
              GitHub
            </a>
            .
          </p>
        ) : (
          <ol className="mt-10 border-t border-rule">
            {releases.map((r) => {
              const lines = RELEASE_SUMMARIES[r.tag] ?? []
              return (
                <li
                  key={r.tag}
                  className="grid gap-x-10 gap-y-3 border-b border-rule py-7 sm:grid-cols-[10rem_1fr]"
                >
                  <div>
                    <div className="font-mono text-[0.9rem] text-accent">{r.tag}</div>
                    <div className="mt-1 font-mono text-[0.72rem] text-fg-dim">
                      {formatDate(r.published)}
                    </div>
                  </div>
                  <div>
                    <h3 className="text-[1rem] font-semibold text-fg-strong">{r.name}</h3>
                    {lines.length > 0 && (
                      <ul className="mt-2.5 space-y-1.5">
                        {lines.map((l, i) => (
                          <li
                            key={i}
                            className="text-[0.9rem] leading-[1.6] text-fg-dim"
                          >
                            {l}
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                </li>
              )
            })}
          </ol>
        )}
      </div>
    </section>
  )
}
