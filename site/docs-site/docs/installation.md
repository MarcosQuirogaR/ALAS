# Installation & Setup

ALAS is built in Rust as a single native executable. There is no Python runtime,
no package manager, and no background sidecar service required.

## Release v1.2.0

### Windows

- **Windows**: Download the portable x86-64 archive from [GitHub Releases](https://github.com/MarcosQuirogaR/ALAS/releases/tag/v1.2.0), verify its SHA-256 file, and run `ALAS.exe`.
- **Linux**: Download the portable x86-64 `tar.gz` archive from the same release, verify its SHA-256 file, extract it, and run `./ALAS` (glibc 2.35 or newer).
- **Windows trust**: A signed binary identifies its publisher, but a new file can still show a SmartScreen reputation prompt until Microsoft has enough clean download history. Verify the publisher and checksum before running it.

### Other Platforms

The v1.2.0 release provides Windows and Linux x86-64 packages. External solver stages remain
optional and require compatible user-supplied installations and licences.

---

## What runs out of the box

Within the core local application, the following capabilities execute without external solvers:

- Parametric airframe sizing and design space definition
- Numerical airframe optimization (L-SHADE differential evolution, epsilon-constrained)
- Vortex-lattice aerodynamics and empirical drag build-ups
- Turbofan cycle thermodynamic modeling
- Wingbox structural sizing and analytical rib spacing
- Mass distribution, center of gravity limits, and longitudinal stability margins
- Native mission trajectory simulation (climb, cruise, descent, reserves)
- Passenger cabin deck arrangement and payload loading

---

## Compatible external tools

ALAS couples with specialized external analysis tools across disciplines. For full setup
and status code documentation, see the [External tools guide](external-tools.md). Compatible
tools are user-supplied; each stage reports itself unavailable when a tool is absent:

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
