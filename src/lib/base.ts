/**
 * Resolves an application path or anchor against import.meta.env.BASE_URL.
 * Supports subpath deployment such as /ALAS-site/ on GitHub Pages.
 */
export function withBase(path = ''): string {
  const rawBase = import.meta.env.BASE_URL || '/'
  const base = rawBase.endsWith('/') ? rawBase : `${rawBase}/`

  if (!path) return base
  if (path.startsWith('#')) return `${base}${path}`

  const cleanPath = path.startsWith('/') ? path.slice(1) : path
  return `${base}${cleanPath}`
}
