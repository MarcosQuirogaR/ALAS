/**
 * Curated release data for the landing page. Everything the Downloads and
 * Release notes sections show comes from this module; no network request is
 * needed to render them.
 *
 * To publish a new release:
 *   1. Set `published` (ISO date, YYYY-MM-DD) on its entry. An entry whose
 *      `published` is null is NOT rendered anywhere. v1.3.2 is in that state:
 *      the development branch is already bumped to it, but it has no GitHub
 *      release yet.
 *   2. Fill in `assets` (size in bytes and the SHA-256 hex of each archive).
 *   3. Upload the archives to /downloads/ with site/scripts/fetch-downloads.ps1.
 * The current version is the newest entry with a non-null `published`.
 */

export const REPO = 'MarcosQuirogaR/ALAS'
export const REPO_URL = `https://github.com/${REPO}`
export const RELEASES_URL = `${REPO_URL}/releases`

export type PlatformAsset = {
  /** File name under /downloads/. */
  file: string
  /** Size in bytes, as listed on the GitHub release. */
  bytes: number
  /** SHA-256 of the archive, lowercase hex. */
  sha256: string
}

export type ReleaseEntry = {
  tag: string
  /** ISO date (YYYY-MM-DD), or null while unreleased (entry is hidden). */
  published: string | null
  headline: string
  details: string[]
  /** Present only for releases whose archives are hosted on this site. */
  assets?: { windows: PlatformAsset; linux: PlatformAsset }
}

/** Newest first. */
export const RELEASES: ReleaseEntry[] = [
  {
    tag: 'v1.3.2',
    // Not published yet: leave null until the GitHub release exists, then set
    // the date and add `assets`. Until then nothing below is shown.
    published: null,
    headline: 'Measured nose geometry, B747-400 upper deck',
    details: [
      'B747-400 upper-deck hump; model OEW is +13.8 % over published.',
      'Measured nose geometry on seven presets, plus the DC-10 nose.',
      'Longer noses shorten the generic cabin and move the CG aft.',
    ],
  },
  {
    tag: 'v1.3.1',
    published: '2026-10-06',
    headline: 'Four new presets, parametric nose',
    details: [
      'E195-E2, C919, B747-400 and A400M join the registry (12 presets).',
      'OEW vs published: E195-E2 within 0.3 %, C919 -3.1 %, B747-400 +9.3 %.',
      'Opt-in parametric nose through six fields.',
      'CG findings reported for B747-400 and A400M.',
      'Tests no longer fail on CI hosts without navigation data.',
    ],
    assets: {
      windows: {
        file: 'alas-v1.3.1-windows-x86_64.zip',
        bytes: 61_837_163,
        sha256: 'd73bc8bc4645abb814a7ee928d559a93ca875c697fa5677a94d227d7a69cbd8b',
      },
      linux: {
        file: 'alas-v1.3.1-linux-x86_64.tar.gz',
        bytes: 60_502_104,
        sha256: '7713fc50873c79edd27c68c3077f6ff3737f2a43d618ae758a497277187f7991',
      },
    },
  },
  {
    tag: 'v1.3.0',
    published: '2026-10-05',
    headline: 'Fast optimizer, hard MTOW, load and trim sheet',
    details: [
      'Screening and refinement in about two minutes.',
      'Hard MTOW: takeoff fuel capped at tank capacity.',
      'Load and trim CG sheet in the results.',
      'Clean-sheet design from a mission brief alone.',
      'Rotation forward-CG limit; N-engine turboprop propulsion.',
      'Wing-box beam check against MSC Nastran (user-supplied).',
    ],
  },
  {
    tag: 'v1.2.0',
    published: '2026-09-24',
    headline: 'Native Rust application, L-SHADE, Sandbox Mode',
    details: [
      'L-SHADE epsilon-constrained optimizer, deterministic per seed.',
      'Sandbox Mode: 3D editing with quick and full analyses.',
      'CFD meshing pipeline (OpenFOAM, Gmsh).',
      'Wing-to-fuselage layout checks; Korn drag-divergence Mach.',
      'Windows and Linux packages built from the tagged commit.',
      'Fixes to fuel loading, solver timeouts and exports.',
    ],
  },
  {
    tag: 'v1.1.0',
    published: '2026-09-14',
    headline: 'Windows and Linux packages with checksums',
    details: [
      'Windows zip and Linux tar.gz, each with a SHA-256 file.',
      'Navigation data downloaded on request, not bundled.',
      'MSES is user-supplied; binaries not distributed.',
    ],
  },
  {
    tag: 'v1.0.0',
    published: '2026-07-29',
    headline: 'First public release as ALAS',
    details: [
      'Renamed from AeroForge to ALAS (Aircraft Layout, Analysis and Sizing).',
      'AGPL-3.0-or-later, with third-party notices.',
      'MSES binaries and bundled navigation data removed.',
      'In-app optional asset download, licence shown first.',
    ],
  },
]

/** Releases with a publication date, newest first. */
export const PUBLISHED: ReleaseEntry[] = RELEASES.filter((r) => r.published !== null)

/** Newest published release; drives version labels and download buttons. */
export const CURRENT: ReleaseEntry = PUBLISHED[0]

export function releaseUrl(tag: string): string {
  return `${RELEASES_URL}/tag/${tag}`
}

export function formatDate(iso: string | null): string {
  if (!iso) return ''
  const d = new Date(`${iso}T00:00:00Z`)
  if (Number.isNaN(d.getTime())) return ''
  return d.toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  })
}

export function formatMB(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`
}
