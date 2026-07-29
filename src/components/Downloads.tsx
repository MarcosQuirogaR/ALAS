import { useReleases } from '../lib/useRelease'

const RELEASES_URL = 'https://github.com/MarcosQuirogaR/ALAS/releases'
const WINDOWS_ASSET = `${RELEASES_URL}/latest/download/ALAS-windows.exe`
const LINUX_ASSET = `${RELEASES_URL}/latest/download/ALAS-linux`

export default function Downloads() {
  const releases = useReleases(1)
  const latest = releases?.[0]

  return (
    <section id="download" className="border-b border-rule bg-raised/40">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Downloads</p>
        <h2 className="mt-6 font-serif text-[1.85rem] font-semibold leading-[1.2] tracking-[-0.015em] text-fg-strong">
          Get ALAS
        </h2>

        <div className="mt-10 grid gap-5 sm:grid-cols-2">
          {/* Windows */}
          <div className="flex flex-col border border-rule bg-raised p-7">
            <div className="flex items-baseline justify-between gap-4">
              <h3 className="text-[1.1rem] font-semibold text-fg-strong">Windows</h3>
              <span className="font-mono text-[0.72rem] text-fg-dim">
                {latest?.tag ?? 'n/a'}
                {latest?.windowsAssetMB ? ` · ${latest.windowsAssetMB} MB` : ''}
              </span>
            </div>
            <p className="mt-3 flex-1 text-[0.92rem] leading-[1.6] text-fg">
              Portable executable. Nothing to install and no Python needed,
              since everything is bundled in.
            </p>
            <a
              href={WINDOWS_ASSET}
              className="mt-7 bg-accent px-6 py-3.5 text-center font-semibold text-base transition-colors hover:bg-accent-bright"
            >
              Download
            </a>
          </div>

          {/* Linux */}
          <div className="flex flex-col border border-rule bg-raised p-7">
            <div className="flex items-baseline justify-between gap-4">
              <h3 className="text-[1.1rem] font-semibold text-fg-strong">Linux</h3>
              <span className="font-mono text-[0.72rem] text-fg-dim">
                {latest?.tag ?? 'n/a'}
                {latest?.linuxAssetMB ? ` · ${latest.linuxAssetMB} MB` : ''}
              </span>
            </div>
            <p className="mt-3 flex-1 text-[0.92rem] leading-[1.6] text-fg">
              Portable binary built natively on Linux. Nothing to install and
              no Python needed, since everything is bundled in.
            </p>
            <a
              href={LINUX_ASSET}
              className="mt-7 bg-accent px-6 py-3.5 text-center font-semibold text-base transition-colors hover:bg-accent-bright"
            >
              Download
            </a>
          </div>
        </div>

        <div className="mt-8 flex flex-col gap-2 text-[0.86rem] leading-[1.6] text-fg-dim">
          <p>
            Windows will show a SmartScreen prompt the first time, because the
            build isn&rsquo;t code-signed yet, so choose{' '}
            <span className="text-fg">More info → Run anyway</span>.
          </p>
          <p>
            Prefer to run from source, or on another platform? See the{' '}
            <a href="/ALAS-site/docs/installation/" className="prose-link">
              installation guide
            </a>
            .
          </p>
        </div>
      </div>
    </section>
  )
}
