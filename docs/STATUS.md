# Project status

This is the answer to "does ALAS work today", as opposed to `docs/PORTING.md`,
which answers a narrower question: "has this module been checked against the
Python reference to a stated tolerance". The two used to be the same question.
They no longer are — most of `alas-pipeline`, `alas-app` and `alas-gui` were
written as native Rust orchestration over already-parity-tested physics
kernels rather than translated line-by-line from a Python counterpart, so
their `PORTING.md` rows read `todo` while the crates themselves build, run,
and are exercised end to end below. Read `PORTING.md` for numerical parity and
licence provenance; read this file for whether the program runs and what is
wrong with it.

This is a working document, like `PORTING.md` claims to be for itself: update
it in the commit that fixes or introduces the thing it describes, rather than
letting it drift and doing a retroactive sweep later.

**Last swept:** 2026-09-03, against `.agent/reports/2026-09-01-*` and the
working tree at that date. Re-verify anything older than a few weeks before
relying on it — this file records what an audit found, not what is
continuously enforced by `cargo xtask gate`.

---

## What runs today

- `cargo run --bin alas` launches the desktop GUI (`alas-gui`); `alas --gui`
  is equivalent, and headless flags (`--config`, `--save-config`) drive the
  same pipeline without it. **`docs/RUNNING.md`'s "not yet possible" is
  stale — fix pending, see below.**
- The full design pipeline runs end to end: geometry build, mass/CG, mission
  flight, drag build-up, wingbox sizing, and figure generation, for
  hand-built and CPACS-imported aircraft alike.
- External-tool adapters exist and drive real runs when the tool is
  installed: AVL, VSPAERO, OpenVSP, MSES, NASTRAN-95. MSC Nastran and Patran
  adapters exist but fail to launch in the audited environment (below).
- Eight reference presets (A220-300, A320-200, A340-300, A380-800, ATR72-600,
  AVE, B787-9, DC-10) build and produce output artifacts end to end.

## What does not work yet, or is not known good

Ranked by what would surprise someone shipping a design from this program.
Each item names the report it was last confirmed in; a later date supersedes
an earlier one for the same claim.

1. **No preset has a verified design mission.** The 2026-09-01 independent
   acceptance run found 0/8 presets pass end-to-end acceptance; 5/8 fail
   basic physical screening (CG, ZFW, or passenger capacity out of bounds).
   The project's own "no exceptions" acceptance bar is not met.
   (`.agent/reports/2026-09-01-independent-external-acceptance-interim.html`,
   data under `.agent/external_preset_native_audit_20260901/`.)
2. **Propulsion is internally inconsistent.** The cycle model
   (`alas-prop::cycle`) over-subtracts ram drag and has a choked-nozzle
   energy inconsistency; the mission-flown model
   (`EmpiricalTurbofanDeck`) is a separate, disconnected model from the one
   report figures use, and always resolves throttle against the max-climb
   rating regardless of flight phase. GUI engine-config edits do not change
   flown fuel/thrust, and are overwritten by `AircraftBuilder::new` during
   geometry rebuild.
   (`.agent/reports/2026-08-30-propulsion-model-audit.html`,
   `.agent/reports/2026-09-01-independent-propulsion-verification.html`;
   not confirmed fixed since.)
3. **CPACS round-trip is split-brained.** Re-importing a CPACS export only
   replaces geometry; polar, trim, mass/CG and feasibility are left stale
   from before the round-trip. P0 in
   `.agent/reports/2026-08-31-alas-mdo-pipeline-audit.html`.
4. **Wingbox margin diagnostics were misleading; now fixed.** The strength
   gate's failure message rounded the controlling margin to six decimals,
   so any shortfall between roughly `-5e-7` and `0` displayed as the
   ambiguous `-0.000000`, indistinguishable from float noise at the root
   boundary. Fixed 2026-09-03: `WingboxSizing::controlling_margin` now
   reports the raw value in scientific notation plus the controlling
   spar/station. This is a diagnostics fix, not a tolerance policy — no
   numerical band has been calibrated, so a genuinely small negative margin
   still fails the gate exactly as before, now legibly.
   (`.agent/reports/2026-09-01-wingbox-sizing-error-prevention.html`.)
5. **Known figure/geometry inconsistencies**, per
   `.agent/reports/2026-08-31-alas-mdo-pipeline-audit.html`: the V-n diagram
   can display VA above VC (should never happen); the three-view and
   design-summary figures have disagreed on span (68 m vs 81.11 m) for at
   least one case. Release-blocking per that audit; not confirmed fixed.
6. **Mission profile parity regression:** 9/144 reference cruise-speed
   values differ from the golden fixture by 2.1-11.1%, carried as an
   "existing baseline issue" as of 2026-09-01 rather than resolved.
7. **`alas-pipeline`'s test suite has 4 known failures**: an airport-elevation
   panic, a tank-limited CG assertion, and two optimizer-finalist feasibility
   assertions, as of 2026-09-01.
8. **Route-globe fullscreen rendering is slow** (~8.4 FPS / 101.8 ms per
   frame) after a correctness fix removed a cached-raster shortcut. A real
   interactive-performance regression, not yet addressed.
   (`.agent/reports/2026-08-31-route-globe-performance.html`.)

## External-solver integration state

- **MSC Nastran** fails to launch in the audited environment: missing VC80
  C runtime (`0xC0000135`). Environment issue, not an ALAS defect. Patran is
  blocked on the same runtime and is otherwise unvalidated.
- **NASTRAN-95 vs MSC Nastran modal parity** disagrees by up to 61.6% on
  mode 1 and up to 9.4% (mean 2.5-4.4%) on modes 2-30. Unresolved; flagged
  for a dedicated correlation study, not treated as passing.
- **NASTRAN-95 is 49-82x slower than MSC Nastran** even after the
  DBMEM/SYSTEM(58)/NE tuning that landed 2026-08-31.
- **FLOWUnsteady** has no configured executable in any audited environment;
  every request returns `RequestRejected`.
- **VSPAERO / AVL**: two real input-contract bugs were found and fixed
  2026-09-01 (a 300s timeout was masking a false "unavailable" result at
  900s; the AVL spanwise-vortex floor was under-specified for fine meshes).
  VSPAERO wake convergence still fails its own convergence gate on at least
  one case.
- **MSES** shows genuine partial numerical non-convergence (3 of 7 angles
  in the audited sweep) — a solver behavior, not a code defect.
- **Navdata** is pulled from an unpinned GitHub branch: not reproducible for
  a release build.

## Known-good, by design (not defects)

These are documented, intentional gaps or faithfully-reproduced upstream
quirks, not regressions. See `docs/PORTING.md` and `CONTRIBUTING.md`'s
"Deliberate deviations" for the mechanism:

- `alas-stab::modes` has an acknowledged factor-of-two phugoid-root error
  vs. AVL, deferred to P14 (deviation-candidate).
- The differentiable-fit atmosphere disagrees with ISA by up to 1.1% T /
  0.4% rho / 0.6% sound speed — reproduced faithfully from upstream, not a
  defect.
- The VORLAX kernel is intentionally `f32` (deviation-candidate for a
  future `f64` pass).
- The turboprop deck is an unquantified/extrapolated surrogate; no public
  568F map exists to calibrate it against.
- The mission model burns maximum available fuel rather than solving for
  required fuel, and has no taxi/contingency/alternate/reserve fuel policy,
  no wind model, and no CAS/Mach or optimum-altitude guidance. This is the
  P0-P5 backlog in `docs/FUEL_MISSION_ROADMAP.md`, not a regression.

## Elsewhere

- `docs/PORTING.md` — numerical parity against the Python reference and
  licence provenance, module by module. Still the enforcement point for
  `cargo xtask gate`'s "every crate has a row" check, and still the process
  CONTRIBUTING.md's parity rule requires for new translated physics.
- `docs/FUEL_MISSION_ROADMAP.md` — the mission/fuel model rebuild plan.
- `docs/C0_GUI_ACCEPTANCE_MATRIX.md` — GUI acceptance scenarios, separate
  from and stricter than a passing Rust or SVG test.
- `.agent/reports/` — the underlying investigation reports this file
  summarizes. Not version-controlled long-term evidence; treat as an
  audit trail, and re-run an investigation rather than trusting an old one
  past its relevance.
