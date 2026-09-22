# MSES Orr–Sommerfeld transition map

`osmapDP.dat` is the unmodified double-precision Orr–Sommerfeld lookup
database distributed in Mark Drela's official XFOIL 6.99 source archive.  ALAS
uses it as a data resource for an installed MSES executable when the MSES case
requests natural (free) boundary-layer transition.

## Provenance

- Upstream page: <https://web.mit.edu/drela/Public/web/xfoil/>
- Exact archive: <https://web.mit.edu/drela/Public/web/xfoil/xfoil6.99.tgz>
- Archive SHA-256:
  `5C0250643F52CE0E75D7338AE2504CE7907F2D49A30F921826717B8AC12EBE40`
- Extracted path in that archive: `Xfoil/orrs/osmapDP.dat`
- Map size: `1,576,588` bytes
- Map SHA-256:
  `2F6B3C63461D71DA9B6CB9CA1340D77CFF0CFBE767679D15B5B8556B45D948C4`
- Acquisition date: 2026-09-20

The archive is retained beside the map so the exact upstream source and
generation inputs remain reviewable.  The archive's Orr–Sommerfeld README
documents generation with the `osgen` program and the double-precision build
flags in `Makefile_DP`; no map bytes were converted or regenerated for ALAS.
The upstream archive includes the raw `osm.*`/`osm_ns.*` slices, the
`osmaps_ns.lst` input list, and the `osgen` source needed to reproduce the
database.  Debian's historical XFOIL packaging discussion records that its
maintainer omitted the generated map after a local regeneration produced many
`NaN` values; that is a build-quality caveat, not evidence that this official
archive's map is absent or that its licence is proprietary.  The exact official
binary map retained here is the one validated against ALAS's installed MSES.

## License and boundary

The official XFOIL page releases XFOIL under the GNU General Public License;
the XFOIL source headers specify GPL version 2 or any later version.  The
corresponding license text is retained in `COPYING-XFOIL.txt`.  This directory
contains the map as an unmodified XFOIL distribution asset and retains the
complete upstream archive for source and provenance access.

This map is separate data consumed by an external process.  It does not grant
permission to redistribute the MSES executables.  MSES remains a separately
licensed, user-supplied installation under the MIT Technology Licensing Office
terms.  The ALAS release package must preserve this notice and continue to
record MSES as user-supplied.

## Compatibility evidence

The staged map has the installed MSES 3.12c-compatible double-precision
unformatted record layout: the first table record is 224 bytes, with dimensions
`NR=28`, `NW=41`, and `NH=18`.  The installed `mses.exe` loaded this exact file
and printed `Converged on tolerance` in a free-transition case.  A single-
precision `osmap.dat` from the same XFOIL archive produced an end-of-record
runtime error, so the similarly named single-precision file must not be used.
