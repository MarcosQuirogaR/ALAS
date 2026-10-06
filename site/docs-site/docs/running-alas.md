# Running ALAS

ALAS runs from the desktop application or headlessly from the command line.
Both call the same pipeline (`alas-pipeline`) and give the same results.

---

## The desktop application

Running the executable with no arguments opens the interface:

```powershell
ALAS.exe                          # installed or compiled binary
cargo run --release --bin ALAS    # from the repository
```

The workflow is **Inputs**, **Design Space**, optional Modeling and Advanced
Settings edits, **Run**, then **Results**. See the [User guide](user-guide.md)
for every page. Points specific to running:

- Form fields come from the configuration schema (`alas-config`) and carry units,
  bounds and tooltips.
- Choosing a preset replaces geometry, propulsion, cabin layout and design bounds together.
- The live preview redraws the three-view geometry and the cabin as you edit.
- A stage that cannot run (optional tool missing, no convergence) shows a
  diagnostic status and the run continues.
- **Analyze reference** evaluates the selected aircraft as given, without optimization.
- Sandbox mode is a separate workspace for drawing an aircraft; see [Sandbox mode](sandbox.md).

---

## The headless command line

The command-line interface enables automated batch runs, parametric sweeps, and
scripted execution without opening a graphical window.

### Common execution patterns

```powershell
# Full optimization, baseline comparison, mission analysis, and figure generation
ALAS.exe -c configs/ave.yaml --output outputs --plots

# Analyze baseline design directly (skip optimization search)
ALAS.exe --no-optimize --plots

# Headless run with fixed seed and isolated output directory
ALAS.exe --seed 42 --output isolated-dir --plots

# Skip mission simulation stage
ALAS.exe --no-mission --plots

# CPACS 3.5 input evaluation
ALAS.exe --cpacs-input path/to/aircraft.xml --no-optimize --no-baseline --plots

# Write full effective configuration to YAML and exit
ALAS.exe --save-config effective_config.yaml

# Download missing airway navigation data files
ALAS.exe --download-navdata
```

If developing within the Rust repository:

```powershell
cargo run --release --bin ALAS -- --seed 42 --output isolated-dir --plots
```

---

## Command line options

Flags supported by the current release:

| Flag | Argument | Description |
|---|---|---|
| `-c, --config` | `<PATH>` | Load configuration overlay from YAML or JSON |
| `--cpacs-input` | `<PATH>` | Use a CPACS 3.5 aircraft document as the geometry input |
| `-o, --output` | `<PATH>` | Output directory for reports and figures (default: `outputs`) |
| `--gui` | None | Force launch of the interactive desktop interface |
| `--no-optimize` | None | Skip design space optimization (evaluate baseline design only) |
| `--no-baseline` | None | Skip baseline high-fidelity polar analysis pass |
| `--no-mission` | None | Disable native mission simulation stage |
| `--no-parallel` | None | Run independent pipeline stages sequentially |
| `--plots` | None | Generate and save SVG/PNG figures into the output directory |
| `--show` | None | Display figures interactively (implies `--plots`) |
| `--seed` | `<INT>` | Specify integer random seed for reproducible optimization |
| `--aero-solver` | `vlm`, `avl`, `both` | Select aerodynamic result family |
| `--optimization-solver` | `vlm`, `avl`, `both` | Select optimizer aerodynamic backend |
| `--quiet` | None | Suppress verbose terminal logging |
| `--save-config` | `<PATH>` | Write effective configuration schema to YAML and exit |
| `--download-navdata` | None | Download missing navigation data files and exit |
| `-h, --help` | None | Display command help message |

---

## Output artifacts

A headless run with `--plots` generates:

1. **Terminal summary**: High-level design point metrics (cruise $\alpha$, $C_L$, $C_D$, $L/D$, MTOW breakdown, feasibility status).
2. **Diagnostic stage summaries**:
   - MSES 2D polar convergence reporting (explicit count of converged vs non-converged angles of attack).
   - Structural sizing summary (rib count, spar cap areas, margins of safety).
   - CPACS export metadata (when enabled).
3. **Figures**: Vector SVG and raster charts saved to `<OUTPUT>/plots/`.
