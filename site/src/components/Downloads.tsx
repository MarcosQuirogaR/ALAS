import { withBase } from '../lib/base'
import {
  CURRENT,
  formatMB,
  PRERELEASES,
  RELEASES_URL,
  releaseUrl,
  type PlatformAsset,
} from '../lib/releases'

type Platform = {
  name: string
  requirement: string
  asset: PlatformAsset | undefined
}

export default function Downloads() {
  const assets = CURRENT.assets

  const platforms: Platform[] = [
    {
      name: 'Windows',
      requirement: 'Windows 10 or later, x86-64. Portable zip.',
      asset: assets?.windows,
    },
    {
      name: 'Linux',
      requirement: 'glibc 2.35 or newer, x86-64. Portable tar.gz.',
      asset: assets?.linux,
    },
  ]

  return (
    <section id="download" className="border-b border-rule bg-raised/40">
      <div className="mx-auto max-w-[68rem] px-6 py-16 sm:py-20">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between sm:gap-10">
          <h2 className="text-[1.65rem] font-bold leading-[1.25] tracking-[-0.015em] text-fg-strong">
            ALAS {CURRENT.tag}
          </h2>
          <p className="text-[0.96rem] text-fg-dim sm:text-right">
            Portable desktop app. External solvers are not included.
          </p>
        </div>

        <div className="mt-10 grid gap-5 sm:grid-cols-2">
          {platforms.map((p) => (
            <div key={p.name} className="flex min-w-0 flex-col border border-rule bg-raised p-7">
              <div className="flex items-baseline justify-between gap-4">
                <h3 className="text-[1.15rem] font-bold text-fg-strong">{p.name}</h3>
                <span className="font-mono text-[0.8rem] text-fg-dim">
                  {CURRENT.tag}
                  {p.asset ? `, ${formatMB(p.asset.bytes)}` : ''}
                </span>
              </div>
              <p className="mt-3 text-[0.9rem] text-fg-dim">{p.requirement}</p>

              <a
                href={p.asset ? withBase(`downloads/${p.asset.file}`) : releaseUrl(CURRENT.tag)}
                download={p.asset ? true : undefined}
                className="mt-7 bg-accent px-6 py-4 text-center text-[1rem] font-semibold text-base transition-colors hover:bg-accent-bright"
              >
                Download for {p.name}
              </a>
            </div>
          ))}
        </div>

        {PRERELEASES.map((pre) => (
          <div key={pre.tag} className="mt-10 border border-rule p-7">
            <h3 className="text-[1.05rem] font-bold text-fg-strong">
              {pre.tag} <span className="ml-2 font-mono text-[0.75rem] text-accent">PRE-RELEASE</span>
            </h3>
            <p className="mt-2 max-w-[62ch] text-[0.9rem] leading-[1.55] text-fg-dim">
              A beta with newer features (add-ons, plot data export, My aircraft, native Browse
              dialogs) and physics fixes. Numbers change against {CURRENT.tag}. Use the stable release
              above unless you want to test the beta.
            </p>
            <p className="mt-4 flex flex-wrap gap-x-6 gap-y-2 text-[0.88rem]">
              {pre.assets && (
                <>
                  <a href={withBase(`downloads/${pre.assets.windows.file}`)} download className="prose-link">
                    Windows ({formatMB(pre.assets.windows.bytes)}) →
                  </a>
                  <a href={withBase(`downloads/${pre.assets.linux.file}`)} download className="prose-link">
                    Linux ({formatMB(pre.assets.linux.bytes)}) →
                  </a>
                </>
              )}
              <a href={releaseUrl(pre.tag)} className="prose-link">
                Release notes and known limitations →
              </a>
            </p>
          </div>
        ))}

        <p className="mt-6 flex flex-wrap gap-x-6 gap-y-2 text-[0.84rem] text-fg-dim">
          <a href={RELEASES_URL} className="prose-link">
            Checksums and all releases on GitHub →
          </a>
          <a href={withBase('docs/installation/')} className="prose-link">
            Installation guide →
          </a>
        </p>
      </div>
    </section>
  )
}
