# Licensing

## ALAS

ALAS is free software under the GNU Affero General Public License, version 3
or any later version (AGPL-3.0-or-later). Copyright 2026 Marcos Quiroga
Rodriguez. It comes with no warranty.

- Source code of every release: [github.com/MarcosQuirogaR/ALAS](https://github.com/MarcosQuirogaR/ALAS).
  Each release package also contains the corresponding source snapshot.
- Licence text: the `LICENSE` file in the repository and in every package.
  `NOTICE` explains what the licence means for this program.
- ALAS is a single desktop program. It runs no server, so the network clause
  (AGPL section 13) does not apply to normal use. It makes outbound requests
  only when you ask for them (navigation data, runtime downloads).
- The name and logo "ALAS" are not licensed by the AGPL. A fork may not
  present itself as ALAS. Stating that software is based on ALAS is allowed.

In the application, **Help > About ALAS** shows the licence, the source
address, the third-party notices and the list of Rust dependencies.

## What a release package contains

| File | Content |
|---|---|
| `LICENSE` | AGPL-3.0 text |
| `NOTICE` | Copyright, licence scope, authorship, trademarks |
| `THIRD-PARTY-NOTICES.md` | Translated code, bundled data, downloads, external tools |
| `THIRD-PARTY-CRATES.md` | Generated list of the Rust crates in the executable, with licences |
| `THIRD-PARTY-LICENSES.txt` | Licence and notice texts shipped by those crates |
| `source/` | Corresponding source snapshot, with `SOURCE-MANIFEST.json` |

The full notices are on GitHub:
[THIRD-PARTY-NOTICES.md](https://github.com/MarcosQuirogaR/ALAS/blob/main/THIRD-PARTY-NOTICES.md)
and
[THIRD-PARTY-CRATES.md](https://github.com/MarcosQuirogaR/ALAS/blob/main/THIRD-PARTY-CRATES.md).

## Third-party components

**Translated code.** These are derivative works; the upstream licence applies
to the translated modules, and each source file names its origin.

| Component | Licence | Used for |
|---|---|---|
| SUAVE 2.5.2 | LGPL-2.1, used under GPL-2.0-or-later | Mission, weights, stability, turbofan network, vortex lattice |
| AeroSandbox 4.2.8 | MIT | Geometry, vortex lattice, Torenbeek weights, flight-dynamics modes |
| NeuralFoil | MIT | Airfoil polar surrogate and weights |
| MINPACK `hybrd` | Public domain | Mission segment solver |

**Rust dependencies.** Over 400 crates, mostly MIT or Apache-2.0, with
Unicode-3.0, BSD, ISC, Zlib, BSL-1.0, MPL-2.0 and CC0-1.0 entries. All are
compatible with AGPL-3.0-or-later and are checked against an allowlist
(`deny.toml`). The list is generated from `Cargo.lock`.

**Bundled data and fonts.**

| Item | Licence |
|---|---|
| Noto Sans, Noto Sans Mono, Noto Sans Math | SIL OFL 1.1 |
| egui default fonts (Hack, Ubuntu-Light, Noto Emoji, emoji-icon-font) | MIT, Ubuntu Font Licence 1.0, SIL OFL 1.1 |
| UIUC airfoil coordinates (1,665 sets, unchanged) | Published by the UIUC Applied Aerodynamics Group |
| XFOIL 6.99 Orr-Sommerfeld map (`osmapDP.dat`) | GPL-2.0-or-later |
| NASA Blue Marble globe texture | Public domain |

## External tools

External tools are separate programs. ALAS starts them as child processes
and nothing of them is linked into ALAS.

| Tool | In the package |
|---|---|
| AVL 3.52 (GPL-2.0) | Windows package: unchanged executable with source and licence text. Linux package: source and licence text only |
| NASTRAN-95 (NASA Open Source Agreement 1.3) | Only when the complete reviewed source and notice set is staged |
| MSES | No. It needs your own per-seat licence from MIT |
| MSC Nastran, MSC Patran | No. They need your own licence |
| OpenVSP, VSPAERO, OpenFOAM, Gmsh, ParaView, FLOWUnsteady (Julia) | No. You install them yourself |

Optional downloads happen only after you give consent, and ALAS does not
redistribute any of them:

- Navigation data (airway and fix files) from the third-party GitHub mirror
  mcantsin/x-plane-navdata.
- APC propeller performance files from APC Propellers (apcprop.com), used by
  the UAV propeller model.
- The OpenVSP preview runtime (CPython 3.13.7, the OpenVSP Python bindings and
  NumPy 2.3.3), fetched with pinned checksums.

See [External tools](external-tools.md) for setup.

## Data sources

- Aircraft presets use dimensions, masses and performance from public
  documents (airport planning manuals, type-certificate data sheets,
  manufacturer publications). Each preset cites its sources.
- The UAV component catalogue holds manufacturer-published specifications and
  retailer prices with a source address per record. Prices are snapshots.
- The CADO airplane database (ENAC, ODbL-1.0) is kept in the repository as a
  separately licensed reference and is not part of the program.

## Trademarks

Airbus, Boeing, McDonnell Douglas, Embraer, COMAC, ATR and other aircraft,
engine and manufacturer names belong to their owners. They are used only to
identify the modelled aircraft. ALAS is not affiliated with, endorsed by or
certified by any of them. Results are preliminary engineering estimates and
not manufacturer data.

## This documentation

The pages are built with MkDocs (BSD-2-Clause), the Material for MkDocs theme
and PyMdown Extensions (MIT), and Python-Markdown (BSD-3-Clause). Equations
are rendered by MathJax 3.2.2 (Apache-2.0), bundled with the docs. The
documentation loads no fonts or scripts from external servers.
