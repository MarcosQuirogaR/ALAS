# Airfoil CFD workflow

`alas-cfd` provides a standalone, reproducible two-dimensional OpenFOAM study
for a section selected from the ALAS airfoil database.  It does not require a
whole-aircraft or mission analysis.  A study stores the selected database
identity, the exact coordinate snapshot and hash, the SI operating point, the
boundary and mesh settings, the solver settings, and the versioned case
template in `study.json`.

The current template is `alas-airfoil-2d-openfoam-gmsh-v2`.  Gmsh creates an
exact straight-edge polygon from the section coordinates, a chord-scaled outer
domain, and a one-layer extrusion.  `gmshToFoam` converts the mesh and the
runner enforces these patch contracts:

| Patch | Role | Constraint |
| --- | --- | --- |
| `airfoil` | section wall | `wall` |
| `inlet` | upstream boundary | `patch` |
| `outlet` | downstream boundary | `patch` |
| `farField` | outer boundary | `patch` |
| `frontAndBack` | two-dimensional extrusion faces | `empty` |

The runner refuses to use an existing non-empty case directory.  This keeps a
new geometry or operating point from reusing an old mesh, field, or force
history.  GUI runs allocate a unique directory for each case and each sweep
point.

## Supported execution backends

The adapter keeps backend-specific command construction outside the case
builder.  `Automatic` probes the native OpenCFD installation before WSL2;
`Native Windows` uses a configured `bin` directory and project directory;
`WSL2 Linux` launches utilities through `wsl.exe` and translates the case path
to `/mnt/<drive>/...`.  Commands are passed as argument vectors, so spaces and
Unicode in a case path do not depend on shell quoting.

The required OpenFOAM utilities are `gmshToFoam`, `checkMesh`, `simpleFoam`,
and `postProcess`.  Gmsh is a separate required dependency for the current
mesh route.  `potentialFoam` is optional: when available, the runner executes
`potentialFoam -initialiseUBCs -writephi` before SIMPLE and records its output.
The native serial OpenCFD distribution uses its serial Pstream library.  MPI
parallel execution is not enabled by the initial template until a matching
MPI installation and serial/parallel agreement have been verified.

The external-tools card persists `OpenFoamPreferences`, including backend,
native and WSL paths, Gmsh path, per-command timeout, and the parallel limit.
The probe reports each utility, the detected OpenFOAM version, and missing
dependencies.  A run captures bounded stdout and stderr, writes one log file
per utility under `logs/`, forwards selected live diagnostics to the GUI, and
terminates the owned process tree on cancellation or timeout.

For a native Windows smoke run, configure the directories and run the example
from the repository root:

```powershell
$env:ALAS_OPENFOAM_BIN = 'C:/path/to/OpenFOAM-v2606/platforms/win64MingwDPInt32Opt/bin'
$env:ALAS_OPENFOAM_PROJECT = 'C:/path/to/OpenFOAM-v2606'
$env:ALAS_GMSH = 'C:/path/to/gmsh.exe'
cargo run -p alas-cfd --example run_airfoil_case -- outputs/airfoil-cfd/naca0012 naca0012
```

The example accepts `ALAS_CFD_MAX_ITERATIONS` and
`ALAS_CFD_STARTUP_ITERATIONS` for short smoke cases.  `ALAS_CFD_SCHEME=upwind`
selects the first-order bounded scheme and disables staging; the normal
default uses bounded upwind startup followed by bounded linear-upwind velocity
and limited-linear turbulence transport.  A short run is deliberately
reported as `unconverged` even when every process exits successfully.

## Operating point and physical conventions

All values written to dictionaries are SI.  The independent input is either
speed or chord Reynolds number.  The linked quantity follows

```text
Re = rho U c / mu
```

The frame is chord `+x`, section normal `+y`, and extrusion `+z`.  Positive
angle of attack sets
`U = (U cos(alpha), U sin(alpha), 0)`.  Drag is positive along the freestream
and lift is positive ninety degrees counter-clockwise in the section plane.
The force reference is quarter chord at the extrusion mid-plane and the
reference area is `chord * extrusion_span`, with the initial extrusion span
equal to `0.01 * chord`.  OpenFOAM's `forceCoeffs` pitch convention is about
`-z`; this is recorded in the frame provenance and used by the surface
integrator.  Pressure in the incompressible `p` field is kinematic pressure in
`m^2/s^2`; the GUI accepts a physical pressure reference in Pa and the case
converts it by dividing by density.

The initial solver is incompressible steady `kOmegaSST`.  The default
external-flow turbulence input is intensity `0.052%` and turbulent-to-molecular
viscosity ratio `0.009`; the generated `k`, `omega`, estimated eddy viscosity,
and implied length scale are recorded in the case README.  A length-scale
input remains available for tunnel or inflow data specified that way.  The
wall treatment is `kqRWallFunction`, blended `omegaWallFunction`, and
`nutUSpaldingWallFunction`, with a default 25-layer boundary-layer field,
first-cell-centre target of `1e-5 m`, and target `y+ = 1`.  The flat-plate
estimate is sizing evidence only.  The solved yPlus summary is retained in
`MeshQuality.near_wall` when the solver emitted a finite patch result.

The initial model is intended for attached or mildly separated turbulent
section flow at low Mach number.  It does not silently claim validity for
laminar or transition-sensitive flow, low-Reynolds separation, stall,
transonic compressibility, or unsteady shedding.  Domain extents and
turbulence inputs remain engineering settings; they require a grid/domain
study and matched validation data before coefficients are used for a design
decision.

## Lifecycle and acceptance

Each run executes geometry preparation, Gmsh meshing, OpenFOAM conversion,
boundary patch correction/inspection, `checkMesh`, optional potential-flow
initialization, SIMPLE solution, and solver-attached post-processing.  A
non-zero process exit is a failure, and `checkMesh` must report `Mesh OK` with
no failed checks even when its exit code is zero.  The quality record retains
cell count, maximum non-orthogonality, maximum skewness, and minimum cell
volume.

Numerical convergence requires all of the following evidence from the same
latest outer SIMPLE iteration:

* finite initial and inner linear-solver residuals for `p`, `Ux`, `Uy`, `k`,
  and `omega`, with the initial residuals below the configured tolerance;
* a stabilized final-stage `Cd`, `Cl`, and `Cm` force window, excluding
  inherited startup samples after a staged restart;
* finite force coefficients and latest local/global continuity errors within
  the configured tolerance.

The cumulative continuity error remains audit evidence and is not compared
with a per-iteration limit.  Process completion without these checks is
`unconverged`, never a successful CFD result.  Cancellation, timeout, missing
dependencies, failed mesh checks, post-processing failure, and solver failure
retain their logs and a `results.json`/`report.md` failure artifact.

## Results and reproducibility

`results.json` contains residual and continuity histories, force coefficients,
pressure/viscous dimensional force decomposition, mesh quality, measured
near-wall statistics, field-artifact paths, and face-resolved surface samples
when the native fields can be paired safely.  The surface parser derives
dimensional pressure, `Cp`, signed skin friction `Cf` toward increasing
chordwise `x`, and a non-negative `Cf` magnitude.  It also integrates pressure
and viscous forces and moments using the configured density, freestream,
reference area, and moment reference.  Front/rear `Cd(f/r)` and `Cl(f/r)`
columns from modern `forceCoeffs` output are kept distinct from pressure and
viscous components; absent components remain unavailable.

The `fields` list points to native OpenFOAM fields and sampled raw outputs.
The GUI can open the case in a configured ParaView executable using the
generated `case.foam` marker.  The case folder, raw logs, native fields,
`results.json`, and `report.md` are exportable together.  The final provenance
records the selected backend, OpenFOAM version, and deterministic FNV-1a hashes
of generated case artifacts and executed utility binaries.  Results include
the exact coordinate hash and never replace a missing or malformed database
airfoil with a fallback section.

The GUI also supports sequential angle-of-attack or Reynolds sweeps.  Every
point gets its own isolated case and retains its own status, result, effective
speed/Reynolds values, and provenance.  Editing inputs invalidates the current
result revision, and a late worker result cannot replace a newer input state.

## Measured native smoke evidence

On 2026-09-13 the native OpenCFD v2606 MinGW serial installation was exercised
with Gmsh 4.15.2 on an AMD Ryzen 7 5800X (8 physical cores, 16 logical
processors).  The fresh NACA0012 case in
`.agent/openfoam-validation/native-run-010-final` generated 86,649 cells;
Gmsh, `gmshToFoam`, `checkMesh`, `potentialFoam`, staged `simpleFoam`, and
solver-attached post-processing all exited successfully.  The ten-iteration
smoke run was reported as `unconverged` because its final outer residuals had
not reached the configured threshold, and it recorded airfoil y+ from 0.299 to
2.820 (average 1.583), a finite face-resolved surface distribution, and the
complete case logs.  The installation checksum, paths, transport choice,
hardware, and the separate tutorial smoke result are recorded in
`.agent/openfoam-installation.json`.  These artifacts demonstrate executable
and bookkeeping integration; they are not an experimental accuracy claim.

## Verification boundaries

The CFD crate tests case contracts, closure handling, duplicate and
self-intersection rejection, Gmsh patch groups, boundary correction, SI
linking, turbulence derivation, force-header semantics, dimensional force
projection, residual grouping, non-finite rejection, mesh-quality failure
parsing, result aggregation, yPlus parsing, provenance, and stale-case
protection.  Native smoke cases have exercised the v2606 serial utilities and
the Gmsh extrusion route, including fixed-value and `freestream` boundary
conditions.

These checks verify implementation and numerical bookkeeping.  They do not
establish experimental accuracy.  Physical qualification still requires
representative symmetric and cambered sections, grid/domain independence,
near-wall sensitivity, serial/parallel agreement when MPI is enabled, and
traceable measurements matched in Reynolds number, Mach number, transition or
trip state, and moment/reference conventions.
