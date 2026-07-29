/* Absolute so the header behaves identically on the landing page and on
   any standalone page that reuses it. */
const BASE = '/ALAS-site/'

const LINKS = [
  { href: `${BASE}#overview`, label: 'Overview' },
  { href: `${BASE}docs/`, label: 'Documentation' },
  { href: `${BASE}#download`, label: 'Downloads' },
  { href: `${BASE}#releases`, label: 'Release notes' },
]

export default function Masthead() {
  return (
    <header className="sticky top-0 z-50 border-b border-rule bg-base/90 backdrop-blur-md">
      <div className="mx-auto flex max-w-[68rem] items-center justify-between gap-6 px-6 py-3.5">
        <a href={BASE} className="flex items-center gap-2.5">
          <img src="/ALAS-site/brand/icon.png" alt="" className="h-7 w-7" />
          <span className="font-serif text-[1.06rem] font-semibold tracking-[-0.01em] text-fg-strong">
            ALAS
          </span>
        </a>

        <nav className="hidden items-center gap-7 text-[0.86rem] text-fg-dim md:flex">
          {LINKS.map((l) => (
            <a key={l.href} href={l.href} className="transition-colors hover:text-fg-strong">
              {l.label}
            </a>
          ))}
        </nav>

        <a
          href={`${BASE}#download`}
          className="bg-accent px-4 py-2 text-[0.82rem] font-semibold text-base transition-colors hover:bg-accent-bright"
        >
          Download
        </a>
      </div>
    </header>
  )
}
