# Third-party notices

ALAS is distributed under AGPL-3.0-or-later (see `LICENSE` and `NOTICE`). It
incorporates, bundles, or interoperates with the components below, each under
its own licence. This file is the attribution record those licences require.

Licences are stated as published by each project at the version ALAS depends
on. Where a project's own metadata is ambiguous, that is called out rather
than guessed.

---

## 1. Vendored in this repository

Source that is committed to this tree and redistributed with it.

| Component | Version | Licence | Location |
|---|---|---|---|
| [SUAVE](https://github.com/suavecode/SUAVE) | 2.5.2 | LGPL-2.1 | `external tools/SUAVE-2.5.2/` |
| [pint](https://github.com/hgrecco/pint) (vendored inside SUAVE) | bundled | BSD-3-Clause style | `external tools/SUAVE-2.5.2/trunk/SUAVE/Plugins/pint/` |
| [Nunito](https://fonts.google.com/specimen/Nunito) | v16 | SIL OFL-1.1 | `desktop/frontend/src/assets/fonts/` |

**SUAVE** (Stanford University Aerospace Vehicle Environment) provides mission
and trajectory analysis. Its `LICENSE` and copyright notices are preserved
unmodified in its directory, and its source is not altered. ALAS invokes it as
a subprocess in an isolated Python 3.10 environment rather than importing it,
so the two never share a process. See `NOTICE` for why the LGPL-2.1/AGPL-3.0
combination is permitted.

**Nunito** ships with its `OFL.txt` alongside the font files, as OFL-1.1
requires.

---

## 2. Bundled data

| Asset | Source | Licence | Location |
|---|---|---|---|
| Airfoil coordinate database | [UIUC Applied Aerodynamics Group](https://m-selig.ae.illinois.edu/ads/coord_database.html) | **Unstated upstream** -- see below | `alas/data/coord_seligFmt.zip` |
| Earth surface texture ("Land Shallow Topo") | [NASA Earth Observatory](https://visibleearth.nasa.gov/) | Public domain | `alas/data/textures/` |
| NASA SC(2)-0714 section coordinates | NASA published research | Public domain | `alas/data/airfoil_data.py` |

**Airfoil database.** Roughly 1,600 sections from the UIUC Airfoil Coordinates
Database, compiled by Michael Selig and the UIUC Applied Aerodynamics Group.
The database carries no explicit licence, and individual contributed sections
have varied provenance. It is included here as factual coordinate data, with
attribution to its compilers. If you are a rights holder in any section and
object to its inclusion, please open an issue.

**Earth texture.** NASA imagery is generally not subject to copyright. NASA
requests, but does not require, credit: *NASA Earth Observatory (Reto Stöckli,
Robert Simmon)*.

---

## 3. Downloaded on demand -- not distributed

| Asset | Source | Licence |
|---|---|---|
| Enroute navdata (`earth_fix.dat`, `earth_awy.dat`, `earth_nav.dat`) | X-Plane project, via the [mcantsin/x-plane-navdata](https://github.com/mcantsin/x-plane-navdata) mirror | **GPL-3.0** |

This data is **not** committed to this repository and **not** included in
released binaries. It is fetched at the user's explicit request, from
Setup > External Tools or `scripts/download_navdata.py`, and the licence is
stated before the download is offered. Without it, routing falls back to a
great-circle path.

It is excluded because GPL-3.0 obligations would otherwise attach to every
redistribution of ALAS. Anyone who redistributes a build with this data
included takes on those obligations themselves.

---

## 4. External programs -- not distributed

Proprietary tools that ALAS invokes when the user has supplied and licensed
them. None is included in this repository or in any release. ALAS is fully
functional without all of them; the features they drive degrade with an
explicit message.

| Program | Vendor | Feature |
|---|---|---|
| MSES | Mark Drela / MIT Technology Licensing Office | 2-D coupled viscous/inviscid airfoil analysis |
| MSC Nastran | Hexagon / MSC Software | Finite-element wingbox solve (an analytical estimate is always available) |
| MSC Patran | Hexagon / MSC Software | Deformation-plot export |

---

## 5. Python dependencies

Runtime dependencies resolved by `pyproject.toml`.

| Package | Licence |
|---|---|
| AeroSandbox | MIT |
| NumPy | BSD-3-Clause |
| SciPy | BSD-3-Clause |
| Matplotlib | Matplotlib licence (BSD-compatible, PSF-derived) |
| PyYAML | MIT |
| pyNastran | BSD-3-Clause |
| FastAPI | MIT |
| Uvicorn | BSD-3-Clause |
| websockets | BSD-3-Clause |
| PyVista | MIT |
| VTK | BSD-3-Clause |
| **CasADi** (via AeroSandbox) | **LGPL-3.0-or-later** |
| NeuralFoil | MIT |
| pandas | BSD-3-Clause |
| seaborn | BSD-3-Clause |
| Pillow | HPND (MIT-CMU style) |
| **tqdm** | **MPL-2.0** and MIT |
| **certifi** | **MPL-2.0** |
| requests | Apache-2.0 |
| urllib3 | MIT |
| fonttools | MIT |
| pooch | BSD-3-Clause |
| sortedcontainers | Apache-2.0 |
| rich, click, pygments | MIT / BSD-3-Clause |

**CasADi** is LGPL-3.0, pulled in by AeroSandbox, and is statically bundled by
PyInstaller in released binaries. LGPL-3.0 section 4 requires that recipients
be able to relink against a modified CasADi; publishing the complete
corresponding source of ALAS under AGPL-3.0, together with the build scripts
in `scripts/`, satisfies that requirement.

**tqdm** and **certifi** are MPL-2.0, which is file-level copyleft and
compatible with AGPL-3.0 in this arrangement.

### Build-time only

| Tool | Licence |
|---|---|
| PyInstaller | GPL-2.0-or-later **with the standard bundling exception** permitting distribution of non-free programs built with it |
| pyinstaller-hooks-contrib | Apache-2.0 / GPL-2.0 |
| pytest | MIT |
| uv | Apache-2.0 / MIT |

PyInstaller's exception is what allows a frozen binary to carry any licence;
it does not impose GPL-2.0 on ALAS.

---

## 6. JavaScript / TypeScript dependencies

Direct dependencies of `desktop/frontend/package.json`. All permissive; no
copyleft.

| Package | Licence |
|---|---|
| React, React DOM | MIT |
| three | MIT |
| three-globe | MIT |
| Vite, @vitejs/plugin-react | MIT |
| TypeScript | Apache-2.0 |
| @types/* (DefinitelyTyped) | MIT |

Notable transitive dependencies: `d3-*` (ISC / BSD-3-Clause), `h3-js`
(Apache-2.0), `meshoptimizer` (MIT), `@tweenjs/tween.js` (MIT), `fflate` (MIT),
`earcut`, `delaunator`, `robust-predicates`, `lodash-es`, `kapsule`,
`tinycolor2`.

---

## 7. Go dependencies

Direct and indirect dependencies of `desktop/go.mod`. All permissive; no
copyleft.

| Module | Licence |
|---|---|
| github.com/wailsapp/wails/v2 | MIT |
| github.com/wailsapp/go-webview2, mimetype | MIT |
| github.com/labstack/echo, gommon | MIT |
| github.com/leaanthony/* | MIT |
| github.com/godbus/dbus, gorilla/websocket, pkg/errors, pkg/browser | BSD-2-Clause |
| github.com/google/uuid | BSD-3-Clause |
| github.com/tkrajina/go-reflector | Apache-2.0 |
| github.com/go-ole/go-ole, mattn/*, rivo/uniseg, samber/lo, valyala/*, bep/debounce, jchv/go-winloader | MIT |
| git.sr.ht/~jackmordaunt/go-toast | MIT |
| golang.org/x/crypto, net, text, sys | BSD-3-Clause |

---

## 8. Methods and references

The following are cited as the sources of implemented methods. They are
literature references, not software dependencies, and carry no licence
obligation -- but the work is theirs and is credited here.

- Raymer, D. P., *Aircraft Design: A Conceptual Approach*, 5th ed. -- mass
  estimation, drag build-up, landing-gear and performance sizing.
- Torenbeek, E., *Synthesis of Subsonic Airplane Design* (1982) -- component
  mass fractions and geometry relations.
- Drela, M. -- MSES coupled viscous/inviscid formulation.
- Whitcomb, R. T. / NASA -- supercritical SC(2) sections, area rule.
- Korn, D. -- transonic wave-drag correlation.

Per-method citations, with the equation and the module implementing each, are
in `docs/methods.md`.

---

## Reporting an omission

If a component is missing, misattributed, or its licence is stated
incorrectly, please open an issue. Attribution errors are treated as bugs.
