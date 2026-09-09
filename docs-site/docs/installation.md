# Installation & Setup

ALAS is built in Rust as a single native executable. There is no Python runtime,
no package manager, and no background sidecar service required.

## Executable & Release Status

### Windows

- **Upcoming Release Candidate**: Remains under verification as a native
  `alas.exe` binary (Rust version 0.1.0) and is not published. The core application runs locally;
  full multidisciplinary analyses require compatible external solver executables and any
  necessary licenses.
- **Legacy binary (v1.0.0, 2026-07-29)**: Historical Python 1.0.0 release (distinct
  from the current Rust version 0.1.0), available on GitHub Releases for reference while the current
  candidate undergoes verification.
- **SmartScreen**: Windows may display an untrusted application warning on first
  launch of newly published binaries. Select **More info → Run anyway**.

### Other Platforms

This candidate's Windows distribution is currently being verified; builds for other
platforms are not verified here at this time.

---

## What runs out of the box

Within the core local application, the following capabilities execute without external solvers:

- Parametric airframe sizing and design space definition
- Numerical airframe optimization (Differential Evolution, NSGA-II, SQP)
- Vortex-lattice aerodynamics and empirical drag build-ups
- Turbofan cycle thermodynamic modeling
- Wingbox structural sizing and analytical rib spacing
- Mass distribution, center of gravity limits, and longitudinal stability margins
- Native mission trajectory simulation (climb, cruise, descent, reserves)
- Passenger cabin deck arrangement and payload loading

---

## Compatible external tools

ALAS couples with specialized external analysis tools across disciplines. Compatible
tools are required as documented for each distribution; each stage reports itself
unavailable when a tool is absent:

| Tool | Discipline | Status when absent |
|---|---|---|
| **MSES** | Transonic section coupled Euler/boundary-layer analysis | Reported unavailable if absent |
| **AVL** | Extended vortex-lattice aerodynamic cross-check | Reported unavailable if absent |
| **OpenVSP / VSPAERO** | CAD geometry generation and aerodynamic cross-check | Reported unavailable if absent |
| **MSC Nastran / NASTRAN-95** | Finite-element structural analysis and vibration modes | Reported unavailable if absent |
| **Patran** | Finite-element structural visualization | Reported unavailable if absent |

Tool directories and executable paths are configured in the application under
**Setup → External Tools** or through configuration files.

Packaging and solver integration notes:

- **AVL**: Packaging currently includes AVL with separate GPL source and notices.
- **NASTRAN-95**: Optional and user-supplied unless a reviewed bundle is explicitly staged.
- **MSES / MSC Nastran / Patran**: Remain user-supplied with required licenses.
- **OpenVSP**: May run headless; preview fallback projects the actual VSPGEOM mesh, explicitly labelled, rather than a screenshot from the native GUI.

---

## Navigation & route data <a id="optional-route-data"></a>

Native mission simulation supports both great-circle tracks and published airway
routes. Airway navigation data can be downloaded on demand through application settings
or via the CLI:

```powershell
alas --download-navdata
```

---

## Building from source (developers)

For developers with an authorized source checkout:

1. Install Visual Studio Build Tools with the **Desktop development with C++** workload.
2. Install Rust via [rustup.rs](https://rustup.rs).
3. Build and launch:

```powershell
# Launch interactive desktop interface
cargo run --release --bin alas

# Run headless pipeline
cargo run --release --bin alas -- --seed 42 --output outputs --plots
```

The compiled binary is written to `target/release/alas.exe` (or `target/release/alas`).
