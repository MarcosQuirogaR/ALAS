// Prints the release tags whose archives the site hosts under /downloads:
// the newest stable release with a publication date, plus every published
// pre-release newer than it. Reads site/src/lib/releases.ts (entries are
// listed newest first). Used by .github/workflows/site-deploy.yml.
import { readFileSync } from 'node:fs'

const raw = readFileSync(new URL('../src/lib/releases.ts', import.meta.url), 'utf8')
const src = raw.split('\r\n').join('\n')
const body = src.slice(src.indexOf('export const RELEASES'))
const entries = body.split(/\n {2}\{\n {4}tag: /).slice(1)
const out = []
for (const e of entries) {
  const tag = /^'([^']+)'/.exec(e)?.[1]
  const published = /\n {4}published: (null|'[^']+')/.exec(e)?.[1]
  if (!tag || !published || published === 'null') continue
  out.push(tag)
  if (!/\n {4}prerelease: true/.test(e)) break
}
console.log(out.join('\n'))
