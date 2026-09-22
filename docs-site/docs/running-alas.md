# Running ALAS

ALAS can be run interactively via the desktop GUI or headlessly from the command
line. Both execute the same underlying multidisciplinary pipeline (`alas-pipeline`),
producing identical analytical results and diagnostic reports.

---

## The desktop application

Running the executable with no arguments launches the graphical interface:

```powershell
# From installed or compiled binary:
alas.exe

# Or from the Rust repository:
cargo run --release --bin alas
```

The desktop window follows the engineering workflow:
**Inputs → Design Space → Advanced Settings → Run → Results**.

Key characteristics of the interface:

- **Metadata-driven controls**: Form inputs are derived directly from the
  underlying configuration schemas (`alas-config`). Every setting includes units,
  bounds, and explanatory tooltips.
- **Reference presets**: Selecting AVE, A380-800, or other presets updates the
  geometry scaffold, propulsion definition, cabin layout, and design bounds in unison.
  (UAS configuration presets are in active development.)
- **Live previews**: Includes debounced 3D geometry rendering and cabin deck arrangement.
- **Discipline-grouped results**: Stages report status dynamically. If an optional external
  solver fails to converge or is uninstalled, its tab displays a diagnostic status
  without aborting the overall run.
- **Baseline evaluation**: "Analyze baseline" evaluates the nominal configuration
  without running numerical optimization.

---

## The headless command line

The command-line interface enables automated batch runs, parametric sweeps, and
scripted execution without opening a graphical window.

### Common execution patterns

```powershell
# Full optimization, baseline comparison, mission analysis, and figure generation
alas.exe -c configs/example_config.yaml --output outputs --plots

# Analyze baseline design directly (skip optimization search)
alas.exe --no-optimize --plots

# Headless run with fixed seed and isolated output directory
alas.exe --seed 42 --output isolated-dir --plots

# Skip mission simulation stage
alas.exe --no-mission --plots

# CPACS 3.5 input evaluation
alas.exe --cpacs-input path/to/aircraft.xml --no-optimize --no-baseline --plots

# Write full effective configuration to YAML and exit
alas.exe --save-config effective_config.yaml

# Download missing airway navigation data files
alas.exe --download-navdata
```

If developing within the Rust repository:

```powershell
cargo run --release --bin alas -- --seed 42 --output isolated-dir --plots
```

---

## Command line options

All flags supported by v1.1.0:

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
| `--optimization-method` | `<METHOD>` | Select algorithm (`differential_evolution`, `feasibility_first_de`, `nsga2`, `turbo_1`, `cma_es`, `sqp`) |
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
