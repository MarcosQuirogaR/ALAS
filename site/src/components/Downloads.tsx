import { withBase } from '../lib/base'
import { useReleases } from '../lib/useRelease'

const REPO_URL = 'https://github.com/MarcosQuirogaR/ALAS'
const RELEASES_URL = `${REPO_URL}/releases`
const RELEASE_VERSION = 'v1.2.0'
const RELEASE_URL = `${RELEASES_URL}/tag/${RELEASE_VERSION}`

export default function Downloads() {
  const releases = useReleases(3)
  const release = releases?.find((item) => item.tag === RELEASE_VERSION)

  const windowsUrl = release?.windowsAssetUrl ?? RELEASE_URL
  const linuxUrl = release?.linuxAssetUrl ?? RELEASE_URL

  return (
    <section id="download" className="border-b border-rule bg-raised/40">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Downloads</p>
        <h2 className="mt-6 text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
          ALAS {RELEASE_VERSION}
        </h2>
        <p className="mt-4 max-w-[58ch] text-[0.96rem] leading-[1.65] text-fg">
          Native Rust packages for Windows and Linux. Each archive includes the executable,
          release manifest, and SHA-256 checksum.
        </p>

        <div className="mt-8 grid gap-5 sm:grid-cols-2">
          <div className="flex flex-col border border-rule bg-raised p-7">
            <h3 className="text-[1.05rem] font-bold text-fg-strong">Windows</h3>
            <p className="mt-3 flex-1 text-[0.92rem] leading-[1.6] text-fg-dim">
              Portable x86-64 package for Windows 10 and later.
            </p>
            <a
              href={windowsUrl}
              className="mt-7 bg-accent px-6 py-3.5 text-center text-[0.92rem] font-semibold text-base transition-colors hover:bg-accent-bright"
            >
              Download for Windows
            </a>
          </div>

          <div className="flex flex-col border border-rule bg-raised p-7">
            <h3 className="text-[1.05rem] font-bold text-fg-strong">Linux</h3>
            <p className="mt-3 flex-1 text-[0.92rem] leading-[1.6] text-fg-dim">
              Portable x86-64 package for current glibc-based distributions.
            </p>
            <a
              href={linuxUrl}
              className="mt-7 border border-rule-strong px-6 py-3.5 text-center text-[0.92rem] font-semibold text-fg-strong transition-colors hover:border-accent hover:text-accent-bright"
            >
              Download for Linux
            </a>
          </div>
        </div>

        <div className="mt-8 flex flex-wrap gap-x-6 gap-y-2 text-[0.86rem] text-fg-dim">
          <a href={RELEASE_URL} className="prose-link">
            Release notes and checksums →
          </a>
          <a href={withBase('docs/installation/')} className="prose-link">
            Installation guide →
          </a>
        </div>
      </div>
    </section>
  )
}
