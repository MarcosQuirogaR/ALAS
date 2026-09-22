# Troubleshooting

Diagnostic guidance for common issues and solver warnings. Most warnings indicate
optional external tool absences or expected physical limits rather than application
failures.

---

## Startup & Execution

### Windows SmartScreen warning

New release files may still be unfamiliar to SmartScreen while publisher and
file reputation builds. Check the displayed publisher, compare the release
SHA-256 checksum, and submit a suspected false positive through the
[Microsoft Security Intelligence submission portal](https://www.microsoft.com/wdsi/filesubmission).

### The application will not launch on Linux

Ensure the executable has execution permissions (`chmod +x alas`). If running
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

---

## Stage Results & Solver Diagnostics

### An optional results tab reports "not available"

When an optional stage cannot execute, ALAS degrades gracefully. The stage records
a diagnostic status (`NotRun` or `Error`) and the rest of the pipeline completes:

| Stage | Common cause |
|---|---|
| **MSES** | Solver not installed in path, or solve did not achieve numerical convergence |
| **Structures (vibration)** | MSC Nastran / NASTRAN-95 executable not configured |
| **Airway routing** | Airway navigation data files not yet downloaded (`alas --download-navdata`) |

### MSES reports non-converged points

MSES is a viscous-inviscid coupled transonic solver. Non-convergence is an expected
outcome when examining sections near boundary-layer separation, buffet onset, or
under severe shock-induced adverse pressure gradients.

In ALAS, non-converged points are recorded with diagnostic status codes and
distinguished from converged data. To improve convergence on steep polars:
- Check that section geometry does not contain sharp irregularities or zero-thickness trailing edges.
- Narrow the angle-of-attack sweep in **Advanced Settings → MSES**.
- Review solver transcript logs retained in the run directory.

### Structural Nastran solver fails to run

MSC Nastran requires an independent licensed installation. Check that the solver
path is correctly configured under **Setup → External Tools** or provided in the
configuration YAML. Analytical beam and wingbox sizing calculations run natively
without Nastran.

---

## Optimization

### Re-running the same case produces slightly different geometry

Optimization algorithms use stochastic population initialization. If the random
seed is unset, runs will differ. Fix the seed via `--seed <INT>` or in
**Advanced Settings → Optimizer** for bitwise reproducible runs.

### Optimization L/D differs from final report polar

The optimization loop evaluates candidates with a high-throughput aerodynamic model
to rapidly assess thousands of geometries. The winning airframe is subsequently
re-analyzed in full post-analysis stages. Always cite values from the final report.

---

## Further Assistance

For bug reports, solver crashes, or parity discrepancies, open an issue on
[GitHub](https://github.com/MarcosQuirogaR/ALAS/issues) including the console
diagnostic output and configuration YAML.
