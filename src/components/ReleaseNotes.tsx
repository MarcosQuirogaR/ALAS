import { formatDate, useReleases } from '../lib/useRelease'

const RELEASES_URL = 'https://github.com/MarcosQuirogaR/ALAS/releases'

/** Release bodies are Markdown. Rather than pull in a renderer (and an HTML
 *  injection surface) for a few bullet points, take the first handful of
 *  lines, strip the markup a changelog actually uses, and render as plain
 *  text: list markers, **bold**, `code` spans, and [links](url) (kept as
 *  their visible text -- a plain-text summary can't offer them as links
 *  anyway). */
function summarise(body: string, maxLines = 4): string[] {
  return body
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.length > 0 && !l.startsWith('#'))
    .slice(0, maxLines)
    .map((l) =>
      l
        .replace(/^[-*•]\s*/, '')
        .replace(/\*\*(.+?)\*\*/g, '$1')
        .replace(/`([^`]+)`/g, '$1')
        .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    )
}

export default function ReleaseNotes() {
  const releases = useReleases(3)

  return (
    <section id="releases" className="border-b border-rule bg-raised/40">
      <div className="mx-auto max-w-[68rem] px-6 py-20">
        <p className="section-mark">Release notes</p>

        <div className="mt-6 flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
          <h2 className="font-serif text-[1.85rem] font-semibold leading-[1.2] tracking-[-0.015em] text-fg-strong">
            What&rsquo;s changed
          </h2>
          <a href={RELEASES_URL} className="prose-link text-[0.92rem]">
            All releases on GitHub →
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
              const lines = summarise(r.body)
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
