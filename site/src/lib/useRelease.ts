import { useEffect, useState } from 'react'

export type Release = {
  tag: string
  name: string
  published: string
  body: string
  htmlUrl: string
  prerelease: boolean
  windowsAssetMB: number | null
  windowsAssetUrl: string | null
  linuxAssetMB: number | null
  linuxAssetUrl: string | null
}

const REPO = 'MarcosQuirogaR/ALAS'

function parse(raw: Record<string, unknown>): Release {
  const assets =
    (raw.assets as { name: string; size: number; browser_download_url?: string }[] | undefined) ?? []
  const win = assets.find((a) => {
    const name = a.name.toLowerCase()
    return (
      name.includes('windows') &&
      !name.endsWith('.sha256') &&
      (name.endsWith('.zip') || name.endsWith('.exe'))
    )
  })
  const linux = assets.find((a) => {
    const name = a.name.toLowerCase()
    return (
      name.includes('linux') &&
      !name.endsWith('.sha256') &&
      !name.endsWith('.txt') &&
      (name.endsWith('.tar.gz') ||
        name.endsWith('.tar.xz') ||
        name.endsWith('.zip') ||
        !name.includes('.'))
    )
  })
  return {
    tag: String(raw.tag_name ?? ''),
    name: String(raw.name || raw.tag_name || ''),
    published: String(raw.published_at ?? ''),
    body: String(raw.body ?? ''),
    htmlUrl: String(raw.html_url || `https://github.com/${REPO}/releases`),
    prerelease: Boolean(raw.prerelease),
    windowsAssetMB: win ? Math.round(win.size / 1_000_000) : null,
    windowsAssetUrl: win?.browser_download_url ?? null,
    linuxAssetMB: linux ? Math.round(linux.size / 1_000_000) : null,
    linuxAssetUrl: linux?.browser_download_url ?? null,
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
