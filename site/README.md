# ALAS: site

Public web presence for [ALAS](https://alas.uvigo.es/),
an aircraft preliminary-design pipeline. This repo holds the public
website and documentation; the application source and release binaries live
in the separate [MarcosQuirogaR/ALAS](https://github.com/MarcosQuirogaR/ALAS)
repository.

Two things live here:

- **Landing page** (repo root): Vite + React + TypeScript + Tailwind CSS v4,
  served at the site root.
- **Documentation** (`docs-site/`): MkDocs + Material, a 26-chapter guide
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

There are two destinations, and they are not the same thing.

**The live site, `alas.uvigo.es`,** is an Apache host at the university. It is
not served by GitHub Pages and no workflow in this repository updates it.
Publishing there is a separate upload of the built `dist/` tree over SFTP, and
it has to be done deliberately.

**The Pages mirror** is built and published by
`.github/workflows/site-pages.yml` at the repository root whenever `site/`
changes on `main`. It has no custom domain attached.

Earlier revisions of this file claimed that pushing to `main` published the
live site. That was never true of `alas.uvigo.es`.

## Releases

Downloadable Windows and Linux packages for v1.1.0 are published on
[GitHub Releases](https://github.com/MarcosQuirogaR/ALAS/releases). The Download
section on this site links directly to each tagged asset and its checksums.
