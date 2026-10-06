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

**GitHub Pages** (`marcosquirogar.github.io/ALAS`) is only a redirect:
`.github/workflows/site-pages.yml` publishes `.github/pages-redirect/index.html`
(also as `404.html`), which forwards every path to the same path on
`alas.uvigo.es`. It does not build or host the site.

Earlier revisions of this file claimed that pushing to `main` published the
live site. That was never true of `alas.uvigo.es`.

## Releases and downloads

Release data (dates, headlines, details, archive sizes and SHA-256 values) is
curated in `src/lib/releases.ts`; the Downloads and Release notes sections
render from it and need no network access. A release whose `published` is
`null` is not rendered.

To publish a release, set its `published` date and `assets` in that file,
build, then place the archives next to the site:

```
npm run build
pwsh scripts/fetch-downloads.ps1 -OutDir dist/downloads
```

`fetch-downloads.ps1` needs the GitHub CLI, downloads the current release
archives and their `.sha256` files with `gh`, and verifies each checksum.
The archives are served at `/downloads/` and mirrored on
[GitHub Releases](https://github.com/MarcosQuirogaR/ALAS/releases).

## Validation section

`public/validation/manifest.json` (a bare array of figures or
`{figures, summary}`) drives the Validation section, which is hidden when
the manifest is missing.
