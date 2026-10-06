# Troubleshooting

Common issues and solver warnings. Most warnings mean an optional external tool
is absent or a physical limit was reached, not that the application failed.

---

## Startup & Execution

### Windows SmartScreen warning

SmartScreen may warn about a new file with little download history. Compare the
release SHA-256 checksum, and report a suspected false positive through the
[Microsoft Security Intelligence submission portal](https://www.microsoft.com/wdsi/filesubmission).

### The application will not launch on Linux

Ensure the executable has execution permissions (`chmod +x ALAS`). If running
from source via Cargo, verify that Rust and standard system build tools are installed.

---

## Design Configuration & Sizing

### The passenger count cannot be edited

The passenger count is derived from the active cabin layout. To set an arbitrary
seat count, set the **Cabin preset** to `Custom` on the Inputs page.

### Layout passenger count differs slightly from target

The cabin generator places integer seating rows within the real parametric fuselage
cross-section. It does not invent fractional rows; if cabin volume is constrained,
the actual seat count may round down to the nearest feasible integer.

### The baseline evaluation reports center of gravity outside envelope

This indicates an unclosed configuration balance: MTOW may be insufficient for the
requested payload, or the wing root attachment position places the mean aerodynamic
chord too far forward or aft relative to the payload and fuel distribution.
Adjust wing position or fuselage stretch before launching optimization.

### My geometry edit was reverted ("Preset geometry is protected")

In the guided workspace a registered preset keeps its defining geometry (wing,
empennage, fuselage, wetted-area factors, engine installation) and its initial
design vector. Edits to those fields, from the form, a loaded file or a hidden
path, are restored and the log says so. To change geometry, open
[Sandbox mode](sandbox.md); to keep the result as a baseline, promote it when
you leave.

### The Run button is disabled with a marker beside it

The configuration has a validation error, and the marker's hover text lists
the reasons (an out-of-range input, an inconsistent cabin, and so on). Fix the
highlighted field and the button re-enables.

### The optimizer reports no feasible design

Every constraint is hard, so a run whose search finds no candidate that
satisfies all of them ends with `NoFeasibleDesign` and the least-violating
candidate as diagnostics. Common causes are a hard MTOW the closed design
cannot meet, takeoff fuel capped by the usable tank capacity (the cap stops
the mass budget being met by fuel the tanks cannot hold), a forward-CG limit
at rotation, tip-back against the tail-scrape angle, or minimum nose-gear
load. Open the evaluation history in the Optimization tab, read which
constraint rejects the candidates, and widen the relevant bound or relax the
requirement. The B747-400 and A400M presets can end this way at reporting
fidelity; that is a known model limitation, not a setup error.

### Model CG finding on a registered preset

Some presets (A380-800, ATR72-600, DC-10, B747-400, A400M) report a model-CG
finding at their nominal design. The model-CG check is a preliminary design
model of the loading envelope and ground mechanisms, separate from the
published evidence the preset was built from.

### Sandbox messages

| Message | Meaning |
|---|---|
| *Finish or cancel the current run before entering the sandbox.* | A run is in progress; wait for it or press Cancel |
| *The geometry does not build; the last valid shape is shown.* | The last edit produced geometry the builder rejects; undo it or change another value |
| *... must stay between a and b ...; the edit was not applied.* | The value is outside the field's valid range |
| *The model changed; these estimates are stale.* | You edited after the last Quick Analysis; run it again |
| *Cancel the running sandbox analysis before leaving.* | A sandbox Full Analysis is still running |
| *The sandbox configuration is not valid; fix the highlighted fields first.* | Quick Analysis, Full Analysis or promotion needs a valid configuration |

---

## Stage Results & Solver Diagnostics

### An optional results tab reports "not available"

When an optional stage cannot execute, ALAS degrades gracefully. The stage records
a diagnostic status (`NotRun` or `Error`) and the rest of the pipeline completes:

| Stage | Common cause |
|---|---|
| **MSES** | Solver not installed in path, or solve did not achieve numerical convergence |
| **Structures (vibration)** | MSC Nastran / NASTRAN-95 executable not configured |
| **Airway routing** | Airway navigation data files not yet downloaded (`ALAS --download-navdata`) |

### MSES reports non-converged points

Non-convergence is expected near boundary-layer separation, buffet onset and
strong shocks. ALAS records non-converged points with a status code and keeps
them out of the converged data. To improve convergence:
- Check that section geometry does not contain sharp irregularities or zero-thickness trailing edges.
- Narrow the angle-of-attack sweep in **Advanced Settings → MSES Analysis**.
- Review solver transcript logs retained in the run directory.

### Structural Nastran solver fails to run

MSC Nastran requires an independent licensed installation. Check that the solver
path is correctly configured under the **External Tools** menu (also the **External Tools** tab of **Advanced Settings**) or provided in the
configuration YAML. Analytical beam and wingbox sizing calculations run natively
without Nastran.

---

## Optimization

### Re-running the same case produces slightly different geometry

The search is stochastic. With the seed unset, runs differ. Fix the seed via `--seed <INT>` or in
**Advanced Settings → Optimizer**, and also switch on **Stop on evaluation budgets only**: a
stage that stops on its time limit analyses as many candidates as the machine affords, so a seed
alone does not reproduce a run. A time-limited run is replayed exactly with the replay counts it
recorded (results card and run manifest).

### Optimization L/D differs from final report polar

The loop scores candidates with a coarse vortex-lattice model. The winner is
re-analyzed with the fine model in the post-analysis stages. Quote the final values.

---

## Further Assistance

For bug reports, solver crashes, or parity discrepancies, open an issue on
[GitHub](https://github.com/MarcosQuirogaR/ALAS/issues) including the console
diagnostic output and configuration YAML.
