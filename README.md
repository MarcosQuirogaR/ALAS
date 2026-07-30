# ALAS: site

Public web presence for [ALAS](https://alas.uvigo.es/),
an aircraft preliminary-design pipeline. This repo holds the public
website and documentation; the application source and release binaries live
in the separate [MarcosQuirogaR/ALAS](https://github.com/MarcosQuirogaR/ALAS)
repository.

Two things live here:

- **Landing page** (repo root): Vite + React + TypeScript + Tailwind CSS v4
  + Framer Motion, served at the site root.
- **Documentation** (`docs-site/`): MkDocs + Material, a 25-chapter guide
  built around the AVE reference case, served at
  [`/docs/`](https://alas.uvigo.es/docs/). Every
  figure in it comes from a real ALAS run.

## Develop

```bash
# Landing page
npm install
npm run dev

# Docs (needs Python 3.10+)
pip install -r docs-site/requirements.txt
mkdocs serve -f docs-site/mkdocs.yml
```

## Deploy

Pushing to `main` builds both (Vite → `dist/`, MkDocs → `dist/docs/`) and
publishes the combined output to GitHub Pages via
`.github/workflows/deploy.yml`.

## Releases

Downloadable installers are attached to
[GitHub Releases](https://github.com/MarcosQuirogaR/ALAS/releases) on the
main ALAS repo, built and published by that repo's own release workflow.
The Download section on this site links to the `latest` release assets
there by a fixed filename (e.g. `ALAS-windows.exe`).
