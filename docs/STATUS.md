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

**Last swept:** 2026-09-05, during the remediation of the 2026-09-03 codebase
audit (`.agent/reports/2026-09-03-codebase-audit.html`, findings F1-F12; the
closure record is the "Audit remediation" section below). Re-verify anything older than a few weeks before
relying on it — this file records what an audit found, not what is
continuously enforced by `cargo xtask gate`.

---

## What runs today

- `cargo run --bin ALAS` launches the desktop GUI (`alas-gui`); `ALAS --gui`
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
   `.agent/reports/2026-08-31-alas-mdo-pipeline-audit.html`: the three-view
   and design-summary figures have disagreed on span (68 m vs 81.11 m) for
   at least one case; not confirmed fixed. The V-n ordering defect is fixed
   2026-09-05: `alas-perf` validates VS < VA <= VC < VD, an invalid order is
   the `InvalidEnvelopeSpeedOrder` pipeline finding, and the report labels
   the envelope invalid instead of drawing it as valid. VC is still
   back-derived as VD/1.25 (a stated conceptual-design assumption).
6. **Mission profile parity, resolved 2026-09-05 as an input mismatch, not
   a numerical regression.** The 9/144 cruise-speed differences came from
   product presets deriving their cruise schedule from Mach and altitude,
   while the frozen Python request carried the baseline explicit TAS
   profile. The parity test now replays that recorded baseline input; the
   golden oracle is unchanged and the request builder agrees at `closed`.
   The Mach-derived preset schedule remains a documented product deviation.
7. **`alas-pipeline`'s previously known test failures are closed
   (2026-09-05).** The tank-limited CG regression now constructs an
   explicitly tank-limited fixture (fuel closure and MTOW raised together).
   The fixed-design finalist test requests a brief the reviewed cabin seats;
   the default 350-passenger brief seats only 349 in the default shell at the
   default design vector, and that shortfall is pinned by its own test as a
   reported finding that the optimizer's own validity flag does not see.
8. **Route-globe fullscreen rendering** was slow (~8.4 FPS / 101.8 ms per
   frame) after a correctness fix removed a cached-raster shortcut
   (`.agent/reports/2026-08-31-route-globe-performance.html`). Resolved
   2026-09-11: the cost was the SVG round-trip of the vector overlay (about
   70 ms of the frame), not the sphere. Orbit views now draw the vector
   elements as egui shapes (`SceneView::vector_overlay`) and rasterize only
   the textured globe, whose rows run in parallel; a camera frame at 1.5x
   density costs about 3 ms (2.7 ms texture, 0.3 ms shapes). Static cards
   and exports keep the SVG raster path unchanged.

## Audit remediation, 2026-09-05

Closure record for `.agent/reports/2026-09-03-codebase-audit.html`. "Closed"
means the mechanism is in place and its tests pass on the working tree at
this date; it is not a claim that any preset is a verified aircraft mission.

- **F1 (gate has no enforcement point): closed.** The single workflow now
  runs `cargo xtask gate` on Windows and a separate Linux `cargo-deny` job;
  the workflow was edited locally and has not yet been executed remotely,
  so its 60-minute budget is unverified. No pre-commit hook was installed;
  `cargo xtask install-hooks` remains the local convenience.
- **F2 (`cargo test --workspace` did not compile): closed** before this
  sweep; the workspace builds and the full suite runs.
- **F3/F4 (dead parity oracle): closed for every parity target.** Historical
  fixtures are replayed as inputs; every independently sourced correction is
  pinned two-sidedly (frozen Python value and corrected Rust value) instead
  of being absorbed by a tolerance. Config presets and settings pin the
  typed engine ratings (A320 120.1 kN, A380 338.7 kN, A220 97.72 kN, DC-10
  224.2 kN) and the A320-214/A340-312 dimension corrections; the mission
  vehicle request pins the same corrections; payload layouts replay the
  frozen cabin frame, resolve the historical premium-economy slot in the
  compatibility interior, and keep the loose bulk position of the frozen
  hold grid; W6.5 replays the frozen pre-builder geometry. `PORTING.md`
  rows are flipped only where the corresponding test passes.
- **F5 (`include!` hid module size): closed.** `cargo xtask checks` counts
  assembled production lines through every `include!`; the modules already
  over 500 lines are listed with non-growing ceilings in
  `docs/source-size-budgets.tsv` (see CONTRIBUTING.md).
- **F6 (two thrust representations): closed.** `EngineConfig` has one
  authoritative rating, `thrust_kn()`, derived from the typed payload; the
  serialized legacy key is a derived mirror and legacy-only files migrate
  through the catalogue. Readers that used the stale flat value now see the
  typed rating (A320: 120.1 kN instead of 117.77 kN), which moves the
  thrust-scaled propulsion mass by about 100 kg.
- **F7 (library panic on missing Jet-A): closed;** the differentiable
  mission path follows the rejected-iterate contract.
- **F8 (V-n ordering): closed;** see item 5 above.
- **F9 (NASTRAN-95 provenance and `-O0`): partially closed.** The sibling
  solver records its upstream revision and inventories the local diff;
  pre-existing change dates and authors are unknown and are not invented,
  so the NOSA characterization is incomplete and redistribution stays
  blocked. Selective optimization is opt-in and untested on this host.
- **F10 (untracked status file): obsolete;** this file is tracked. The
  `.agent/reports/` evidence it cites remains unversioned by design.
- **F11 (check backlog, no supply-chain check): closed.** Repository
  checks pass; `cargo deny --locked check` passes on advisories, bans,
  licenses and sources with two named maintenance-notice exceptions
  (`docs/dependency-policy.md`). `alas-screen` gained contract tests and
  rejects malformed sweep bounds before allocation. Units remain a naming
  convention, not a type.
- **F12** was affirmative evidence and needed no change.

Physical findings surfaced while re-pinning the acceptance matrix on the
source-corrected presets (all reported by the product, none suppressed):

- **A220-300:** with its PW1521G-3 binding (it was previously weighed and
  flown as a 467 kN GE9X) and the bulk-only lower hold, the policy takeoff
  CG is 38.8% MAC in the model frame and 35.4% MAC in the Airbus
  recovery-publication frame, inside the published planning envelope at the
  flown takeoff mass; the operating-empty state sits forward of the model's
  configured forward range, the published tanks cannot reach MTOW by about
  700 kg, and the item ledger places the partial fuel load in the wing cells
  where the lumped fuel point cannot. The acceptance test recomputes both
  frames from their primitives and requires them to name the same station.
- **A320-200:** the MTOW closure remainder is 16,882 kg (it was 17,733 kg
  before the dimension, payload and engine corrections); flown at the EASA
  basic-scheme takeoff mass the route lands below the WV017 maximum landing
  mass, and the operating-empty state sits forward of the model's configured
  forward range.
- **Default brief:** the default 350-passenger shell seats 349 at the
  default design vector.

Not verified in this sweep: a rebuilt or re-timed NASTRAN-95, the remote
execution of the CI workflow, and any physical validation of the numbers
above against flight data.

## Mass, fuel and optimization overhaul, 2026-09-06

Delivered as a coherent increment on the working tree; the closure record
for the design intent is `docs/FUEL_MISSION_ROADMAP.md` ("Delivery status")
and the state-of-the-art basis is the three research notes under
`.agent/reports/research-2026-09-05-*.md` (fuel regulations, tank layouts,
mass/CG/inertia methods, MDO drivers).

- **One mass model for every aircraft.** The product analysis places each
  group at a geometry-derived station (`mass_model.geometric_component_
  stations`, default on): the integrated wingbox centroid, the tails at
  42 percent of their mean chord, the gear at the configured nose and main
  stations, the engines at their nacelles, the fuel in its tanks. The
  frozen point placement stays selectable and is what the parity fixtures
  replay. The item ledger behind those points carries a centroidal inertia
  tensor per item, so every named state (operating empty, zero fuel, flown
  takeoff, flown landing, maximum fuel) has a full tensor about its own
  centre of gravity; the dynamic-mode figure reads it instead of the
  radius-of-gyration guess. The radius-of-gyration reference was itself
  corrected to Raymer's half-dimension definition, checked against the
  measured B747-100 tensor (NASA CR-2144) to one percent.
- **Fuel is a tank state, not a remainder.** Every preset declares its
  tanks with the manufacturer's per-tank usable volumes (A320 three-tank,
  A220, A340 with trim tank, A380 with inner/mid/outer/trim, B787-9, DC-10-30
  with auxiliary tank, ATR 72 wing tanks, the notional AVE); a redesigned
  wing keeps a consistent capacity through a retained calibration factor.
  Unusable fuel belongs to the operating empty mass. The design database
  exports every tank, state and ledger row.
- **Regulation-based fuel allocation.** The fuel policy decomposes the load
  into taxi, trip, contingency, alternate, final reserve, additional and
  extra fuel under the EASA basic scheme (default), 14 CFR 121.639 or
  121.645, or a named study convention, each quantity tagged with the rule
  that produced it. The mission stage closes the takeoff mass against that
  requirement, re-flying the native mission until it settles, and reports a
  shortfall when the takeoff-mass limit or the tanks cannot carry it.
  Selecting a scheme is a study assumption; nothing here is a compliance
  finding.
- **Optimization driven by the mission.** A mission-sized objective (block
  fuel, takeoff mass, empty mass, fuel per seat-kilometre) with an inner
  takeoff-mass closure and typed hard/soft/diagnostic requirement families
  (mass and fuel, balance, CS-25.121 and field performance, geometry) is
  selectable in `optimizer.objective`. (Superseded later the same day: the
  legacy lift-to-drag objective is no longer a product objective; see
  "Mission-driven objective only" below.)
- **What moved.** The interactive routes are flown at the policy takeoff
  mass rather than at MTOW: the A320-200 (LEMD-LEPA) lands below its maximum
  landing mass, the A220-300 sits inside its published planning envelope,
  and the acceptance matrix pins the new load cases. Geometry-derived
  stations move the operating-empty centre of gravity forward by 3.0 to 4.2
  percent MAC on the probed presets (A320 15.7 to 12.1, A220 21.9 to 17.7,
  AVE 15.5 to 12.5) because podded engines now sit at their nacelle
  mid-length ahead of the wing; at the same fuel the takeoff centre of
  gravity moves forward on the A320 and A220 and aft on the AVE, whose tank
  centroid lies aft of the frozen fuel point. Both the frozen and the
  geometric operating-empty positions sit well forward of published
  dry-operating positions for the A320 and A220 (about 25 to 30 percent
  MAC), so the bias is in the preset wing placement or the group masses
  rather than in the station method; a per-preset weight-and-balance
  comparison is the next increment.
- **Not verified.** No physical validation against flight data or an
  operational flight plan; the holding fuel flow is an analytic estimate
  (cruise TSFC at minimum-drag speed) rather than an engine-deck value; tank
  spanwise boundaries other than the A320's are volume-consistent estimates.

## FLOPS airframe mass and gradient-based MDO driver, 2026-09-06

Delivered on the working tree; the methods are described in
`docs/methods.md` ("FLOPS transport mass method" and "Multidisciplinary
sizing loop and gradient-based driver").

- **Complete FLOPS transport method.** `mass_model` gains
  `structural_mass_method` and `propulsion_mass_method` beside the existing
  `systems_mass_method`, each with a `flops_transport_v1` option, and a
  `flops_structure` group carrying the FLOPS technology factors and
  overrides. The structural group (wing with simplified or detailed bending
  factor, tails, fuselage, gear, nacelles, paint) and the propulsion group
  (scaled engine, distributed-propulsion scaling, reversers, controls,
  starters, fuel system) are evaluated from the built geometry and the
  declared architecture; a missing datum blocks the method with a typed
  reason. Defaults are unchanged: every preset still uses the frozen
  methods, and the parity fixtures are untouched.
- **Converged sizing loop.** The mission-sized candidate is closed as a
  multidisciplinary analysis (mass, CG, trim, polar, fuel, takeoff mass)
  with Aitken acceleration and re-trimming on CG shift
  (`objective.retrim_cg_tolerance_pct_mac`, default 0.1 percent MAC), so
  the design is trimmed at its own weight. `FixedRequirement` sizing keeps
  its single pass.
- **`sqp` optimizer method.** A native line-search SQP driver (l1 merit,
  damped BFGS, elastic dense interior-point QP, parallel forward
  differences) over the converged loop, with the hard requirement residuals
  as explicit inequality constraints and the termination reason reported.
  Two solver settings were added (`finite_difference_step`,
  `constraint_tolerance`). The driver is verified on analytic constrained
  problems and on a bound-constrained delegated objective; on the native
  objective it is exercised for one major iteration in the test suite.
- **Verification status.** Every FLOPS group reproduces the two FLOPS-run
  validation cases NASA Aviary distributes (simple and detailed wing) to the
  data file's quoted precision (`crates/alas-mass/tests/
  flops_validation_cases.rs`, data in `.agent/reports/
  flops-aviary-validation-data.md`). The SQP driver is verified on analytic
  constrained problems and a bound-constrained delegated objective, and
  exercised for one major iteration on the native mission-sized objective.
  No physical validation against weighed aircraft, and no optimization
  study has been run to convergence on a preset yet.

## Mission-driven objective only, 2026-09-06

- **The lift-to-drag objective is gone from the product.** `ObjectiveKind`
  no longer has a legacy variant; the default objective is block fuel with
  the takeoff mass sized by the mission (`mtow_sizing = sized_by_mission`,
  `requirements.mtow_kg` is the ceiling). The frozen weighted lift-to-drag
  cost survives only inside `DesignObjective::new_reference_compatibility`,
  which the parity fixtures replay; a saved configuration naming
  `legacy_lift_to_drag` is rejected.
- **The `enforce_physical_constraints` switch is removed.** It only relaxed
  the frozen objective's penalties; requirement families are now governed by
  their `optimizer.objective` policies.
- **The AVL optimization branch is mission-sized too.** AVL supplies the
  induced drag at the required cruise lift; the parasite build-up, trim
  incidence and neutral point stay the native report's, and the sizing loop
  closes mass, fuel and takeoff mass around that fixed polar
  (`assess_candidate_with_polar`). NSGA-II's first objective and the
  convergence figure follow the mission objective.
- **What is inert now.** Of `optimizer.weights`, the product search reads
  only `failure_cost` and the tail-volume window; the rest is replay-only
  and sits in a collapsed "Reference-replay penalty weights" section of the
  optimizer page. `docs/OPTIMIZATION.md` lists how a run is driven and every
  input the mission-sized search reads.

## External-solver integration state

- **MSC Nastran** fails to launch in the audited environment: missing VC80
  C runtime (`0xC0000135`). Environment issue, not an ALAS defect. Patran is
  blocked on the same runtime and is otherwise unvalidated.
- **NASTRAN-95 vs MSC Nastran modal parity** disagrees by up to 61.6% on
  mode 1 and up to 9.4% (mean 2.5-4.4%) on modes 2-30. Unresolved; flagged
  for a dedicated correlation study, not treated as passing.
- **NASTRAN-95 is 49-82x slower than MSC Nastran** even after the
  DBMEM/SYSTEM(58)/NE tuning that landed 2026-08-31. That ratio was
  measured against a NASTRAN-95 executable compiled at global `-O0` (the
  workaround for a miscompiled `mis/ifp1c.f` at `-O3`), so it compares an
  unoptimised build with a production solver. 2026-09-11: the selective
  `-O3` option was built and fails the production SOL 101 deck (fatal 321);
  several unrelated legacy routines miscompile. A kernel-whitelist build
  (`-O2` with loop and aliasing guards on the decomposition, substitution,
  multiply, transpose and eigensolver families only, everything else `-O0`)
  passes both production decks with F06 output identical to the `-O0`
  baseline apart from time stamps: SOL 101 13.6 s against 23.6 s, SOL 103
  270 s against 440 s on this host. The bundle still ships the `-O0`
  executable; see `docs/NASTRAN95-BUNDLE.md` for the build options and the
  remaining adoption requirements.
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
- The mission model has no wind model and no CAS/Mach or optimum-altitude
  guidance (P3 in `docs/FUEL_MISSION_ROADMAP.md`). The maximum-available-fuel
  load case is retained only behind `fuel_policy.fly_policy_load_case =
  false`; the product flies the fuel-policy closure described below.

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
