# Third-party notices

This file lists every third-party component whose code, data or interface this
program incorporates. It covers three distinct relationships, which have
different licensing consequences and are kept separate below:

- **Translated code**: a derivative work. The upstream licence applies to the
  translation, and every affected module carries a provenance header naming its
  origin, upstream licence and source revision.
- **Bundled data**: redistributed unmodified, or repacked without changing the
  values.
- **Invoked executables**: separate programs the user supplies. Not
  distributed here, not covered by this program's licence.

Cargo dependencies are not listed individually. `cargo tree` enumerates them and
`cargo deny` checks their declared licences against `deny.toml`. The dependency
policy admits AGPL-3.0-or-later, Apache-2.0, Apache-2.0 WITH LLVM-exception,
MIT, BSD-2-Clause, BSD-3-Clause, BSL-1.0, ISC, Zlib, MPL-2.0, Unicode-3.0,
OFL-1.1 and LicenseRef-UFL-1.0; this list is a dependency-policy allowlist,
not a claim that every embedded asset or external executable has one of these
licences.

---

## Translated code

### SUAVE 2.5.2: LGPL-2.1

Stanford University Aerospace Design Lab and contributors.
<https://github.com/suavecode/SUAVE>

The mission-analysis stack is translated from SUAVE: the vehicle and turbofan
network model, the `Weights_Transport` correlations, `Fidelity_Zero`
aerodynamics and stability, the VORLAX-derived vortex-lattice method, and the
Chebyshev pseudospectral mission segment solver.

LGPL-2.1 section 3 permits a recipient to treat a copy under the ordinary GPL
version 2 or any later version. This program takes that route, which makes the
translated modules compatible with AGPL-3.0-or-later.

Affected crates: `alas-mission`, `alas-aero` (VORLAX, drag build-up, lift
surrogate), `alas-mass` (`Weights_Transport`), `alas-stab` (`Fidelity_Zero`),
`alas-prop` (turbofan network and sizing), `alas-atmo` (US Standard 1976),
`alas-math` (Chebyshev pseudospectral operator).

### AeroSandbox 4.2.8: MIT

Peter Sharpe and contributors. <https://github.com/peterdsharpe/AeroSandbox>

The parametric geometry model (`Airplane`, `Wing`, `Fuselage`, `Airfoil` and
their query methods), the vortex-lattice method, the ISA atmosphere, the
Torenbeek weight correlations and the flight-dynamics mode approximations are
translated from AeroSandbox.

Affected crates: `alas-geom`, `alas-aero`, `alas-atmo`, `alas-mass`, `alas-stab`.

### NeuralFoil: MIT

Peter Sharpe and contributors. <https://github.com/peterdsharpe/NeuralFoil>

The airfoil-polar surrogate: network evaluation, the Kulfan/CST coordinate fit
that produces its inputs, and the output unpacking. The trained weights are
bundled as data; see below.

Affected crate: `alas-aero`.

### MINPACK `hybrd`: public domain

Argonne National Laboratory (Burton S. Garbow, Kenneth E. Hillstrom,
Jorge J. Moré, 1980). Distributed without restriction.

The mission segment solver reproduces `scipy.optimize.fsolve`, which is a
wrapper over this routine. It is translated directly rather than reimplemented
so that convergence behaviour can be compared against the Python implementation
rather than merely approximated.

Affected crate: `alas-math`.

---

## Bundled data

### UIUC Airfoil Coordinates Database

University of Illinois at Urbana-Champaign Applied Aerodynamics Group.
<https://m-selig.ae.illinois.edu/ads/coord_database.html>

1,665 airfoil coordinate sets, repacked from the individual `.dat` files
distributed with AeroSandbox into a single indexed blob. Each entry's bytes are
carried verbatim; no coordinate value is modified, reordered or resampled.

### NeuralFoil trained weights

MIT, with NeuralFoil above. Embedded as `f32` arrays converted from the
upstream `.npz` files without retraining or modification.

### Bundled Noto typefaces

The desktop application embeds Noto Sans, Noto Sans Mono, and Noto Sans Math
from the Google Fonts repository revision
`e44c4b011a820c2cbe2fd2cfa8052037d7edb571`. They are used by both the egui
text atlas and the SVG figure rasterizer, so Greek and engineering notation do
not depend on host-font discovery. The exact source paths, SHA-256 values, and
complete SIL Open Font License 1.1 texts are retained under
`crates/alas-fonts/assets/`.

The `egui`/`eframe` and SVG rendering dependency graph still carries its own
default font data and declares OFL-1.1 and Ubuntu Font License entries in
`deny.toml`; those dependency notices apply to the corresponding dependency
binaries. See `docs/dependency-policy.md` for the current dependency-level
audit.

### Native file dialogs: rfd (MIT)

The desktop application links `rfd` (https://github.com/PolyMeilex/rfd, MIT
license) for its "Browse..." file and folder dialogs, with its small
transitive additions `pollster` (Apache-2.0 OR MIT) and `block2` (MIT). See
`docs/dependency-policy.md` for the feature selection.

### Airport and engine reference data

Compiled by the author from published sources, each cited next to its entry.
Not a redistribution of any third-party database.

### Departure obstacle surveys: ODbL-1.0 (OpenStreetMap), terrain tiles, FAA DOF

`crates/alas-config/data/airport_obstacles.json`, generated by
`tools/airport_obstacles.cjs`, holds per-runway-direction terrain grids and
discrete obstacles of the curated aerodromes. It contains information from
OpenStreetMap (tall structures with a height tag, node and way ids kept as
source references), © OpenStreetMap contributors, available under the Open
Database License (ODbL) 1.0 (<https://www.openstreetmap.org/copyright>,
<https://opendatacommons.org/licenses/odbl/1-0/>). The file as a whole is a
Derivative Database: it is **not** relicensed under this program's
AGPL-3.0-or-later licence, anyone who distributes it or a database derived from
it must keep this attribution and offer it under the ODbL (share-alike); the
program only reads it. Its terrain grids come from the Mapzen/AWS Terrain Tiles
(<https://registry.opendata.aws/terrain-tiles/>) and carry the attributions
required there (<https://github.com/tilezen/joerd/blob/master/docs/attribution.md>):
Australia terrain data © Commonwealth of Australia (Geoscience Australia) 2017;
Austria terrain data © offene Daten Österreichs – Digitales Geländemodell (DGM)
Österreich; Europe terrain data produced using Copernicus data and information
funded by the European Union - EU-DEM layers; Mexico terrain data source:
INEGI, Continental relief, 2016; United Kingdom terrain data (LIDAR Composite DTM) © Environment
Agency copyright and/or database right 2015, all rights reserved; United States
3DEP (formerly NED) and global GMTED2010 and SRTM terrain data courtesy of the
U.S. Geological Survey; Global ETOPO1 terrain data U.S. National Oceanic and
Atmospheric Administration. The FAA Digital Obstacle File (US aerodromes) and
the OurAirports runway ends are public domain.

### CADO airplane database v1.3: ODbL-1.0

Nicolas Monrolin, Thierry Druot, Nicolas Peteilh, Pascal Roches and
Yri-Amandine Kambiri, ENAC (École Nationale de l'Aviation Civile).
*CADO airplane database*, Version 1.3, Recherche Data Gouv, 2024.
DOI [10.57745/LLRJO0](https://doi.org/10.57745/LLRJO0).

Contains information from the CADO airplane database, which is made available
under the Open Data Commons Open Database License (ODbL) 1.0:
<https://opendatacommons.org/licenses/odbl/1-0/>.

The database is stored unmodified in `docs/data/cado/` as a separately licensed
database, with the complete ODbL text in `docs/data/cado/DATABASE_LICENSE.txt`
and its provenance and digests in `docs/data/cado/README.md`. It is **not**
relicensed under this program's AGPL-3.0-or-later licence, and no production
code reads or embeds it. It is a secondary compilation that its authors state
is not intended for operational use. A small number of individual values are
cited from it as secondary corroboration anchors in
`golden/aircraft/real_aircraft_parity.json` (`cite: "cado_enac_v1_3"`); that
file does not reproduce the database. Anyone who distributes the database, or
a database derived from it, must keep this attribution and offer that database
under the ODbL (share-alike).

### NASA Blue Marble

Public domain NASA raster. `assets/textures/earth_blue_marble.png` is checked
into this repository, embedded at compile time by the route/report/pipeline
renderers, and included in release source archives. It is not downloaded on
demand and is not user-configurable at runtime.

### Natural Earth 110m land polygons

Public domain (Natural Earth terms of use: <https://www.naturalearthdata.com/about/terms-of-use/>).
`assets/textures/natural_earth_110m_land.geojson` (`ne_110m_land`, SHA-256
`3D34C10CEBFDBA17F5F97180DD7C5100C8C5E05F2F583061AA8DE2E9F942A904`) is checked
into this repository and embedded at compile time for the globe and route
renderers. No attribution is required; it is listed so this file stays the
inventory of bundled data.

### CPACS 3.5 schema: Apache-2.0

`external tools/cpacs_schema.xsd` is the CPACS 3.5 XML schema published by the
German Aerospace Center (DLR), <https://www.cpacs.de/>. It is used unmodified
to validate exported aircraft descriptions and is licensed under the Apache
License, Version 2.0, whose notice heads the file
(<http://www.apache.org/licenses/LICENSE-2.0>). SHA-256
`BE204E1E1C552FAE67A57073EE3DBD05A652FFB91E974DF910807A4A308AFD8F`. The ALAS
extension schema `external tools/alas_uav_cpacs35.xsd` is part of this program
and follows its licence.

### XFOIL 6.99 Orr–Sommerfeld map: GPL-2.0-or-later

The release includes `assets/mses/osmapDP.dat`, an unmodified
double-precision Orr–Sommerfeld lookup database extracted from Mark Drela's
official XFOIL 6.99 source archive:
<https://web.mit.edu/drela/Public/web/xfoil/xfoil6.99.tgz>.

The exact archive is retained beside the map as
`assets/mses/xfoil6.99.tgz`, together with the complete GPL text in
`assets/mses/COPYING-XFOIL.txt`, the provenance and compatibility record in
`assets/mses/README.md`, and the reproducible acquisition script in
`assets/mses/acquire_osmap.ps1`. The map is consumed by an MSES process for
free boundary-layer transition; it does not contain or redistribute the MSES
executables. The map SHA-256 is
`2F6B3C63461D71DA9B6CB9CA1340D77CFF0CFBE767679D15B5B8556B45D948C4`.

---

## Downloaded on demand

Fetched at the user's request, cached locally, never redistributed in this
repository or in a release archive.

### X-Plane navigation data: GPL-3.0

Airway and fix data used for airway routing. Downloaded from a public mirror
when the user enables real airway routing; the program falls back to
great-circle routing when it is absent.

### APC propeller performance data (`PER3_*.dat`)

Propeller thrust and power coefficient tables published by APC Propellers
(Landing Products, Inc.) at
<https://www.apcprop.com/technical-information/performance-data/>. The files
are APC's property, carry no stated licence, and are **not** distributed with
ALAS (source, releases, or tests). The UAV electric-propulsion features read
them from a local directory that the user fills with
`cargo run -p alas-uav --release --example fetch_props` (polite, opt-in
download with robots.txt respected and 5 s between requests) or by importing
files already held. Tests use made-up synthetic tables. See
`docs/propeller-data.md`.

### UIUC Propeller Data Site

Independent wind-tunnel propeller measurements by the University of Illinois at
Urbana-Champaign (Selig and collaborators),
<https://m-selig.ae.illinois.edu/props/propDB.html>. Not distributed with ALAS.
Anyone comparing a table against these measurements should download the files
from that site and cite its volumes and the authors named there; ALAS keeps no
copy and no excerpt.

---

## Invoked executables

Separate programs invoked through process boundaries. A distribution may
aggregate the open-source programs identified below; proprietary programs
remain user-supplied. AVL specifically ships as its unchanged win32
executable only in the Windows package, the only build this project has
reviewed and tested; every other package (including Linux) ships the same
GPL-2.0 corresponding source and licence text with a written build/
acquisition note instead of an executable that platform cannot run; see
`docs/release-packaging.md` and `xtask/src/dist_avl.rs`.

| Program | Licence | Used for |
|---|---|---|
| MSES (`mset`, `mses`, `mplot`) | Proprietary, per-seat from MIT | Two-dimensional viscous airfoil analysis |
| OpenVSP / VSPAERO | NASA Open Source Agreement, as supplied by the selected OpenVSP release | Geometry export, native CAD screenshots, and independent three-dimensional aerodynamic checks |
| CPython 3.13.7 and NumPy 2.3.3 (optional app-local preview runtime) | Python Software Foundation license; NumPy BSD license and bundled dependency notices | Runs OpenVSP's graphics API in a separate process; downloaded by `tools/setup_openvsp_preview.ps1`, with upstream licenses retained |
| MSC Nastran | Proprietary | Wingbox statics, normal modes, vibration |
| MSC Patran | Proprietary | Structural preprocessing and post-processing launcher |
| NASTRAN-95 | NOSA 1.3 | Wingbox statics and normal modes, where MSC Nastran is unavailable |
| AVL | GPL-2.0 | Independent vortex-lattice and dynamic-mode cross-check |
| FLOWUnsteady adapter / Julia environment | User-supplied; licence follows the selected external release | Optional lifting-surface unsteady analysis through a process boundary |

The committed AVL files are identified by SHA-256 so the corresponding-source
claim can be checked against what is in the repository: `external tools/avl352.exe`
`443520D255408491222A8DF9060BD000F78DA95A68845A2F6EFDBC67E203F07A` and
`external tools/avl3.52.tgz`
`0B588ECEA9222F5B625D0AF0C87AE31DAF3CDBA1532CF0BBB36F93D6E854849B`. No build
recipe in this repository links the executable to the source archive.

The release review uses these primary licence or provenance references. The
reference identifies the governing terms; it does not turn a user-selected
installation into a reviewed bundle:

- MSES: [MIT Technology Licensing Office commercial-use EULA](https://web.mit.edu/tlo/documents/MSES_EULA_commercial_2021_05.pdf).
  The exact MSES installation and any seat or field-of-use terms must be
  reviewed before redistribution, so ALAS keeps it user-supplied.
- OpenVSP/VSPAERO: [official OpenVSP NASA Open Source Agreement page](https://openvsp.org/license.shtml).
  A particular binary still needs its matching source, notices, and
  modification record before it can enter a package.
- AVL: [the upstream MIT AVL distribution](https://web.mit.edu/drela/Public/web/avl/),
  with the GPL text carried as `external tools/AVL-GPL-2.0.txt` when AVL is
  bundled.
- NASTRAN-95: the repository's [fail-closed bundle policy](docs/NASTRAN95-BUNDLE.md)
  records the exact source, build, NOSA notice, and runtime evidence required
  for a reviewed aggregation.
- FLOWUnsteady: [the upstream project and its MIT licence](https://github.com/byuflowlab/FLOWUnsteady).
  ALAS does not assume that one Julia environment's dependency closure or
  selected release is covered by that upstream notice, so the adapter remains
  user-supplied.

The one-click tool installer (`docs/tool-installer.md`) downloads publisher
archives only at the user's request, verifies a pinned SHA-256, and installs them
under the user's data folder; ALAS does not redistribute them, and each keeps its
own licence. A tool without a maintainer-reviewed pin is never fetched.

NOSA 1.3 is not compatible with the GPL family. NASTRAN-95 is therefore invoked
as a separate executable and nothing of it is linked or translated into this
program; the two are aggregated, not combined. If a NASTRAN-95 build is
redistributed alongside a release, it must carry its own NOSA notices intact.
ALAS release packaging additionally requires the exact corresponding source,
frozen source revision, local modification record, build marker, runtime and
rigid-format files; a missing item blocks archive creation.
