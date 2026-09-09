import { withBase } from '../lib/base'
import { useReleases } from '../lib/useRelease'

const REPO_URL = 'https://github.com/MarcosQuirogaR/ALAS'
const RELEASES_URL = `${REPO_URL}/releases`
const V1_RELEASE_URL = `${RELEASES_URL}/tag/v1.0.0`

export default function Downloads() {
  const releases = useReleases(1)
  const latestRelease = releases?.[0]

  return (
    <section id="download" className="border-b border-rule bg-raised/40">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Downloads &amp; Releases</p>
        <h2 className="mt-6 font-serif text-[1.85rem] font-semibold leading-[1.2] tracking-[-0.015em] text-fg-strong">
          Get ALAS
        </h2>

        {/* Release Status Banner */}
        <div className="mt-6 border border-rule bg-raised p-6 text-[0.92rem] leading-[1.62]">
          <div className="flex items-center gap-2 font-mono text-[0.78rem] font-semibold uppercase tracking-[0.1em] text-accent">
            <span className="h-2 w-2 rounded-full bg-accent" />
            <span>Release candidate under verification</span>
          </div>
          <p className="mt-2.5 text-fg">
            A new release candidate is currently under verification. Prebuilt standalone
            binaries will be published once checks are complete; it is not yet available for
            general download.
          </p>
          <p className="mt-2 text-fg-dim">
            The latest public binary distribution is the legacy{' '}
            <a href={V1_RELEASE_URL} className="prose-link font-medium">
              v1.0.0 release (2026-07-29)
            </a>
            {latestRelease?.tag ? ` (repository tag: ${latestRelease.tag})` : ''}.
          </p>
        </div>

        <div className="mt-10 grid gap-5 sm:grid-cols-2">
          {/* Windows */}
          <div className="flex flex-col border border-rule bg-raised p-7">
            <div className="flex items-baseline justify-between gap-4">
              <h3 className="text-[1.1rem] font-semibold text-fg-strong">Windows</h3>
              <span className="font-mono text-[0.72rem] text-fg-dim">
                Legacy binary: v1.0.0
              </span>
            </div>
            <p className="mt-3 flex-1 text-[0.92rem] leading-[1.6] text-fg">
              Legacy standalone Windows executable from the v1.0.0 release (2026-07-29). The upcoming
              candidate is being verified as a standalone <code>alas.exe</code> requiring no separate runtime.
            </p>
            <div className="mt-7 flex flex-col gap-2">
              <a
                href={V1_RELEASE_URL}
                className="bg-accent px-6 py-3.5 text-center font-semibold text-base transition-colors hover:bg-accent-bright"
              >
                View v1.0.0 on GitHub
              </a>
              <span className="text-center font-mono text-[0.72rem] text-fg-dim">
                New candidate under verification
              </span>
            </div>
          </div>

          {/* Platform & Solvers */}
          <div className="flex flex-col border border-rule bg-raised p-7">
            <div className="flex items-baseline justify-between gap-4">
              <h3 className="text-[1.1rem] font-semibold text-fg-strong">Platform &amp; Solvers</h3>
              <span className="font-mono text-[0.72rem] text-fg-dim">
                Local executable
              </span>
            </div>
            <p className="mt-3 flex-1 text-[0.92rem] leading-[1.6] text-fg">
              Core sizing and optimization run locally. Extended external analysis stages (such as
              MSES, OpenVSP, or Nastran) require compatible tools as documented for the distribution;
              ALAS reports a stage unavailable when a tool is absent.
            </p>
            <div className="mt-7 flex flex-col gap-2">
              <a
                href={RELEASES_URL}
                className="border border-rule-strong px-6 py-3.5 text-center font-semibold text-fg-strong transition-colors hover:border-accent hover:text-accent-bright"
              >
                View Releases on GitHub
              </a>
              <span className="text-center font-mono text-[0.72rem] text-fg-dim">
                Tagged release assets and notes
              </span>
            </div>
          </div>
        </div>

        <div className="mt-8 flex flex-col gap-2 text-[0.86rem] leading-[1.6] text-fg-dim">
          <p>
            Windows may display a SmartScreen notice on first execution of untrusted builds; select{' '}
            <span className="text-fg">More info → Run anyway</span>.
          </p>
          <p>
            Need configuration details or solver integration notes? See the{' '}
            <a href={withBase('docs/installation/')} className="prose-link">
              installation guide
            </a>
            {' '}or read the{' '}
            <a href={withBase('docs/running-alas/')} className="prose-link">
              running guide
            </a>
            .
          </p>
        </div>
      </div>
    </section>
  )
}
