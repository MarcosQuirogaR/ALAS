# Installation & Setup

ALAS is a single native Rust executable. It needs no Python runtime, package
manager or background service.

## Download

The latest published release is **v1.3.1**. The same two packages
are on [alas.uvigo.es](https://alas.uvigo.es/) (`/downloads/<file>`) and on the
[GitHub release](https://github.com/MarcosQuirogaR/ALAS/releases/tag/v1.3.1),
which is the canonical copy.

| Platform | File | Notes |
|---|---|---|
| Windows 10 or later, x86-64 | `alas-v1.3.1-windows-x86_64.zip` | Includes AVL 3.52 |
| Linux x86-64 | `alas-v1.3.1-linux-x86_64.tar.gz` | glibc 2.35 or newer |

Each archive has a sibling `.sha256` file (for example
`alas-v1.3.1-windows-x86_64.zip.sha256`) and contains `RELEASE-MANIFEST.json`
and `SOURCE-MANIFEST.json`, which record the source commit and every packaged
file. Verify the checksum before extracting.

### Windows

1. Download the zip and its `.sha256` file.
2. Check the hash in PowerShell and compare it with the value in the `.sha256`
   file:

   ```powershell
   Get-FileHash .\alas-v1.3.1-windows-x86_64.zip -Algorithm SHA256
   ```

3. Extract the archive, keep the folder intact, and run `ALAS.exe`.

Windows SmartScreen may warn about a new file with little download history.
Compare the checksum before running it.

### Linux

```bash
sha256sum -c alas-v1.3.1-linux-x86_64.tar.gz.sha256
tar xzf alas-v1.3.1-linux-x86_64.tar.gz
cd alas-v1.3.1-linux-x86_64
./ALAS
```

The desktop application needs the graphics stack a normal X11 or Wayland
session already provides (an OpenGL or Vulkan driver, fontconfig). ALAS also
runs headless from the command line with no display; see
[Running ALAS](running-alas.md). The package does not bundle AVL on Linux (the
GPL source and a build note are included); the AVL cross-check is absent until
you configure an executable.

### What is in the package

`ALAS` or `ALAS.exe`, the release and source manifests with the matching AGPL
source snapshot, `configs/ave.yaml` (a generated configuration template),
`README.md`, `LICENSE`, `NOTICE`, `THIRD-PARTY-NOTICES.md`,
`THIRD-PARTY-CRATES.md`, `THIRD-PARTY-LICENSES.txt`, `external tools/` (AVL on
Windows; NASTRAN-95 only when the release staged it, see its entry in
`RELEASE-MANIFEST.json`) and `assets/mses/` (XFOIL transition data for an
installed MSES). MSES, MSC Nastran, MSC Patran, OpenVSP/VSPAERO and FLOWUnsteady
are never included. See [Licensing](licensing.md).

### First start

The application opens on **Inputs** with the AVE preset loaded. Twelve presets
ship: AVE, A340-300, A380-800, B787-9, A320-200, A220-300, ATR72-600, DC-10,
E195-E2, C919, B747-400 and A400M. Press **Analyze reference** in the control
bar for a quick baseline analysis, then see the [User guide](user-guide.md). To
draw an aircraft that is not in the list, use [Sandbox mode](sandbox.md).

!!! note "Source builds are newer than the release"
    This documentation describes the current source. The latest published
    package is v1.3.1; features added since then are marked in the pages that
    describe them.

---

## What runs without external tools

- Parametric airframe sizing and design-space definition
- L-SHADE optimization with epsilon constraints
- Vortex-lattice aerodynamics and the drag build-up
- Turbofan cycle model
- Wingbox sizing, rib spacing and the analytical beam solution
- Mass, CG envelope and longitudinal stability
- Mission simulation (climb, cruise, descent, reserves)
- Cabin layout and payload loading
- Sandbox mode and the fixed-wing UAV workflow

## Optional external tools

All are user-supplied except AVL (included in the Windows package) and NASTRAN-95 (included only if the release staged it). Each stage
reports itself unavailable when its tool is absent. Setup, licences and status
codes are in the [External tools guide](external-tools.md).

| Tool | Used for |
|---|---|
| MSES | Transonic section analysis |
| AVL | Vortex-lattice cross-check |
| OpenVSP / VSPAERO | CAD export and panel-method cross-check |
| MSC Nastran, NASTRAN-95 | Finite-element statics and modes |
| MSC Patran | Deformation images |
| OpenFOAM, Gmsh, ParaView | Airfoil CFD window |
| FLOWUnsteady | Unsteady adapter |

Paths are set under the **External Tools** menu or in the configuration file.

---

## Navigation & route data <a id="optional-route-data"></a>

Missions fly a great-circle track unless route data is available. Airway
navigation data (X-Plane community mirror, third-party GPLv3) downloads on
demand from the **External Tools** window after you consent, or from the CLI:

```powershell
ALAS --download-navdata
```

---

## Building from source (developers)

From a source checkout (Rust 1.85 or newer; on Windows also the Visual Studio
Build Tools with the **Desktop development with C++** workload):

```powershell
# Launch interactive desktop interface
cargo run --release --bin ALAS

# Headless run
cargo run --release --bin ALAS -- --seed 42 --output outputs --plots
```

The binary is `target/release/ALAS.exe` (`ALAS` on Linux).
