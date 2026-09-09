import { useState } from 'react'
import { withBase } from '../lib/base'

export default function Masthead() {
  const [isOpen, setIsOpen] = useState(false)

  const links = [
    { href: withBase('#overview'), label: 'Overview' },
    { href: withBase('docs/'), label: 'Documentation' },
    { href: withBase('#download'), label: 'Downloads' },
    { href: withBase('#releases'), label: 'Release notes' },
  ]

  return (
    <header className="sticky top-0 z-50 border-b border-rule bg-base/90 backdrop-blur-md">
      <div className="mx-auto flex max-w-[68rem] items-center justify-between gap-6 px-6 py-3.5">
        <a href={withBase('')} className="flex items-center gap-2.5">
          <img
            src={withBase('brand/wordmark.png')}
            alt="ALAS"
            className="h-7 w-auto object-contain"
          />
        </a>

        {/* Desktop navigation */}
        <nav className="hidden items-center gap-7 text-[0.86rem] text-fg-dim md:flex">
          {links.map((l) => (
            <a key={l.href} href={l.href} className="transition-colors hover:text-fg-strong">
              {l.label}
            </a>
          ))}
        </nav>

        <div className="flex items-center gap-3">
          <a
            href={withBase('#download')}
            className="hidden sm:inline-block bg-accent px-4 py-2 text-[0.82rem] font-semibold text-base transition-colors hover:bg-accent-bright"
          >
            Downloads
          </a>

          {/* Mobile menu toggle */}
          <button
            type="button"
            onClick={() => setIsOpen(!isOpen)}
            aria-expanded={isOpen}
            aria-label="Toggle navigation"
            className="flex h-9 w-9 items-center justify-center border border-rule text-fg-dim transition-colors hover:border-rule-strong hover:text-fg-strong md:hidden"
          >
            {isOpen ? (
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="square" strokeWidth="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            ) : (
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="square" strokeWidth="2" d="M4 6h16M4 12h16M4 18h16" />
              </svg>
            )}
          </button>
        </div>
      </div>

      {/* Mobile dropdown panel */}
      {isOpen && (
        <div className="border-t border-rule bg-base/98 px-6 py-4 md:hidden">
          <nav className="flex flex-col gap-3 text-[0.92rem]">
            {links.map((l) => (
              <a
                key={l.href}
                href={l.href}
                onClick={() => setIsOpen(false)}
                className="py-1 text-fg-dim transition-colors hover:text-fg-strong"
              >
                {l.label}
              </a>
            ))}
            <a
              href={withBase('#download')}
              onClick={() => setIsOpen(false)}
              className="mt-2 inline-block bg-accent px-4 py-2.5 text-center text-[0.86rem] font-semibold text-base transition-colors hover:bg-accent-bright"
            >
              Downloads
            </a>
          </nav>
        </div>
      )}
    </header>
  )
}
