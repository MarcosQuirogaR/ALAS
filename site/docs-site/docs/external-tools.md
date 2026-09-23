# External Tools Guide

ALAS runs its core multidisciplinary pipeline (vortex-lattice aerodynamics,
turbofan cycles, wingbox sizing, weight and balance, mission trajectories) as
a single native Rust application, no external solver required. For higher
fidelity or an independent cross-check, it can also drive a handful of
specialized external tools as separate child processes. This guide covers
where to point ALAS at each tool, what each produces, and how to read results.

The v1.1.0 release is a native Rust build. External solver stages remain
optional and user-supplied; their configuration is independent of the core
binary and its portable Windows and Linux packages.

## Setting Up External Tools

Everything lives on **Setup → External Tools**. Each solver has its own card
with a browse field for its install location and an **Open folder** button to
inspect it in the system file explorer. A live **Availability for the next
run** panel shows whether each path resolves, is absent, or is incomplete
(e.g., a launcher present with no solver binary) before you start an analysis.

The same page also holds mission-routing settings unrelated to solvers: a
**SimBrief username** and **Fetch timeout [s]** for importing a filed flight
plan, and separate **Navigation-data directory** / **Saved-routes directory**
fields for local route data.

Whatever you set here is written to `tool-preferences.json` under
`%LOCALAPPDATA%\ALAS` on Windows, a per-user file that survives reinstalls
without touching the project's own config files. Headless and scripted runs read
the same solver settings from the run's YAML/JSON config instead (`mses` and
`structures` sections, plus CLI overrides).

## Athena AVL

[AVL](https://web.mit.edu/drela/Public/web/avl/) is Mark Drela and Harold
Youngren's vortex-lattice solver, used here as an independent check against
ALAS's own lifting-line and VLM code. Both a packaged Windows release and a
source checkout keep AVL 3.52 at `external tools/avl352.exe` next to the
application, along with its GPL-2.0 notice and source archive; leave the
**Executable path** field on the AVL card empty and ALAS finds it there
automatically, or point the field at a different install. ALAS writes out the
`.avl` geometry deck it built for the run, the session script, console output,
and AVL's own force files, so any mismatch with ALAS's native result is easy
to trace. Only cases where lift coefficient and pitching
moment share a common reference frame with ALAS get overlaid in Model
Comparison; the rest still solve, but stay off that chart.

## OpenVSP & VSPAERO

[OpenVSP](https://openvsp.org/) builds parametric aircraft geometry and
[VSPAERO](https://openvsp.org/) solves 3D panel/vortex aerodynamics on it;
both are NASA Open Source Agreement software you install yourself; point the
**Install directory** field at the folder holding both `vspscript.exe` and
`vspaero.exe`. ALAS drives OpenVSP headlessly through `vspscript.exe`, so its
on-screen `ScreenGrab` preview isn't always available; when that happens the
results view reports the native CAD preview as unavailable, with the reason.
If the mesh export (`VSPGEOM`) itself succeeded, ALAS also builds its own
wireframe projection from that mesh, available even without the native PNG,
but if the export failed too, neither view exists.

ALAS runs each VSPAERO case with a fixed wake-iteration count and retains the
native wake-iteration history as one of the result's artifacts. It only marks
a run `completed_comparable` if the lift, drag, and moment coefficients have
stopped changing between the last two iterations, within a tight numerical
tolerance; otherwise the run stays `completed_not_comparable`, with the reason
recorded on the card. Either way, a numerically converged VSPAERO solve only
means its equations satisfied their tolerance; it says nothing about whether
the underlying aircraft shape is aerodynamically sound.

## MSES

[MSES](https://web.mit.edu/drela/Public/web/mses/) is MIT's coupled
viscous/inviscid solver for 2D transonic airfoil sections. It's proprietary;
ALAS does not ship it, so you need your own licensed installation and
executables (`mset.exe`, `mses.exe`, `mplot.exe`) pointed at from the **Install
directory** field on the MSES card.

Natural-transition (e^N) runs also need an Orr–Sommerfeld amplification-rate
table. As tested here, the installed Windows MSES build only accepts the
double-precision form (`osmapDP.dat`); the single-precision version some
XFOIL builds ship is rejected by ALAS's format check. This file isn't included
either; configure a verified, compatible resource from your own MSES/XFOIL
setup, either through the hidden `mses.osmap_path` config entry or the
`MSES_OSMAP` environment variable ALAS passes to the process.

MSES can converge cleanly at one angle of attack and fail to at the next, so
ALAS checks for the solver's own `"Converged on tolerance"` line in its output
rather than trusting that an `mplot` table exists. A polar sweep can therefore
finish `partial_convergence`, keeping the angles that converged and dropping
the rest from plots and regressions. ALAS first tries the pressure and
Mach-contour case at the exact trimmed cruise angle; if that fails, it retries
at a few nearby offset angles and attempts to bridge a converged offset back
to the exact angle in small steps. The result card reports the angle it
actually solved at, so a run that only reached a nearby offset is visible as
such rather than presented as the requested condition; if nothing converges
at all, the figures are marked unavailable.

## MSC Nastran & NASTRAN-95

For finite-element cross-checks against ALAS's own analytical wingbox sizing,
configure **MSC Nastran** (Hexagon, commercial license) and/or **NASTRAN-95**
(NASA Open Source Agreement 1.3, user-supplied) on the NASTRAN card. Beyond the
main **Executable path**, a **MSC solver override** covers Student Edition
installs where the visible launcher and the solver kernel are split; point it
at `analysis.exe` under Patran's `servermode` tree. NASTRAN-95 adds its own
**Local NASTRAN-95 directory**, a **Runtime DLL directory** for its GNU
Fortran runtime, and a **Short RF staging directory** under 38 bytes (e.g.
`C:/nas-rf`) for rigid-format files; ALAS checks that runtime directory for
the required DLLs before launching NASTRAN-95 and reports a clear error rather
than starting a solver that can't run. **Open-core words (OCMEM)** is an
optional memory override, blank by default. The **Run a real NASTRAN solve**
checkbox switches between an actual solve and ALAS's analytical sizing, which
also runs whenever a solver is absent or the box is unchecked.

Runs are organized as SOL 101 (linear statics: pull-up, push-down, 1g cruise),
SOL 103 (normal modes), and, when enabled, SOL 111 (modal frequency response,
including force-power-spectral-density RMS response if that option is on).

## MSC Patran

[Patran](https://hexagon.com/) is Hexagon's pre/post-processor. Point its
**Executable path** field at your own licensed install and check **Export/run
Patran (requires NASTRAN above)** to have ALAS run it in batch mode after a
successful SOL 101 solve, exporting deformation contour images per load case.
Without Patran, ALAS's own analytical deformation plots are still there in the
Structural Analysis results.

## FLOWUnsteady Adapter

[FLOWUnsteady](https://github.com/byu-cpc/FLOWUnsteady) (typically run in
Julia) is an optional path for unsteady lifting-surface or rotor analysis.
ALAS doesn't bundle Julia or FLOWUnsteady itself; instead, set the
`ALAS_FLOWUNSTEADY_EXE` environment variable to your own adapter script or
wrapper, which ALAS calls as
`<adapter> --alas-request <request-file> --alas-result <result-file>`.

## Reading Statuses and Evidence

Every external tool reports one of a small set of outcomes, shown on its card
under **Results → External tool evidence**: `not_configured`/absent (no usable
install found), `incomplete` (a folder exists but a required binary doesn't),
`launch_failed`/`timed_out`/`solver_failed` (process-level problems),
`output_missing`/`parse_failed` (the result file is gone or malformed), and
finally `completed_not_comparable` or `completed_comparable`, depending on
whether the result shares ALAS's reference frame closely enough to appear in a
comparison chart. A few tools add their own vocabulary: AVL can reject a
deck outright (`deck_rejected`), VSPAERO can fail before starting a case
(`setup_rejected`, `geometry_unavailable`), and MSES reports convergence per
point rather than per run, as above. Every card has a **Reveal** button to
open its artifacts directly, and the run's output directory keeps the raw
inputs, transcripts, and results per tool (`avl/`, `openvsp/`, `vspaero/`,
`mses/`, `structures/`); the stdout/stderr and setup files there are the
actual record of what each tool did. Each run also writes its own
`design_database.json`, an aircraft/config/feasibility export.
`RELEASE-MANIFEST.json` is a separate, one-off artifact produced when a
distributable ALAS package is built, not by individual analysis runs.
