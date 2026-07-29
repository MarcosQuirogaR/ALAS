# Troubleshooting

Problems are grouped by when you hit them. Most of what looks like a
failure here is an optional component reporting that it is absent, which is
different from something being broken.

## Starting up

### Windows blocks the download or the first launch

SmartScreen warns about executables it has not seen before, and this build
is not code-signed. Choose **More info**, then **Run anyway**. If your
browser blocked the download itself, use its downloads list to keep the
file.

### The window takes a long time to appear on first run

The first launch unpacks the bundled analysis stack, which is several
hundred megabytes and a few thousand files. Later launches reuse what was
unpacked and start much faster. If your antivirus scans on access, the
first run is slower still.

### It starts, but the interface never loads

The application is a thin shell around a local analysis service, and the
shell is waiting for that service to answer. Give it a minute on a slow or
heavily-loaded machine. If it never connects, the usual cause is security
software blocking a loopback connection: the service listens only on your
own machine and never opens a network port to the outside.

## While setting up a design

### The passenger count will not let me edit it

Passenger count is derived from the cabin preset. Set the preset to
**Custom** on the Inputs page and the field becomes editable. See
[Cabin & payload](cabin-and-payload.md).

### A preset gives a different passenger count than I asked for

The layout engine places whole rows into the real fuselage shape and stops
when it runs out of cabin. It will not invent a partial row to reach a
round number, so a request for 350 seats can come back as 349.

### The baseline pass says the design does not balance

This is the check doing its job. Something in the inputs is inconsistent:
commonly a take-off weight the mass build-up cannot close against, a
payload too large for the fuselage, or a wing position that puts the centre
of gravity outside the envelope. Read the weight and balance tab and fix
the input rather than moving on to a full run.

## During a run

### A candidate is rejected as infeasible

Expected, and frequent. The optimizer explores the whole bounded space,
including combinations that violate a constraint, and penalises them. Worry
only if convergence stalls completely or every candidate in a generation
fails, which usually means the constraints have left no feasible region;
loosen whichever one is binding.

### The run finishes but a results tab says "not available"

An optional stage did not run. The tab explains which and why. The rest of
the run is unaffected. Common cases:

| Tab | Usual cause |
|---|---|
| Mission & Route | Mission environment not set up; see [Installation](installation.md#mission-simulation-setup) |
| MSES | The solver did not converge on this geometry |
| Structures (vibration) | No Nastran licence; this check has no analytical fallback |
| Structures (Patran renders) | No Patran licence |

### Mission analysis reports "not configured"

The isolated mission environment has not been provisioned. **Setup →
External Tools** shows the SUAVE row and how to set it up. It is a one-time
step of a couple of minutes.

### The transonic solve produces nothing

MSES ships with the application, so absence is rarely the reason. Far more
often the solver did not converge, which is a normal outcome on some
sections rather than a crash. If it reports the target lift coefficient
outside its converged range, widen the sweep half-width on **Advanced
Settings → MSES Analysis**.

Very thin or otherwise degenerate sections (the kind an optimizer produces
when a bound is set too wide) can also fail to mesh.

### Nastran fails to launch

Nastran is not installed, not licensed, or not on a path the application
can start it from. Check that it runs from a plain terminal before looking
at the ALAS side. Every structural number except the vibration check
comes from the built-in solver and does not need a licence at all: see
[Structural analysis](structural-analysis.md).

### A long run appears to hang

Optimisation with a large population and iteration count is genuinely slow,
and progress between generations can be quiet. The log is the thing to
watch. Airfoil screening is the one long operation with a cancel button,
because interrupting it is often the right move once you realise a filter
was wrong.

## Results and output

### Two runs of the same configuration give different answers

Check the optimizer seed. Left unset, each run starts from a different
random population, and a search over this many constrained variables can
settle on a different local optimum. Fix the seed on **Advanced Settings →
Optimizer & weights** to make runs comparable.

### The optimizer's lift-to-drag does not match the final report

Correct, and expected. The search uses a deliberately cheap aerodynamic
model, and the winning design is then re-analysed at full fidelity. The two
numbers come from different calculations. Quote the second.

### Figures are hard to read when pasted into a document

Switch the theme before exporting. Figures follow the application theme, so
the light theme gives you charts that sit properly on a white page. **View
→ Light theme**, then export.

### Where did my output go?

Alongside the figures, every run writes a design database, the winning
section coordinates and, if a mission ran, the flight data. The Results
view exports a PDF report and a figure archive on demand. See
[Reporting & export](reporting-and-export.md).

## Running from source

### A build under OneDrive fails with a locked directory

OneDrive holds locks on freshly written files for longer than the build
tolerates, which matters when a step has just produced several hundred
megabytes. Delete the stale directory the error names and run again, or
move the checkout outside a synced folder.

### Installing pyvista or pyvistaqt fails

Those belong to the optional `gui` group and are only needed for 3D
previews and the textured map. Skip the group if you are working
headlessly.

## Nothing here matches

Open an issue on
[GitHub](https://github.com/MarcosQuirogaR/ALAS/issues) with what
you did, what happened, and the log from the run. The log is the useful
part: it names the stage that failed and usually the reason.
