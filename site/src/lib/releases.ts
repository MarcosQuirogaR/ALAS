/**
 * Curated release data for the landing page. Everything the Downloads and
 * Release notes sections show comes from this module; no network request is
 * needed to render them.
 *
 * To publish a new release:
 *   1. Set `published` (ISO date, YYYY-MM-DD) on its entry. An entry whose
 *      `published` is null is NOT rendered anywhere. Mark pre-releases with
 *      `prerelease: true`: they are offered next to, never instead of, the
 *      newest stable release.
 *   2. Fill in `assets` (size in bytes and the SHA-256 hex of each archive).
 *   3. Upload the archives to /downloads/ with site/scripts/fetch-downloads.ps1.
 * The current version is the newest stable entry with a non-null `published`;
 * a newer published pre-release is shown as BETA. The deploy workflow fetches
 * the archives of both (site/scripts/release-tags.mjs).
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
  /** True for a beta or other pre-release (never the "current" release). */
  prerelease?: boolean
  headline: string
  details: string[]
  /** Present only for releases whose archives are hosted on this site. */
  assets?: { windows: PlatformAsset; linux: PlatformAsset }
}

/** Newest first. */
export const RELEASES: ReleaseEntry[] = [
  {
    tag: 'v1.4.1-beta.1',
    published: '2026-10-19',
    prerelease: true,
    headline: 'Beta: add-ons, plot data export, My aircraft, native Browse dialogs',
    details: [
      'Native Browse dialogs for every file and folder field; per-user data folder.',
      'My aircraft: save, reload, import and export your own presets. Search boxes in long lists.',
      'Add-ons: sandboxed Rhai scripts and external process or Python plug-ins, disabled until enabled and recorded in the run provenance.',
      'Plot data export: the numbers behind every plot as CSV and JSON (GUI and --plot-data).',
      'Airport import fills the custom-airport form and merges into your registry.',
      'Physics fixes: CS-25 take-off and landing distance models, Jenkinson thrust charts, wing mass from gust, torque and fatigue cases, certified seat counts, LD3 hold positions.',
      'Real-aircraft comparison: 172 scored cells within 10 % (158 before), 16 outside; limitations listed in the release notes and on the validation page.',
      'Fixed-wing UAV page enabled as preliminary (not validated).',
    ],
    assets: {
      windows: {
        file: 'alas-v1.4.1-beta.1-windows-x86_64.zip',
        bytes: 47_699_457,
        sha256: '5f44c5428ea17bac25497565d7b64567a070a1773547cfbaf1383a21aeb4b698',
      },
      linux: {
        file: 'alas-v1.4.1-beta.1-linux-x86_64.tar.gz',
        bytes: 46_515_916,
        sha256: 'cf9ab69b3009f3ebff9a5f6aae8c33147252029aa09997282931d344b4baed9c',
      },
    },
  },
  {
    tag: 'v1.4.0',
    published: '2026-10-19',
    headline: 'Consistency and validation release',
    details: [
      'Geometry, drag, thrust, mass and payload bookkeeping corrected across the twelve presets.',
      'Jenkinson thrust charts for take-off, climb and cruise with uncertainty bands and sourced cruise anchors.',
      'Wing structure sized from gust, torque and fatigue cases, with Torenbeek non-optimum mass.',
      'CS-25.113 take-off distance with CLmax from the high-lift system; CS-25.121(d) approach climb.',
      'Seat-packer fix; A400M freight hold of nine 463L pallets.',
      'Fixed-wing UAV page enabled as preliminary (not validated).',
      'Native matrix run in about 41 minutes instead of 87. Designs are not comparable with v1.3.2.',
    ],
    assets: {
      windows: {
        file: 'alas-v1.4.0-windows-x86_64.zip',
        bytes: 46_605_732,
        sha256: 'b0394313a7301926320f62d78081a7f0e3f6da3266268f317abc17230607fea5',
      },
      linux: {
        file: 'alas-v1.4.0-linux-x86_64.tar.gz',
        bytes: 45_414_775,
        sha256: '34978f0cb7f500fa4220cd8490edba4ce282f1b99dd7c5ecd1d8c416d5e552c1',
      },
    },
  },
  {
    tag: 'v1.3.2',
    published: '2026-10-07',
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

/** Newest published stable release; drives version labels and download buttons. */
export const CURRENT: ReleaseEntry = PUBLISHED.find((r) => !r.prerelease) ?? PUBLISHED[0]

/** Published pre-releases newer than the current stable release, newest first. */
export const PRERELEASES: ReleaseEntry[] = PUBLISHED.filter(
  (r) => r.prerelease && PUBLISHED.indexOf(r) < PUBLISHED.indexOf(CURRENT),
)

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
