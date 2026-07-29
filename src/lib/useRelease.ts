import { useEffect, useState } from 'react'

export type Release = {
  tag: string
  name: string
  published: string
  body: string
  windowsAssetMB: number | null
  linuxAssetMB: number | null
}

const REPO = 'MarcosQuirogaR/ALAS'

function parse(raw: Record<string, unknown>): Release {
  const assets = (raw.assets as { name: string; size: number }[] | undefined) ?? []
  const win = assets.find((a) => a.name === 'ALAS-windows.exe')
  const linux = assets.find((a) => a.name === 'ALAS-linux')
  return {
    tag: String(raw.tag_name ?? ''),
    name: String(raw.name || raw.tag_name || ''),
    published: String(raw.published_at ?? ''),
    body: String(raw.body ?? ''),
    windowsAssetMB: win ? Math.round(win.size / 1_000_000) : null,
    linuxAssetMB: linux ? Math.round(linux.size / 1_000_000) : null,
  }
}

/**
 * Releases straight from the GitHub API, newest first. Returns null while
 * loading and on any failure (rate limit, offline) so callers can fall back
 * to static copy rather than showing a broken panel.
 */
export function useReleases(count = 3): Release[] | null {
  const [releases, setReleases] = useState<Release[] | null>(null)

  useEffect(() => {
    let cancelled = false
    fetch(`https://api.github.com/repos/${REPO}/releases?per_page=${count}`)
      .then((r) => (r.ok ? r.json() : null))
      .then((data) => {
        if (cancelled || !Array.isArray(data) || data.length === 0) return
        setReleases(data.map(parse))
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [count])

  return releases
}

export function formatDate(iso: string): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  return d.toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric' })
}
