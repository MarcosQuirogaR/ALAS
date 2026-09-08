# Third-party notices

This file lists every third-party component whose code, data or interface this
program incorporates. It covers three distinct relationships, which have
different licensing consequences and are kept separate below:

- **Translated code** — a derivative work. The upstream licence applies to the
  translation, and every affected module carries a provenance header naming its
  origin, upstream licence and source revision.
- **Bundled data** — redistributed unmodified, or repacked without changing the
  values.
- **Invoked executables** — separate programs the user supplies. Not
  distributed here, not covered by this program's licence.

Cargo dependencies are not listed individually. `cargo tree` enumerates them and
`cargo deny` checks their licences; the workspace admits only permissive
licences (MIT, Apache-2.0, BSD, Zlib, Unicode-3.0).

---

## Translated code

### SUAVE 2.5.2 — LGPL-2.1

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

### AeroSandbox 4.2.8 — MIT

Peter Sharpe and contributors. <https://github.com/peterdsharpe/AeroSandbox>

The parametric geometry model (`Airplane`, `Wing`, `Fuselage`, `Airfoil` and
their query methods), the vortex-lattice method, the ISA atmosphere, the
Torenbeek weight correlations and the flight-dynamics mode approximations are
translated from AeroSandbox.

Affected crates: `alas-geom`, `alas-aero`, `alas-atmo`, `alas-mass`, `alas-stab`.

### NeuralFoil — MIT

Peter Sharpe and contributors. <https://github.com/peterdsharpe/NeuralFoil>

The airfoil-polar surrogate: network evaluation, the Kulfan/CST coordinate fit
that produces its inputs, and the output unpacking. The trained weights are
bundled as data — see below.

Affected crate: `alas-aero`.

### MINPACK `hybrd` — public domain

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

### DejaVu Sans

DejaVu Fonts Team. Licensed under the DejaVu Fonts License (a permissive
Bitstream Vera derivative). Embedded so that exported figures render
identically on machines without it installed, and so that they remain visually
comparable to the Python implementation's Matplotlib output.

### Airport and engine reference data

Compiled by the author from published sources, each cited next to its entry.
Not a redistribution of any third-party database.

---

## Downloaded on demand

Fetched at the user's request, cached locally, never redistributed in this
repository or in a release archive.

### X-Plane navigation data — GPL-3.0

Airway and fix data used for airway routing. Downloaded from a public mirror
when the user enables real airway routing; the program falls back to
great-circle routing when it is absent.

### NASA Blue Marble

Public domain. Earth surface texture for the route map and globe.

---

## Invoked executables

Separate programs invoked through process boundaries. A Windows distribution
may aggregate the open-source programs identified below; proprietary programs
remain user-supplied.

| Program | Licence | Used for |
|---|---|---|
| MSES (`mset`, `mses`, `mplot`) | Proprietary, per-seat from MIT | Two-dimensional viscous airfoil analysis |
| MSC Nastran | Proprietary | Wingbox statics, normal modes, vibration |
| NASTRAN-95 | NOSA 1.3 | Wingbox statics and normal modes, where MSC Nastran is unavailable |
| AVL | GPL-2.0 | Independent vortex-lattice and dynamic-mode cross-check |

NOSA 1.3 is not compatible with the GPL family. NASTRAN-95 is therefore invoked
as a separate executable and nothing of it is linked or translated into this
program; the two are aggregated, not combined. If a NASTRAN-95 build is
redistributed alongside a release, it must carry its own NOSA notices intact.
ALAS release packaging additionally requires the exact corresponding source,
frozen source revision, local modification record, build marker, runtime and
rigid-format files; a missing item blocks archive creation.
