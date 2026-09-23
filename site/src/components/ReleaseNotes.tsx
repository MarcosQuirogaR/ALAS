import { formatDate, useReleases } from '../lib/useRelease'

const RELEASES_URL = 'https://github.com/MarcosQuirogaR/ALAS/releases'

// Keep the landing page's release copy local and deliberate. GitHub release
// bodies can contain historical branding or editorial notes that do not belong
// in the current product site.
const RELEASE_SUMMARIES: Record<string, string[]> = {
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
