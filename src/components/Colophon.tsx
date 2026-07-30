export default function Colophon() {
  const year = new Date().getFullYear()

  return (
    <footer>
      <div className="mx-auto max-w-[68rem] px-6 py-12">
        <div className="flex flex-col gap-6 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-center gap-2.5">
            <img src="/brand/icon.png" alt="" className="h-6 w-6" />
            <span className="font-serif text-[1rem] font-semibold text-fg-strong">
              ALAS
            </span>
          </div>

          <nav className="flex flex-wrap gap-x-7 gap-y-2 text-[0.86rem] text-fg-dim">
            <a href="/docs/" className="transition-colors hover:text-fg-strong">
              Documentation
            </a>
            <a
              href="https://github.com/MarcosQuirogaR/ALAS/releases"
              className="transition-colors hover:text-fg-strong"
            >
              Releases
            </a>
            <a
              href="/docs/troubleshooting/"
              className="transition-colors hover:text-fg-strong"
            >
              Support
            </a>
            <a href="/acknowledgements/" className="transition-colors hover:text-fg-strong">
              Acknowledgements
            </a>
          </nav>
        </div>

        <div className="mt-9 flex flex-col gap-3 border-t border-rule pt-6 text-[0.78rem] text-fg-dim sm:flex-row sm:items-center sm:justify-between">
          <p>© {year} ALAS. AGPL-3.0-or-later.</p>
          <p className="max-w-lg sm:text-right">
            Built on AeroSandbox and SUAVE (LGPL-2.1). Nastran and Patran
            integrations are optional and require your own licence.
          </p>
        </div>
      </div>
    </footer>
  )
}
