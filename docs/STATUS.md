# Project status

This file answers "what does ALAS do today, and what should a user not rely
on". `docs/PORTING.md` answers a narrower question: whether a module has been
checked against the Python reference to a stated tolerance, and what licence
its content carries. Most of `alas-pipeline`, `alas-gui` and `alas-app` are
native orchestration over parity-tested kernels, so their `PORTING.md` rows
read `todo` while the crates build and run end to end.

Claims here are of three kinds, kept apart: implementation (the code does the
thing), numerical verification (it agrees with a stated reference or invariant),
and physical validation (it agrees with a real aircraft). Only the first two
are established by the test suite. `cargo xtask gate` enforces formatting,
lints, tests and repository checks; it does not enforce the statements below.

Update this file in the change that alters what it describes.

---

## Capability

- `cargo run --bin ALAS` launches the desktop GUI (`alas-gui`); `ALAS --gui` is
  equivalent. Headless flags (`--config`, `--save-config`, `--no-optimize`,
  `--seed`) drive the same pipeline; see
  `docs/RUNNING.md`.
- The design pipeline runs end to end: geometry, mass and CG, mission, drag
  build-up, trim and stability, optimisation, wingbox sizing, feasibility
  findings and figures, for hand-built and CPACS-imported aircraft.
- Eight registered presets: A220-300, A320-200, A340-300, A380-800, ATR72-600,
  AVE (a synthetic ALAS design), B787-9 and DC-10.
- **Optimisation.** One search, `differential_evolution`
  (`docs/OPTIMIZATION.md`, `docs/methods.md`): L-SHADE under the epsilon-constrained method
  (`alas-opt::search_methods::lshade_de`) with a mission-sized objective (block
  fuel by default; takeoff mass, empty mass and fuel per seat-kilometre are the
  alternatives) and takeoff mass closed by the sizing mission. In the product
  profile a reported design satisfied every hard constraint at full coupled
  fidelity; if none is found the run returns `NoFeasibleDesign` with the
  least-violating candidate as diagnostics. Seeded runs replay identically at
  any worker count. A screening stage and a refinement stage each have an
  evaluation budget and a time limit; termination is `converged`,
  `stagnated`, `evaluation_budget`, `time_budget` or `cancelled`. MADS, SQP, NSGA-II, TuRBO, CMA-ES and the weighted
  lift-to-drag `scipy_legacy` profile were removed; saved configurations
  naming those tokens are migrated to `differential_evolution`
  on load.
- **Mass and fuel.** One item-level mass ledger with centroidal inertia tensors
  for every named state (operating empty, zero fuel, takeoff, landing, maximum
  fuel). Fuel is a tank state in burn order; unusable fuel belongs to operating
  empty mass. The fuel policy decomposes the load into taxi, trip, contingency,
  alternate, final reserve, additional and extra fuel under the EASA basic
  scheme (default), 14 CFR 121.639 or 121.645, or a named study convention, and
  the mission stage closes takeoff mass against it. Selecting a scheme is a
  study assumption, not a compliance finding. The FLOPS transport mass method is
  available beside the frozen reference methods (`docs/flops-mass-model.md`).
- **Cabin.** One canonical cabin layout feeds sizing, mass and CG, feasibility,
  reports and the 3-D preview (`docs/research/cabin-layout-sources.md`).
- **Structures.** Wingbox sizing, analytical deflection, and NASTRAN deck
  generation with native OP2 reading. MSC Nastran and NASTRAN-95 are optional
  solvers; the analytical path needs neither.
- **External tools.** Adapters exist for AVL, VSPAERO, OpenVSP, MSES, MSC
  Nastran, NASTRAN-95, Patran, FLOWUnsteady and OpenFOAM with Gmsh (`alas-cfd`).
  Each is optional and reports itself unavailable when absent. Their behaviour
  depends on the installed tool and version, which the repository does not pin.
- **Other workflows.** Airfoil-database screening (`alas-screen`), two-dimensional
  OpenFOAM airfoil studies (`alas-cfd`), and fixed-wing electric UAV component
  selection and feasibility (`alas-uav`).

## Verification and validation status

- Numerical verification: physics kernels translated from the Python
  implementation are held to the tolerance tiers in `alas-testkit` against
  fixtures under `golden/`. The tiers assume the MSVC runtime as the reference;
  on other platforms some tiers are relaxed (`REFERENCE_RUNTIME`).
- Registered-aircraft acceptance: every preset executes. At the 1.2 release,
  AVE (zero-fuel CG forward of its configured range) and ATR72-600 (see below)
  raised model findings in the baseline screening.
- Product-profile optimisation matrix at 1.2 (`balanced` budget, 15 generations): seven presets
  delivered a numerically feasible candidate and none converged, so every
  termination was `iteration_limit`. ATR72-600 returned `NoFeasibleDesign`.
  Presets are optimised clean-sheet, so fuel deltas against the registered
  aircraft are reported as not comparable.
- **No preset has a source-backed design mission.** Every optimisation audit row
  records the mission as `UNVERIFIED`, and the acceptance matrix reports its
  result as incomplete for that reason.
- Real-aircraft parity (`docs/aircraft-parity.md`): 241 comparison rows, 74
  scored, of which most compare registered reference inputs rather than model
  outputs. It is not a certification or flight-test comparison.
- Public planning CG is evaluated only for the A220-300, the one preset with a
  registered public planning envelope.

## Known limitations

Ranked by what would most surprise someone using a result.

1. **No physical validation against flight or weighed-aircraft data** for any
   preset. All numbers are conceptual-design estimates.
2. **ATR72-600 is outside the mass model's validity domain.** The generic FLOPS
   transport fuselage, furnishings and systems correlations (Eqs. 104, 106, 110)
   are fitted to larger jet transports. The modelled OEW (about 15,244 kg
   against 13,450 kg published) and CG are not usable until a sourced
   regional-turboprop mass model replaces those terms. No calibration was
   invented for it.
3. **A380-800 fuel distribution** is an approximate ground distribution over
   aggregate tank groups. It conserves mass and balances left/right pairs but
   does not show fuel in all four physical feed tanks, and it does not take
   zero-fuel mass and CG as the Airbus fuel quantity management system does.
   It is not a certified loading schedule.
4. **Mission assumptions** that are engineering choices rather than sourced
   values: the generic 250 m/s TAS climb schedule used by preset routes, the
   0.9 m bulk-hold handling clearance, and the step-climb interval (derived from
   the modelled burn rate). The holding fuel flow is an analytic estimate at
   minimum-drag speed. The mission model has no wind model and no CAS/Mach or
   optimum-altitude guidance (P3 in `docs/FUEL_MISSION_ROADMAP.md`).
5. **Tank geometry** other than the A320's is volume-consistent estimation from
   the manufacturer's usable volumes; spanwise tank boundaries are not sourced.
6. **Cancellation latency.** Inside the search a cancel costs at most the block
   of candidates in flight. Finalist verification and the reporting stages check
   the flag only between blocks and stages, so a request there can wait tens of
   seconds on the largest presets.
7. **Solver behaviour** (properties of the tools, not of ALAS code):
   - NASTRAN-95 and MSC Nastran disagree on modal results (up to 61.6 % on mode
     1, up to 9.4 % on modes 2-30 in the audited case); this is unresolved and
     not treated as passing. NASTRAN-95 is far slower than MSC Nastran and the
     bundled executable is built at `-O0` because of miscompilation at higher
     optimisation; `docs/NASTRAN95-BUNDLE.md` lists the build options and the
     remaining adoption requirements.
   - MSES shows partial numerical non-convergence on some angles of attack.
   - VSPAERO wake convergence fails its own gate on at least one case.
   - FLOWUnsteady has no configured executable in the audited environments;
     requests are rejected.
   - MSC Nastran and Patran did not launch in the audited environment for lack
     of a required C runtime. That is an environment issue, and Patran is
     otherwise unvalidated.
8. **Deliberate reference-compatible behaviour** (recorded as deviation
   candidates in `docs/PORTING.md`, not defects):
   - `alas-stab::modes` carries a factor-of-two phugoid-root error against AVL.
   - The differentiable-fit atmosphere disagrees with ISA by up to 1.1 % in
     temperature, 0.4 % in density and 0.6 % in speed of sound, as upstream does.
   - The VORLAX influence kernel is `f32` by design.
   - The turboprop deck is an extrapolated surrogate; no public 568F map exists
     to calibrate it.
   - The `new_reference_compatibility` constructors replay the historical
     objective and DE search for the parity fixtures.
9. **Phase-scoped CG limits: checked against two published forward limits
   only, not validated.** Each loading state is gated only by the mechanisms
   of its phase (`PhaseLimits`): rotation and the static-margin floor at
   takeoff, landing trim and ground mechanisms at landing, ground mechanisms
   at OEW. The rotation limit is the nose-wheel lift-off moment balance about
   the main-gear contact (Sadraey 2012, sec. 9.6.2 and 12.6) at VR = 1.10 VS
   (stall branch) with the all-engine thrust term and runway friction (mu
   0.02, an engineering estimate), the ledger pitch inertia transferred to
   the contact (solved exactly, quadratic in the CG), 7 deg/s^2 pitch
   acceleration for every class (Sadraey's Table 9.6 could not be confirmed;
   optional config overrides exist) and a tail lift at full up-elevator
   derived from the tail geometry, downwash in ground effect, a -25 deg class
   elevator limit, the DATCOM plain-flap large-deflection correction (0.60 at
   25 deg; other anchors not digitised from the figure) and a tail-section
   lift cap (does not bind on the presets): CL_h -0.85 to -0.97. Drag is
   omitted (about 1 %MAC or less). Against published limits, on the
   manufacturer's MAC frame:
   - Forward, rotation at takeoff: A320 9.2 vs 17 (ACAP most-forward CG used
     in the pavement-load analysis at MRW, not a certified limit), A220 -3.0
     vs 18.4 at 67,585 kg (ARP Table 3, certified flight and ground limit)
     and A340 28.3 vs 20.3 (ACAP; aft of the published value, a known
     residual, its test is ignored). Before the large-deflection correction
     and the tail cap these were -17.5, -37.9 and 5.6. The A320 and A220
     can still lift the nose wheel forward of the published limits, so
     those are set by criteria the model does not carry (finding F-ROT-1). The takeoff state's
     forward limit is then the maximum nose load on the A320 and A220 and
     rotation elsewhere. A380 (25.0), B787 (-0.8), DC-10 (0.6), AVE (8.8)
     and ATR (10.9) are unanchored. The elevator effectiveness keeps the
     thin-airfoil value times an empirical large-deflection factor, still a
     class-generic assumption (delta_e -25 deg).
   - Aft: the A320 aft limit is the minimum-nose-load ground limit (6 % nose
     load) and reproduces the published 40 %MAC. Tip-back is about 5 points
     too restrictive on the A320 (h_cg over the aft axle). The A220 aft limit
     is not weight-dependent in the model while the published one is (31.0 to
     37.3 %MAC), so it is too tight at mid weights. The aerodynamic aft limit
     never governs.
   - DC-10: its rotation limit (0.6 %MAC) no longer rejects the loaded
     takeoff (16.5 %MAC); the registered design fails the minimum nose load
     at OEW (23.62 against 23.58 %MAC). Its main-gear station is not anchored
     to a source.
   - OEW-CG residual: the A320 model OEW CG sits +7.85 %MAC aft of the ACAP
     nominal 26.5 %, so the A320 load-trim potato leaves the ground limits
     aft. The A320 OperationalReserve state (33.3) and the tank-burn path
     (34.8) also disagree at the same mass.
   - The ATR72-600 fails the ground limits at OEW.
   - OEW residuals against the reference aggregator: ATR +6.1 %, A380 -7.2 %
     (the census probe `alas-pipeline/examples/phase_census.rs` lists all).
   The acceptance tests assert inequalities against the published envelopes
   (A220 ARP; A320 17 to 40 %MAC from ACAP pavement and nose-load values), not
   pins on the model CG.
10. **One takeoff mass per report, one flown mass per dispatch.** Figures,
    summary, structural loads and the V-n envelope read the report's sized
    takeoff mass (the declared design gross weight for the structure and V-n of
    a registered aircraft). The mission dispatch re-flies that aircraft with the
    pipeline's mission model and keeps the sized mass when its fuel re-price
    agrees within max(5 kg, 1e-4 of the takeoff mass); otherwise the flown mass
    is refined separately and may differ by about that tolerance.

    Known limitation, open: the optimizer, the report and the flown mission
    use three fuel models and disagree on the sized takeoff mass. The MDO trip
    fuel is priced on the optimizer's polar; the dispatch flies the native
    mission. The gap reaches +8.6 % of sized-vs-flown mass on long-haul
    aircraft. Single-model unification is pending and its selection will be
    based on correlation with sourced references. The A220 case (26 kg) comes
    from the reserves being priced on the report's untrimmed sweep polar
    against the optimizer's trimmed one.
    `report_load_case_and_mission_share_the_sized_takeoff_mass` is ignored for
    this reason.

    | Preset | Mass gap | MDO trip (kg) | Native trip (kg) |
    |---|---|---|---|
    | A220 | +26 kg | 1,400 | 1,399 |
    | A320 | +0.5 % | 1,936 | 2,199 |
    | A340 | +6.4 % | 41,889 | 54,617 |
    | A380 | -1.5 % | 76,899 | 70,670 |
    | B787 | +5.5 % | 52,892 | 64,230 |
    | DC-10 | +8.6 % | 50,894 | 67,348 |
11. **Mass sizing is bound to the search.** The finalist is replayed at the
    sized takeoff mass; the AVL branch reports at the configured MTOW.

## Related documents

- `docs/ARCHITECTURE.md`: crate map and data flow.
- `docs/PORTING.md`: numerical parity and licence provenance, module by module.
  `cargo xtask gate` requires every crate to be mentioned there.
- `docs/methods.md`: the models. `docs/OPTIMIZATION.md`: how a run is driven.
- `docs/FUEL_MISSION_ROADMAP.md`: the fuel and mission model, delivered scope
  and remaining work.
- `docs/aircraft-parity.md`: real-aircraft comparison.
