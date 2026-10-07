/**
 * Resolves an application path or anchor against import.meta.env.BASE_URL.
 * Supports both the live domain root and an optional subpath deployment.
 */
export function withBase(path = ''): string {
  const rawBase = import.meta.env.BASE_URL || '/'
  let base = rawBase.endsWith('/') ? rawBase : `${rawBase}/`
  // A relative build (VITE_BASE=./) serves pages from different depths; a page
  // below the root names its way back with <meta name="site-root" content="../">.
  if (base === './' && typeof document !== 'undefined') {
    const root = document.querySelector('meta[name="site-root"]')?.getAttribute('content')
    if (root) base = root.endsWith('/') ? root : `${root}/`
  }

  if (!path) return base
  if (path.startsWith('#')) return `${base}${path}`

  const cleanPath = path.startsWith('/') ? path.slice(1) : path
  return `${base}${cleanPath}`
}
