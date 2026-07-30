import { useReleases } from '../lib/useRelease'

const WINDOWS_ASSET =
  'https://github.com/MarcosQuirogaR/ALAS/releases/latest/download/ALAS-windows.exe'

export default function Hero() {
  const releases = useReleases(1)
  const latest = releases?.[0]

  return (
    <section id="top" className="relative isolate overflow-hidden border-b border-rule">
      {/* Render by Antón Ochoa Castro and Aarón Pérez Pardiñas. See
          Acknowledgements. Positioned so the aircraft sits in the right-hand
          third, which the veil deliberately leaves clear. */}
      <img
        src="/brand/hero.jpg"
        alt=""
        aria-hidden="true"
        className="absolute inset-0 -z-20 h-full w-full object-cover object-[72%_42%]"
      />
      <div aria-hidden="true" className="hero-veil absolute inset-0 -z-10" />
      <div aria-hidden="true" className="hero-fade absolute inset-0 -z-10" />

      <div className="mx-auto max-w-[68rem] px-6 pb-20 pt-24 sm:pb-28 sm:pt-32 lg:pb-36 lg:pt-40">
        <p className="section-mark">Aircraft preliminary design</p>

        <h1 className="mt-6 max-w-[22ch] font-serif text-[2.6rem] font-semibold leading-[1.08] tracking-[-0.02em] text-fg-strong sm:text-[3.4rem]">
          Size an airframe.
          <br />
          Then find out if it flies.
        </h1>

        <p className="mt-7 max-w-[46ch] text-[1.08rem] leading-[1.6] text-fg">
          The desktop application that sizes an aircraft against the mission
          you design.
        </p>

        <div className="mt-10 flex flex-col gap-4 sm:flex-row sm:items-center">
          <a
            href={WINDOWS_ASSET}
            className="inline-flex items-center justify-center gap-3 bg-accent px-7 py-4 font-semibold text-base transition-colors hover:bg-accent-bright"
          >
            Download for Windows
            {latest && (
              <span className="font-mono text-[0.72rem] font-normal opacity-80">
                {latest.tag}
                {latest.windowsAssetMB ? ` · ${latest.windowsAssetMB} MB` : ''}
              </span>
            )}
          </a>

          <a
            href="/docs/"
            className="inline-flex items-center justify-center border border-rule-strong px-7 py-4 font-semibold text-fg-strong transition-colors hover:border-accent hover:text-accent-bright"
          >
            Read the documentation
          </a>
        </div>

        <p className="mt-6 font-mono text-[0.72rem] uppercase tracking-[0.13em] text-fg-dim">
          Free · Open source · Runs offline · Windows &amp; Linux
        </p>
      </div>
    </section>
  )
}
