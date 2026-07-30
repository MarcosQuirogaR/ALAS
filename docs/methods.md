# ALAS — Mathematics, Formulas & Solvers

Every model, equation, and solver used in ALAS, with the code location and
the configuration field that controls each tunable constant. Symbols: free-stream
Mach `M`, true airspeed `u`, density `ρ`, dynamic viscosity `μ`, dynamic pressure
`q`, wing area `S`, mean aerodynamic chord `c̄`, sweep `Λ`, thickness ratio `t/c`,
lift/drag/moment coefficients `CL`, `CD`, `Cm`.

> Notation note: the code uses `Sref = S` (main-wing planform area) as the
> aerodynamic reference area throughout.

---

## 1. Flight condition & atmosphere

The International Standard Atmosphere is provided by AeroSandbox
(`asb.Atmosphere(altitude)`), giving `ρ`, speed of sound `a`, and `μ` at the
cruise altitude.

```
u = M · a                          (true airspeed)
q = ½ · ρ · u²                     (dynamic pressure)
```

Used in: `physics/aerodynamics.py`, `optimization/objective.py`,
`analysis/full_analysis.py`. Inputs: `requirements.cruise_mach`,
`requirements.cruise_altitude_m`.

---

## 2. Cruise design point (required lift)

Level cruise requires lift = weight:

```
W            = MTOW · g
CL_required  = W / (q · S)         (derived — not a user-input stall CL)
```

`CL_required` is the **operating** cruise lift coefficient: a direct consequence of
the aircraft's weight, wing area, and flight condition. It is not the wing's
aerodynamic stall CL (which depends on section shape and high-lift devices). As the
wing area `S` shrinks, `CL_required` rises; a very small wing forces the aircraft to
cruise at a high CL with little margin before aerodynamic degradation (buffet onset,
wave-drag rise, trim difficulty).

The **cruise-CL guard** (`requirements.max_cruise_cl`, default 0.95) is a HARD
rejection: candidates whose `CL_required` exceeds this limit are discarded as
infeasible (they return `failure_cost`) because the cruise point would be
aerodynamically unsustainable. Code: `DesignRequirements.required_cruise_cl`.
Inputs: `requirements.mtow_kg`, `requirements.gravity_m_s2`.

> **Preset calibration note.** Each aircraft preset's initial wing chord values
> (root, break, tip) are chosen so that `CL_required` at the nominal design point
> sits well below 0.95 (typically 0.65–0.83), and the ±10 % optimizer bounds do
> not violate the guard at the all-variables-at-minimum corner. This ensures every
> preset produces at least a substantial fraction of valid optimizer candidates on
> the first generation.

---

## 3. Inviscid aerodynamics — Vortex-Lattice Method (VLM)

Lift, **induced** drag, and pitching moment come from AeroSandbox's
`VortexLatticeMethod`, a linear potential-flow panel method that solves for the
spanwise/chordwise circulation distribution of the lifting surfaces.

- Outputs consumed: `CL`, `CD` (induced only — VLM is inviscid), `Cm`.
- Panel density: the geometry is pre-subdivided (`*_subdivisions` in
  `GeometryConfig`); resolution multipliers are
  `analysis.spanwise_resolution`, `analysis.chordwise_resolution`.

VLM **cannot** see viscous or compressible drag; those are added by the
semi-empirical models below (hence "hybrid" aerodynamics).

---

## 4. Parasite (viscous) drag — Raymer component buildup

Total zero-lift parasite drag is summed over components `i`:

```
CD0 = ( Σ_i  Cf_i · FF_i · Q_i · (Swet_i / S) ) · k_margin
```

`k_margin = drag_model.viscous_margin` (default 1.10) lumps excrescences/gaps not
modelled component-by-component. Code: `AeroAnalysis.parasite_drag`.

### 4.1 Turbulent flat-plate skin friction (compressible Prandtl–Schlichting)

```
Cf = 0.455 / [ (log10 Re)^2.58 · (1 + 0.144 M²)^0.65 ]
Re = ρ · u · L / μ
```

`L` is the reference length of the component (MAC for a wing, body length for the
fuselage). Code: `AeroAnalysis._turbulent_cf`.

### 4.2 Wing form factor

```
FF_wing = ( 1 + 0.6/(x/c) · (t/c) + 100 · (t/c)^4 ) · ( 1.34 · M^0.18 · cos(Λ)^0.28 )
```

- `t/c` is the **actual** maximum thickness of the morphed root section
  (`Airfoil.max_thickness()`), not a constant.
- `x/c` (chordwise location of max thickness) = `drag_model.max_thickness_chordwise_loc`.
- `Λ` = the design sweep (`DesignVector.sweep_deg`), passed into `AeroAnalysis`.
- Wing wetted area: `Swet = S_planform · geometry.wing_wetted_area_factor` (≈ 2.05).
- Interference `Q = drag_model.interference_factor_wing` (default 1.0).

### 4.3 Fuselage form factor & wetted area

```
Swet_fus = π · d · L_fus · k_fus          (k_fus = geometry.fuselage_wetted_factor ≈ 0.9)
Q_fus    = drag_model.interference_factor_fuselage   (default 1.25)
```

`d` = `geometry.fuselage.diameter_m`, `L_fus` from the built fuselage stations.
(The fuselage form-factor term is folded into `Q_fus` here as a lumped factor; a
slender-body `FF = 1 + 60/f³ + f/400` can be reinstated if finer fidelity is
wanted — see "Limitations".)

### 4.4 Engine nacelle wetted area & interference

For each engine nacelle `j` in the configuration:

```
Swet_nac = π · d_nac · L_nac
Q_nac    = 1.30                           (wing-pylon junction interference factor)
```

`d_nac` = `2 · geometry.engine.radius_scale_m`, `L_nac` is the nacelle length from the engine spec.
Parasite drag skin friction is calculated using the turbulent flat-plate coefficient based on the nacelle's length Reynolds number.

---

## 5. Transonic wave drag — Korn equation

```
M_dd   = κ/cosΛ − (t/c)/cos²Λ − CL/(10 · cos³Λ)        (drag-divergence Mach)
CD_wave = C_w · (M − M_dd)^4      if M > M_dd, else 0
```

- Ignored entirely below `drag_model.wave_drag_onset_mach` (0.6).
- `κ` (airfoil technology factor) = `drag_model.korn_technology_factor` (0.95 for
  supercritical sections).
- `C_w` = `drag_model.wave_drag_coefficient` (20).
- `t/c` and `Λ` as in §4.2.

Code: `AeroAnalysis.wave_drag`.

---

## 6. Total drag and efficiency

```
CD  = CD_parasite + CD_induced + CD_wave
L/D = CL / CD
```

Code: `DragComponents.cd_total`, `AeroAnalysis.run_sweep`.

---

## 7. Fast cruise estimate (inside the optimizer)

To keep the optimization loop cheap, `quick_performance` runs **two** VLM points
(`analysis.probe_alpha_low_deg`, `probe_alpha_high_deg`) and linearises:

```
CL_α   = (CL_hi − CL_lo) / (α_hi − α_lo)            (lift-curve slope)
α_req  = α_lo + (CL_required − CL_lo) / CL_α        (trim angle of attack)

k_ind  = CD_lo / CL_lo²                             (induced-drag factor from VLM)
CD_ind = k_ind · CL_required²
CD     = CD_parasite + CD_ind + CD_wave             (parasite/wave from §4–5)
```

Code: `AeroAnalysis.quick_performance`.

---

## 8. Final drag-polar fit (full analysis)

After the fine sweep, a parabolic polar is least-squares fitted over the mid-`CL`
range (0.3–0.6, widened if too few points):

```
CD = CD0 + k · CL²                       (fit by scipy/numpy lstsq)
e  = 1 / (π · AR · k)                     (Oswald span efficiency)
```

`AR` is the main-wing aspect ratio. Code: `FullAnalysis._fit_polar`.

---

## 8a. VLM Streamline Flow Visualization

To visualize the inviscid aerodynamic flow field around the optimized configuration, streamlines are calculated by integrating velocity vectors starting from seed points upstream of the wing:

```
V = V_freestream + V_induced
```

AeroSandbox's 3D streamline tracer (`VortexLatticeMethod.calculate_streamlines`) integrates `dx/dt = V(x)` over `n_steps = 220` steps, `length = 4.5·span`. Streamline *count* (not just step count) dominates cost — each step evaluates induced velocity against every vortex filament, so cost scales as `O(n_steps · n_seeds · n_panels)`. Seeding is capped at 40 streamlines regardless of geometry complexity (`_sparse_streamline_seeds`: one seed per trailing-edge panel, thinned to the cap) rather than AeroSandbox's default auto-seeding, which targets ~200 but can overshoot far past that on geometries with many trailing-edge panels. VLM resolution for this figure is spanwise/chordwise 6/3 (still above the optimizer's default fidelity, for a legible wake, but well below the 12/6 originally used here). Measured: 3600 panels/600 streamlines/108 s → 900 panels/40 streamlines/~1 s. The resulting stream coordinates are plotted in 3D alongside the lifting-surface panels. Code: `reporting/visualization.py` (`figure_vlm_flow`, `_sparse_streamline_seeds`).

---

## 8b. NeuralFoil Airfoil Aerodynamics Sweep

To evaluate section-level lift characteristics over a range of operational conditions, the root airfoil section is simulated at multiple Reynolds numbers using NeuralFoil, a deep-neural-network-backed 2D airfoil solver:

```
[CL, CD, CM] = NeuralFoil(coordinates, alpha, Re)
```

We evaluate the lift coefficient `CL` across `alpha` from -6 to 14 degrees at Reynolds numbers `Re = [5e5, 1e6, 3e6, 1e7]`. This provides section-level stall limits, lift slope variation, and Reynolds-sensitivity trends. Code: `reporting/visualization.py` (`figure_airfoil_reynolds`).

---

## 9. Longitudinal stability & Neutral Point

Static margin from a two-point `Cm`–`CL` slope:

```
SM = −dCm/dCL ≈ −(Cm_hi − Cm_lo) / (CL_hi − CL_lo)
```

The VLM neutral point is calculated relative to the physical CG reference:
```
x_np_vlm = x_cg + SM_vlm · c̄
```

This inviscid neutral point is then corrected for tail efficiency `η_t`:
```
x_np_tail = x_wing_ac + η_t · (x_np_vlm − x_wing_ac)
```

And finally corrected for the destabilizing fuselage pitching moment `Cm_α_fus` (from wetted area buildup):
```
Δx_np_fus = Cm_α_fus · c̄ / (dCL/dα)
x_np = x_np_tail − Δx_np_fus
```

The physical static margin is then:
```
SM_physical = (x_np − x_cg_physical) / c̄
```
Probe condition: `analysis.autobalance_velocity_m_s`, `autobalance_alpha_low/high_deg`.
Code: `physics/stability.py` (`neutral_point`).

---

## 9b. Component mass buildup & physical CG

To ensure physical realism and prevent the optimizer from stretching the fuselage/empennage aft without penalty, the program performs a component weight and balance analysis.

### Mass calculation (Torenbeek methods)

1. **Wing structural mass:**
   Uses Torenbeek Appendix C:
   `m_wing = torenbeek.mass_wing(wing, MTOW, n_ult, M_suspended, V_dive, ...)`
   where `M_suspended = 0.75 * MTOW` is the assumed suspended mass.
2. **Horizontal/Vertical Stabilizer mass:**
   Uses Torenbeek Appendix C with zero suspended mass and flap deflections:
   `m_hstab = torenbeek.mass_wing(hstab, MTOW, n_ult, 0.0, V_dive, ...)`
   `m_vstab = torenbeek.mass_wing(vstab, MTOW, n_ult, 0.0, V_dive, ...)`
3. **Fuselage structure mass:**
   Uses Torenbeek Eq. 8-16 (simple method):
   `m_fus = 0.23 * (V_dive * l_tail / (W_max + H_max))^0.5 * S_wet_fus^1.2`
   where `l_tail` is the tail moment arm and `W_max`/`H_max` are the max width and height.
4. **Landing Gear mass:**
   `m_gear = 0.04 * MTOW`
5. **Propulsion mass:**
   Engine dry weight + pylons/cowling installation overhead:
   `m_prop = N_engines * (Thrust_SL / (6.0 * g)) * 1.30`
6. **Systems & Equipment mass:**
   `m_sys = 0.11 * MTOW`
7. **Payload mass:**
   Determined by aircraft configuration type:
   - Passenger: `m_payload = N_passengers * 100 kg` (passenger + baggage)
   - Cargo: `m_payload = cargo_payload_kg`
8. **Fuel mass (remainder):**
   `m_fuel = MTOW - (m_str + m_prop + m_sys + m_payload)`
   If `m_fuel` is negative, a large penalty is applied.

### Component CG coordinates — occupied-cabin model

Centroid locations are dynamically linked to geometry. Critically, **payload and systems
are placed at the centre of the *occupied* cabin length**, not the full available cabin.
This prevents the optimizer from gaining a free CG balance by stretching the fuselage.

```
L_cabin       = L_fus - x_cabin_start - L_tailcone        (total available cabin)
L_occ         = min(L_cabin, m_payload / ρ_cabin)          (occupied portion)
ρ_cabin       = mass_model.cabin_payload_density_kg_m       (default 800 kg/m)

x_payload     = x_cabin_start + 0.50 * L_occ
x_systems     = x_cabin_start + 0.35 * L_occ
```

When `L_fus` grows but the required payload stays fixed, `L_occ` stays constant and
the payload CG does **not** drift aft — the optimizer must close the resulting
CG mismatch by other means (or accept the penalty).

Other component centroids:
- **Fuselage structure:** `[0.45 * L_fus, 0, Z_fus]`
- **Wing:** `[x_ac + 0.20 * c_root, 0, Z_wing_root]`
- **Tail surfaces:** `[L_fus - offset/2, 0, Z_stabilizer]`
- **Propulsion:** Nacelle centroid if nacelles are generated, else wing root AC.
- **Fuel:** Wing aerodynamic center `x_ac`.

### Global physical CG

```
x_cg_physical = (Σ mᵢ · xᵢ) / (Σ mᵢ)
```

### Optimizer penalties

The cost function penalizes mismatch between the physical static margin `SM_physical` and the required target static margin `SM_target`:
```
J_cg = (SM_physical - SM_target)² · w_cg
```
If fuel mass is negative:
```
J_fuel = |m_fuel| · w_fuel
```

---

## 9c. Tail volume coefficient constraints

Tail volume coefficients measure stabiliser *effectiveness* by combining area and
moment arm. Unlike area-fraction constraints, they penalise both too-short tails
(undersized for the wing) and artificially long-fuselage configurations that reduce
tail area while exploiting the large moment arm.

```
Vh = Sh * Lh / (S * c̄)      (horizontal tail volume coefficient)
Vv = Sv * Lv / (S * b)       (vertical tail volume coefficient)
```

`Lh` and `Lv` are measured **from the wing aerodynamic centre to the stabiliser
aerodynamic centre** — the Etkin/Reid convention. Raymer's tables use the CG as
origin; the difference is ≈ SM × c̄ ≈ 4 % at SM = 10 % MAC and is absorbed in the
bound tolerances.

**Published ranges (CS-25/FAR-25 transports — Raymer Ch. 6, Torenbeek §9.3):**

| Coefficient | Typical range (Raymer, CG-ref) | ALAS bounds (Etkin, wing-AC-ref) |
|-------------|-------------------------------|--------------------------------------|
| Vh          | 0.90 – 1.10                   | 0.75 – 1.25                          |
| Vv          | 0.06 – 0.12                   | 0.06 – 0.13                          |

> **Convention note**: The AVE freighter reference design yields Vh ≈ 0.82 —
> below the textbook floor but accepted as a lean freighter outlier.
> Passenger-transport designs typically land above 0.85.

The optimizer penalty is quadratic outside the valid range, consistent with the
normalisation of all other penalty terms:

```
if Vh < Vh_min:  J_Vh = ((Vh_min - Vh) / Vh_min)² * w_vol
if Vh > Vh_max:  J_Vh = ((Vh - Vh_max) / Vh_max)² * w_vol
(same structure for Vv)
```

Weights: `optimizer.weights.min/max_hstab_volume_coef`, `min/max_vstab_volume_coef`,
`tail_volume_penalty_scale` (default 200).

The Vh/Vv formula itself lives in `physics/stability.py::tail_volume_coefficients()`
(a single source of truth); `optimization/objective.py`'s penalty term and the
control-surface sizing diagram (§9f) both call it, so the diagram always shows the
exact number the optimizer is actually enforcing against these bounds.

---

## 9d. Longitudinal trim solve (three-point VLM probe)

Two-point stability alone (§9) does not determine a physically trimmed
condition — it measures `dCm/dCL` at a fixed elevator/stabiliser setting,
leaving `Cm` generally nonzero at the cruise angle of attack. A trimmed
aircraft must satisfy both lift equilibrium and moment equilibrium
simultaneously:

```
CL(α, i_h) = CL_required        (lift = weight)
Cm(α, i_h) = 0                  (pitching moment about the CG = 0)
```

where `i_h` is the horizontal stabiliser's rigid incidence angle — a real,
physically adjustable trim mechanism on most transports (a "trimmable
horizontal stabiliser," distinct from elevator deflection but the standard
trim device modelled here), following Etkin & Reid's trimmed-equilibrium
formulation (already this codebase's named convention for §9c's tail volume
coefficients).

For small perturbations, `CL` and `Cm` are locally linear in `(α, i_h)`:

```
CL(a, i) = CL_1 + CL_α · (a − a_lo) + CL_ih · (i − i_h0)
Cm(a, i) = Cm_1 + Cm_α · (a − a_lo) + Cm_ih · (i − i_h0)
```

Three VLM evaluations at the cruise Mach/altitude give the four needed
partial derivatives from three points (not four), by holding one variable
fixed between each pair of probes:

```
Point 1: (a_lo, i_h0)                    -- baseline
Point 2: (a_hi, i_h0)                    -- alpha probe
         CL_α = (CL_2 − CL_1) / (a_hi − a_lo)
         Cm_α = (Cm_2 − Cm_1) / (a_hi − a_lo)
Point 3: (a_lo, i_h0 + Δi_h)              -- incidence probe
         CL_ih = (CL_3 − CL_1) / Δi_h
         Cm_ih = (Cm_3 − Cm_1) / Δi_h
```

The trimmed `(α*, i_h*)` solves the resulting 2×2 linear system in closed
form (no iteration):

```
[ CL_α  CL_ih ] [ a* − a_lo ]   [ CL_required − CL_1 ]
[ Cm_α  Cm_ih ] [ i* − i_h0 ] = [        −Cm_1        ]
```

A final VLM evaluation at `(α*, i_h*)` gives the true trimmed `CL`/`CD`/`Cm` —
the `CD` at this point already reflects whatever induced-drag penalty the
tail's required lift/download to achieve `Cm = 0` imposes (the physical trim
drag), since the VLM solves the whole-aircraft circulation distribution
self-consistently. No separate ad-hoc trim-drag formula is added on top; the
residual `Cm` at this final evaluation (should be ≈ 0) is kept only as a
diagnostic of how well the small-perturbation linearisation held.

`i_h0` is the current root-station twist of the "Horizontal Stabilizer" wing
(`geometry/aircraft_builder.py`'s `hstab_root/tip_twist_deg`); the probe
perturbs every cross-section's twist by the same rigid delta
(`analysis.trim_incidence_probe_delta_deg`, default 1°) and restores it
afterward — `WingXSec.twist` is a plain mutable float, perturbed in place the
same way `stability.autobalance()` already mutates `airplane.xyz_ref[0]`.

**Probe condition.** Static margin here (same formula as §9: tail efficiency
`η_t` + fuselage Munk/Multhopp correction) is evaluated from probes 1 & 2 at
the *cruise* Mach/altitude, not the fixed low-speed reference condition §9's
`neutral_point()` uses. This is a deliberate consolidation for the optimizer
and final-analysis code paths only: it replaces what used to be two
independent 2-point VLM probes (`neutral_point`'s stability probe +
`quick_performance`'s untrimmed alpha probe, 4 VLM solves total) with one
3-point probe serving both purposes (3 VLM solves), a net reduction while
adding real trim physics that didn't exist before. `neutral_point()` itself
is unchanged and still used by the deliberately-cheap Stage-0 baseline pass
(§17b), so its reported static margin is unaffected by this consolidation.

If the airplane has no horizontal stabiliser, the solve degrades to a
pure-alpha trim (`CL(α) = CL_required` from probes 1 & 2 only; `i_h`
undefined) — never raises for that case.

Code: `physics/stability.py::stability_and_trim`,
`physics/aerodynamics.py::AeroAnalysis.trimmed_performance`.
Config: `analysis.trim_incidence_probe_delta_deg`,
`analysis.probe_alpha_low/high_deg`, `analysis.tail_efficiency`.
Consumers: `optimization/objective.py` (every candidate evaluation) and
`analysis/full_analysis.py` (the final design's `trimmed_design_point`,
alongside the untrimmed alpha-sweep `design_point` used for the drag-polar
fit/figures, see §16/§17).

---

## 9f. Control-surface layout & dynamic-mode analysis

**Control-surface diagram (`reporting/visualization.py::figure_control_surfaces`).**
Slat/flap/aileron/spoiler (wing) and elevator/rudder (tail) chord-fraction and
span-fraction (of local semi-span) regions from `config/control_surfaces_config.py`
are drawn as shaded patches on a top view (wing + h-stab) and a v-stab side view
(rudder — a top-down projection can't show a vertical surface's chordwise control
region). This is a pure *representation* input: ALAS's wings are plain lifting
surfaces without deflectable sub-geometry, so these fractions don't feed the
aero/mass models, only this diagram. A patch's leading-edge x/chord at a given
span station is linearly interpolated between the enclosing pair of `WingXSec`s;
its area is the trapezoidal `0.5*(c0+c1)*chord_frac*(s1-s0)`, doubled if the
surface is on a symmetric (mirrored) wing/h-stab. Vh/Vv (§9c) are shown as a
pass/fail readout against `optimizer.weights`' configured bounds.

**Dynamic-mode analysis (`physics/dynamics.py`).** Longitudinal (phugoid,
short-period) and lateral-directional (dutch roll, roll subsidence, spiral) modes
of the linearized small-perturbation dynamics, computed via AeroSandbox's own
`dynamics.flight_dynamics.airplane.get_modes(airplane, op_point, mass_props, aero)`.
`aero` is a VLM stability-derivative dict
(`VortexLatticeMethod.run_with_stability_derivatives(alpha=True, beta=True, p=True,
q=True, r=True)` — CL, CD, Cma, Cmq, Clp, CYb, Cnb, CYr, Cnr, Clb, Clr). `mass_props`
needs `Ixx`/`Iyy`/`Izz` (not `Ixz`, despite it being an `asb.MassProperties`
constructor field), which ALAS doesn't otherwise compute — estimated via the
standard radius-of-gyration approximation:

```
rx = 0.25 * span            Ixx = m * rx²
ry = 0.38 * fuselage_length  Iyy = m * ry²
rz = 0.40 * fuselage_length  Izz = m * rz²
```

a well-established conceptual-design approximation, not a substitute for a real
structural mass-properties model. Each mode's result carries `eigenvalue_real`
(damping), `eigenvalue_imag` (frequency), `damping_ratio`, and
`period = 2π/|eigenvalue|` (0 for the purely-real roll-subsidence/spiral modes).
Stability is `eigenvalue_real < 0`. Resolution is kept at the project's standard
`spanwise_resolution=1` (not the higher resolution `figure_vlm_flow` uses for pure
shape visualization) — `run_with_stability_derivatives` runs ~6 finite-difference
VLM solves internally, so resolution has an outsized effect on wall time here;
benchmarked directly, resolution 1 vs. 4 changes phugoid/short-period eigenvalues
by <1% and dutch-roll by ~12% while cutting time 9s → 0.8s.

Visualized as an s-plane pole plot (real part = damping, imaginary part =
frequency, shaded left-half-plane = stable) on a symmetric-log axis scale, since
phugoid/spiral are typically 1-2 orders of magnitude slower than
short-period/dutch-roll — a linear scale collapses the slow modes onto the origin.
`linthresh` is floored at 1% of the plot span rather than derived purely from the
smallest eigenvalue, which otherwise leaves too narrow a linear region and crowds
symlog's automatic tick labels right at the origin.

Code: `physics/dynamics.py`, `physics/stability.py`,
`reporting/visualization.py` (`figure_control_surfaces`, `figure_dynamic_modes`),
`config/control_surfaces_config.py`.

---

## 10. Geometry parametrization

Main wing (3 sections: root, break, tip), with break at a fraction of semi-span:

```
y_break = f_break · (b/2)                              (f_break = wing.break_span_fraction)
Δx_break = y_break · tan(Λ_in)
Δx_tip   = Δx_break + (b/2 − y_break) · tan(Λ_out)
Λ_out    = Λ_in − Δsweep                               (wing.outboard_sweep_decrement_deg)
```

Empennage in-plane dimensions scale with `DesignVector.tail_scale`. Engine inlet
longitudinal station:

```
x_inlet = ( x_wing_global + |y_engine| · tan Λ ) − x_offset    (engine.inlet_x_offset_m)
```

### 10.1 Airliner-like Fuselage profile

The fuselage geometry is divided into three regions (Nose, Cabin, and Tailcone) using mathematically smooth profiles to mimic airliner shaping:

1. **Nose ($0$ to $x_{cabin\_start}$):**
   10 stations spaced sinusoidally using `np.sinspace`.
   * Equivalent radius: $r(x_i) = R_{cabin} \cdot \sqrt{1 - (1 - x_i)^2}$ (ellipsoidal quadrant)
   * Centerline Z: $z(x_i) = z_{cabin} + (z_{nose} - z_{cabin}) \cdot (1 - x_i)^2$ (parabolic droop curve)
   where $x_i = x / x_{cabin\_start} \in [0, 1]$.

2. **Cabin ($x_{cabin\_start}$ to $x_{cabin\_end}$):**
   Constant cross-section with equivalent radius $R_{cabin}$ and centerline $z_{cabin}$.

3. **Tailcone ($x_{cabin\_end}$ to $x_{tail}$):**
   10 stations spaced linearly.
   * Equivalent radius: $r(x_j) = R_{cabin} \cdot (1 - x_j^{1.5})$ (smooth taper)
   * Centerline Z: $z(x_j) = z_{cabin} + (z_{tail} - z_{cabin}) \cdot x_j^{1.5}$ (upsweep curve)
   where $x_j = (x - x_{cabin\_end}) / L_{tailcone} \in [0, 1]$.

4. **Cross-Section Shaping (Circular vs. Ovoid):**
   * **Circular (Default):** Each cross-section station is circular with radius equal to the equivalent radius $r$.
   * **Ovoid (e.g., A380 Double-Deck):** If a distinct `height_m` is specified, the cross-sections are modeled as ellipses (superellipse shape parameter 2.0) with:
     $$\text{Width} = 2 \cdot r$$
     $$\text{Height} = 2 \cdot r \cdot \frac{h_{cabin}}{d_{cabin}}$$
     where $d_{cabin}$ is the cabin diameter (`diameter_m`) and $h_{cabin}$ is the cabin height (`height_m`).

Code: `geometry/aircraft_builder.py`. The wing location coordinates are defined in absolute space; the physical weight and balance analysis (§9b) computes the final physical CG and static margin.

---

## 11. Airfoil shaping

The working wing section is produced by **bumps → morphing** from a reference
section (`geometry/airfoils.py`, `build_section`).

### 11.1 Local bumps (Hicks–Henne-style)

Additive surface perturbation at chordwise centre `x_c`:

```
Δy(x) = A · sin(π·x)^w · exp( −10 · (x − x_c)² ),   w = 2.5
```

Applied at four control stations (upper ~0.25 & ~0.75, lower ~0.40 & ~0.85), with
amplitudes `A` from the four `bump_*` design variables. Bumps vanish at the LE/TE.
Code: `apply_bumps`.

### 11.2 Global thickness/camber morphing

Decompose the section into thickness and camber, scale independently, recombine:

```
t(x)      = y_upper(x) − y_lower(x)
camber(x) = ½ ( y_upper(x) + y_lower(x) )

t'      = s_t · t                  (s_t = airfoil_thickness_scale)
camber' = s_c · camber             (s_c = airfoil_camber_scale)

y_upper' = camber' + t'/2
y_lower' = camber' − t'/2
```

Code: `morph_airfoil`. This gives the optimizer independent control over wing-box
volume / wave drag (thickness) and section loading / pitching moment (camber).

---

## 12. Objective function (what the optimizer minimises)

For a candidate design with genuinely *trimmed* `L/D` and trim `α` (§9d — not
the old untrimmed 2-point estimate), the cost is the primary L/D reward plus a
sum of independent, mostly-soft penalty terms (defaults from
`config/optimizer_config.py::ObjectiveWeights`; see §16 for the full weight
table):

```
J = −ld_weight · (L/D)
    + J_payload                                  # payload/seating shortfall
    + J_tail_area + J_tail_volume                 # tail sizing floors
    + J_wing_pos                                  # wing-root-vs-cockpit floor
    + J_fineness                                  # fuselage slenderness ceiling
    + J_alpha                                     # trim-angle window penalty
    + span_penalty_per_m · span                   # structural-mass proxy
    + cd0_penalty_scale · CD0                     # parasite-drag floor
    + J_area + J_WS                               # wing area / loading constraints
    + J_SM_target                                 # static-margin TARGET deviation (soft)
    + J_SM_floor                                  # static-margin FLOOR (graduated, see below)
    + J_cg_envelope                               # CG-envelope violation / compliance reward
    + J_fuel + J_fuel_volume                      # fuel-budget and wing-tank-volume checks
    + J_thickness + J_fuselage_floor              # airfoil-thickness / min-fuselage-length floors
    + J_taper + J_te_angle                        # wing-taper / TE-root-angle realism
    + J_empty_stretch                             # penalises fuselage length beyond payload need

J_alpha = 0                                if α ∈ [alpha_min_penalty_deg, alpha_max_penalty_deg]
        = (α − α_bound)² · alpha_penalty_scale     otherwise (α_bound = whichever edge was crossed)

J_area = (ΔS/S_max)² · area_penalty_scale          if S > S_max = requirements.max_wing_area_m2, else 0
J_WS   = (ΔWS/WS_min)² · wing_loading_penalty_scale  if W/S < WS_min = requirements.min_wing_loading_kg_m2, else 0

J_SM_target = (SM − SM_target)² · static_margin_penalty_scale        if |SM − SM_target| ≤ 0.5
            = (|SM − SM_target| · 10)³                                otherwise (steep cubic beyond 0.5 MAC)

J_SM_floor = (deficit · severity)³   where deficit = max(0, min_physical_static_margin − SM), severity derived
             from instability_failure_cost (severity = 20 at the field's default 1000) -- see §17/§17's
             "physical-validity penalties" note: NOT an early return, added on top of the real L/D.

J_cg_envelope = (worst_exceedance·100)² · cg_envelope_penalty_scale   if any of OEW/MZFW/MTOW violates the
                dynamic (aero ∩ gear-derived) envelope (§17)
              = −cg_envelope_reward                                   otherwise (compliance reward)
```

The trim-alpha window (`alpha_min_penalty_deg`/`alpha_max_penalty_deg`,
default 0°/10°) lives entirely on `ObjectiveWeights`, not on
`DesignRequirements` — an earlier version of this doc described a separate
`requirements.target_cruise_alpha_deg`/`acceptable_alpha_band_deg` pair, but
those fields were never actually read by this function and have since been
removed from the config.

Two different mechanisms mark a candidate *invalid* (`n_valid` not
incremented, tallied in `OptimizationHistory.reject_reason_counts`), and they
behave differently on purpose:

- **Build/analysis crash — a true early-return hard reject**
  (`optimizer.weights.failure_cost`): geometry build failure, mass-analysis
  crash, VLM/stability solve crash, or `CL_required > max_cruise_cl` (§2, the
  stall guard). These leave no sensible operating point to evaluate at all,
  so the candidate never reaches the cost formula above.
- **Physical instability / CG-envelope violation — NOT an early return**
  (`J_SM_floor`/`J_cg_envelope` above): `static_margin <
  min_physical_static_margin`, or the physical CG outside the `[fwd_lim,
  aft_lim]` envelope (§17) at any of the OEW/MZFW/MTOW loading states, or a
  payload/seating shortfall. Unlike a build/analysis crash, `L/D` is still
  computed for these candidates and folded into the cost formula above — only
  a large, graduated penalty is *added* on top, never a bare early return.
  This is a deliberate correction: an earlier version of this function *did*
  early-return a flat/graduated cost with no `L/D` at all for these cases,
  which blinded `differential_evolution` to aerodynamic quality for every
  non-compliant candidate and, empirically, prevented the solver from ever
  finding a compliant *and* aerodynamically-decent region even when one
  existed in the search space. The penalty is still
  large enough that a compliant candidate always beats a non-compliant one
  when both are reachable — it's a dominant term, not a competing one — but
  it never erases the L/D signal the solver needs to find its way there.
  `reject_reason` records which of `static_margin`/`cg_envelope`/
  `payload_shortfall` fired (a candidate can hit more than one at once).
- **Soft penalties** (`J_alpha`, `J_area`, `J_WS`, `J_SM_target`, and the
  remaining structural/geometric terms above) — the candidate is *valid* but
  costs more; the optimizer can still escape these regions.

Code: `optimization/objective.py`.

---

## 13. Solvers used

| Solver | Purpose | Library | Key settings |
|--------|---------|---------|--------------|
| **Vortex-Lattice Method** | inviscid CL, CD_i, Cm | AeroSandbox | resolution multipliers (`AnalysisConfig`) |
| **Differential Evolution** | global design-space search | `scipy.optimize` | strategy, maxiter, popsize, tol, seed, workers (`SolverSettings`) |
| **Linear least squares** (`lstsq`) | drag-polar fit `CD=CD0+kCL²` | NumPy/SciPy | mid-CL mask |
| **ISA atmosphere** | ρ, a, μ vs altitude | AeroSandbox | cruise altitude |

Differential evolution is chosen because the design space is non-convex,
mixed-sensitivity, and the objective is noisy/discontinuous (penalty walls,
solver failures) — a gradient-free global method is appropriate.

### 13a. Initial population seeding (`SolverSettings.seed_near_initial_design`)

By default (`True`), the population is NOT SciPy's default full-space
latin-hypercube sample — it's a tight cluster of small random perturbations
around the initial/preset design (`SolverSettings.seed_perturbation_fraction`,
default 5% of each DOF's bound range), with the unperturbed initial design
itself placed at population index 0. This follows SciPy's own documented
recommendation for the `init` parameter: an array "could be used ... to
create a tight bunch of initial guesses in a location where the solution is
known to exist, thereby reducing time for convergence."

This matters because a uniform-random population routinely produces
candidates that stretch the fuselage (or otherwise move the payload CG) far
more than the wing/tail-shift DOFs happen to compensate for in the *same*
candidate — even though the initial/preset design itself is CG-compliant,
most or all of generation 0 (and sometimes the whole run) could otherwise be
physically invalid, purely because independent uniform sampling across 16
dimensions rarely lands near the (potentially narrow) compliant sub-region.
Verified empirically on both the AVE and A320-200 reference cases:
generation-0 valid fraction went from 25-0% (uniform sampling) to 100%
(seeded), and overall valid fraction across a full run roughly doubled.

If `initial_design`'s array falls outside the bounds actually passed to
`DesignOptimizer.run()` (e.g. bounds not yet narrowed to the loaded preset),
seeding is skipped in favour of the ordinary full-space fallback rather than
silently clipping the initial design into an unrelated point — see
`optimizer.py::DesignOptimizer.run` for the guard. In the GUI, this can't
happen in practice: `DesignSpaceTable` always narrows `Lower`/`Upper` to
±10% around whichever preset's vector is loaded (§2a of `architecture.md`),
so `initial_design` and `bounds` are always mutually consistent there.

Code: `optimization/optimizer.py`. Config: `SolverSettings.
seed_near_initial_design`, `SolverSettings.seed_perturbation_fraction`.

---

## 14. Assumptions & limitations

- **Inviscid lift.** VLM ignores viscous decambering and stall; valid for
  attached, pre-stall cruise conditions only.
- **Empirical drag.** The Raymer/Korn buildups are preliminary-design estimates
  (typical accuracy ~±10%), not CFD. The lumped `viscous_margin` and the
  fuselage `Q` factor absorb several finer effects.
- **Wave drag** uses a single representative `t/c` and `Λ`; it is a trend model,
  not a transonic flow solution.
- **Fast vs fine consistency.** §7 linearises lift and scales induced drag from a
  single VLM point; it can diverge from the §6 sweep at high `CL`. The final
  reported numbers always come from the fine sweep.
- **Static margin** uses a CG reference shift as a proxy for true mass balancing;
  a mass/CG buildup is a roadmap item (SUAVE handoff).

---

## 15. Low-speed performance & matching chart (`physics/performance.py`)

All formulas use SI units unless noted; Raymer constants (originally imperial) are
converted inline. Code: `physics/performance.py`, consumed by
`alas/sidecar/figures_extra.py`'s `figure_matching_chart`/`_figure_lto`
(the desktop GUI's Matching Chart and Landing & Take-Off figures).

Every tunable constant used in this section is exposed as a field of
`config/performance_config.py::PerformanceConfig` and is editable at runtime
via Advanced Settings → Performance without touching code:

| `PerformanceConfig` field | Default | Used in |
|--------------------------|---------|---------|
| `cl_max_to`              | 1.80    | §15d, §15f (Vstall_TO, V1, VR, V2) |
| `cl_max_land`            | 2.60    | §15e, §15f (Vstall_land, VAPP, VTD) |
| `thrust_lapse`  η        | 0.235   | §15b (cruise T/W₀ to sea-level static) |
| `oei_gradient`  γ_min    | auto    | §15c (0.024/0.027/0.030 per N; fallback for exotic configs) |
| `k_land`        K        | 0.60    | §15e (landing field length factor) |

### 15a. Density ratio

```
σ = ρ_local / ρ₀ = p / (287.05 · (T_ISA + ΔT_ISA)) / 1.225
```

Where `T_ISA` is the ISA temperature at field elevation and `ΔT_ISA` is the
user-specified hot-day deviation. Source: `config/airports.py`, each `Airport` entry
carries `elevation_m` and `isa_deviation_c`.

### 15b. Cruise T/W₀ constraint

Level flight at cruise altitude gives the required thrust-to-weight at altitude:

```
(T/W)_alt = q·CD₀/(W/S) + k·(W/S)/q
```

Converting to sea-level static via a fixed lapse rate η (default 0.235 for
high-bypass turbofan at M 0.84, FL390):

```
(T/W₀) = (T/W)_alt / η
```

### 15c. OEI second-segment climb (FAR 25.121)

At 1.2·Vstall in take-off configuration:

```
CD_TO = CD₀ + 0.025 + k·1.2²      (0.025 ≈ flap/gear increment)
(T/W₀) = [N/(N−1)] · (CD_TO/1.2 + γ_min)
```

Where `N` is the number of engines and `γ_min` is the FAR 25.121 minimum gross
climb gradient, which depends on engine count:

| N (engines) | γ_min  | Aircraft type    |
|-------------|--------|------------------|
| 2           | 0.024  | Twin-jet         |
| 3           | 0.027  | Tri-jet          |
| 4           | 0.030  | Quad-jet         |

The matching chart auto-selects the correct `γ_min` from `FAR25_OEI_GRADIENT`
in `physics/performance.py` based on the designed engine count. The value in
`PerformanceConfig.oei_gradient` serves as a fallback for non-standard
configurations. This constraint is independent of W/S.

### 15d. Take-off field length (Raymer Ch.17)

Empirical regression for jet transports (Raymer, Table 17.1, imperial units
converted):

```
T/W₀ = 37.7 · (W/S [lb/ft²]) / (σ · CL_max_TO · TODA [ft])
```

Rearranged for a given T/W₀:

```
TODR [ft] = 37.7 · (W/S [psf]) / (σ · CL_max_TO · T/W₀)
TODR [m]  = TODR [ft] / 3.28084
```

### 15e. Landing field length / W/S limit

Reverse of the landing-distance formula (K = 0.60):

```
(W/S)_max [Pa] = LDA [m] · σ · CL_max_land / K
```

Required landing distance at design W/S:

```
LDR [m] = (W/S [Pa]) · K / (σ · CL_max_land)
```

### 15f. V speeds (FAR 25)

All evaluated at field density ρ = σ·1.225 and aircraft MTOW W/S. Every
multiplicative factor below is a `PerformanceConfig` field (defaults shown;
see `config/performance_presets.py` for the per-technology-class bundles —
"standard narrow-body", "modern narrow-body", "advanced high-lift widebody",
"conservative simple flaps" — applied per-preset in `config/presets.py`):

| Speed | Formula | FAR 25 ref |
|-------|---------|------------|
| Vstall_TO  | √(2·W/S / (ρ·CL_max_TO))  | 25.103 |
| Vstall_land | √(2·W/S / (ρ·CL_max_land)) | 25.103 |
| Vmc  | vmc_vstall_factor·Vstall_TO (default 1.13) | 25.149 |
| VR   | max(vr_vmc_factor·Vmc, vr_vstall_factor·Vstall_TO) (defaults 1.05, 1.10) | 25.107 |
| V1   | max(v1_vr_factor·VR, Vmc) (default v1_vr_factor 0.95) | 25.107 |
| V2   | max(v2_vstall_factor·Vstall_TO, VR) (default 1.20) | 25.107 |
| VAPP | vapp_vstall_land_factor·Vstall_land (default 1.30) | 25.125 |
| VTD  | vtd_vstall_land_factor·Vstall_land (default 1.15) | 25.125 |

`V1` is explicitly floored at `Vmc` — FAR 25.107 requires the decision speed
never sit below minimum control speed, which a tight
`vr_vmc_factor`/`vr_vstall_factor` combination could otherwise violate by a
fraction of a knot even with `v1_vr_factor` close to 1. An earlier version of
this table showed a fixed, unconfigurable `V1 = 0.90·VR` with no VMC floor;
`0.90` put V1 unrealistically far below VR (e.g. ~152 kt against a real 787-9
V1 of 160-165 kt) and is superseded by the per-preset-calibrated
`v1_vr_factor` above.

### 15g. Balanced field length (BFL)

Raymer approximation for twin-engine jet transports:

```
BFL ≈ 1.15 · TODR
ASD ≈ BFL   (at the balanced-field point, ASD = TOD)
```

### 15h. V-n (flight envelope) diagram (`physics/performance.py::build_vn_diagram`)

CS-25-style flight envelope, all speeds in equivalent airspeed (EAS) at MTOW,
sea-level reference density. Every boundary is derived from existing
`requirements`/`performance` config rather than new hardcoded design speeds:

```
n_ult,pos = requirements.ultimate_load_factor          (existing field, default 3.75)
n_lim,pos = n_ult,pos / 1.5                             (CS-25.303 factor of safety)
n_lim,neg = requirements.limit_load_factor_neg          (new field, default -1.0, CS-25.337(c))
n_ult,neg = n_lim,neg * 1.5

VD = requirements.dive_speed_m_s                        (existing field)
VC = VD / 1.25                                           (CS-25.335(b) minimum margin)

Vs = sqrt(2·W / (ρ_SL·S·CLmax_clean))                    (1g clean stall at MTOW)
Va = sqrt(2·n_lim,pos·W / (ρ_SL·S·CLmax_clean))          (maneuvering speed)

n_stall(v) = 0.5·ρ_SL·v²·S·CLmax_clean / W               (stall boundary, ± for CLmax_clean/CLmin_clean)
```

`CLmax_clean`/`CLmin_clean` are new `PerformanceConfig` fields (the true clean-
configuration stall CL, distinct from the existing flaps-down `cl_max_to`/
`cl_max_land`, which don't apply to the structural envelope). The actual
operating cruise point (EAS from the real cruise Mach/altitude, converted via
`sqrt(ρ_alt/ρ_SL)`) is plotted as a separate marker from the VC design-speed
boundary — they answer different questions (where the aircraft actually flies
vs. the certification design margin).

Zone construction mirrors the standard V-n diagram convention: green (normal,
v≤VC) and yellow (caution, VC<v≤VD) both bounded by `min(n_stall, n_lim)`;
orange (structural margin) fills between that and `min(n_stall, n_ult)`
(pinches to zero width below Va, where the aircraft physically can't reach
even the limit load); red (never-exceed) covers the full stall envelope plus
v>VD. Every zone boundary is drawn with its own line (including the
ultimate-load edge) and every label position is computed relative to the
actual Va/VC/VD/n values rather than fixed offsets, so the diagram stays
correctly proportioned for any aircraft size.

Code: `physics/performance.py` (`build_vn_diagram`), `reporting/visualization.py`
(`figure_vn_diagram`).

### 15i. Breguet range & payload-range diagram (`physics/performance.py`)

Standard Breguet range equation, SI throughout:

```
R = (V / (c·g)) · (L/D) · ln(W_start / W_end)
```

where `c` is thrust-specific fuel consumption in kg fuel / (N·s) (mass flow
per unit thrust force per second), converted from the more readable
`EngineSpec.cruise_tsfc_kg_kgf_hr` config unit via `c = tsfc_kg_kgf_hr /
(g · 3600)`. `V` is cruise TAS; `L/D` is the trimmed cruise value
(`report.trimmed_design_point`, falling back to the untrimmed `design_point`).

**Wing fuel-tank volume** (`wing_fuel_volume_m3`), Torenbeek's geometric
estimate:

```
V_geo = 0.54 · (S²/b) · (t/c)_root · (1 + λ + λ²) / (1 + λ)²
```

with S, b, taper ratio λ, and root `(t/c)` read directly from the built
`asb.Wing` (`wing.area()`, `wing.span()`, `wing.taper_ratio()`, root xsec's
`airfoil.max_thickness()`) rather than static config, so the estimate tracks
whatever geometry the optimizer/preset actually produced. The 0.54
coefficient already represents "usable tank fraction of the theoretical
wing-box volume" (Torenbeek 1982); `MassModelConfig.fuel_tank_usable_fraction`
(default 0.85) is an additional derate on top for structure/ribs/systems/
unusable-fuel allowance. Converted to a mass capacity via
`MassModelConfig.fuel_density_kg_m3` (804, Jet-A).

**Payload-range 4-point diagram** (`payload_range_diagram`): fuel capacity is
`min(MTOW − OEW, tank_volume_m3 · fuel_density · usable_fraction)` — whichever
constraint actually binds (structural weight budget vs. physical tank
volume), unlike a single hardcoded `Fuel_Max_Capacity`. `max_payload_kg` is
the payload the analyzed design's detailed cabin/cargo layout actually
carries (`report.component_masses["Payload"]`), not an independently-derived
structural maximum. Points:

```
A: max payload, 0 fuel, range = 0
B: max payload, fuel = min(fuel_capacity, MTOW - OEW - max_payload)
C: fuel = fuel_capacity, payload = max(0, min(max_payload, MTOW - OEW - fuel_capacity))
D: fuel = fuel_capacity, 0 payload (ferry range)
```

B and C can coincide (when the wing-tank volume, not MTOW, is the binding
fuel constraint even at max payload) — `figure_payload_range` merges their
labels in that case rather than drawing two overlapping annotations.

Code: `physics/performance.py` (`wing_fuel_volume_m3`, `breguet_range_m`,
`payload_range_diagram`, `fuel_volume_check`), `reporting/visualization.py`
(`figure_payload_range`, `figure_fuel_volume_check`).

---

## 16. Optimization objective function

The design optimizer (SciPy `differential_evolution`) minimises a scalar cost
assembled from a primary aerodynamic reward and a large set of soft-penalty
terms (§12 gives the full term list with formulas; this section is the
tunable-weights reference). The formula extends well past a bare ~10-term
form: payload/seating shortfall, tail-area and tail-volume floors,
wing-position and fineness-ratio floors, wing-taper and TE-root-angle
realism, wing fuel-tank-volume capacity, and a fuselage-empty-stretch
penalty. Two of the weight defaults below reflect a recalibration made when
a duplicate static-margin term was removed from the sum.

### Wing-taper realism (closes the "free MAC inflation" loophole)

`break_chord_m` (6–10 m) is a free design variable while the break station's
*spanwise* position (`GeometryConfig.wing.break_span_fraction`) is fixed —
so the optimizer can inflate the wing's mean aerodynamic chord (`c_ref`,
i.e. MAC) "for free" by pushing `break_chord_m` toward `root_chord_m`
(blunting the yehudi break) rather than by tapering it realistically.
Since **every** %MAC-normalised term (`cg_envelope_penalty_scale`, the
static-margin target penalty, and the physical static-margin definition
itself, `SM = (x_np − x_cg)/MAC`, §9) divides by that same MAC, enlarging it
directly cheapens all of them without a genuine stability improvement — a
real reward-hacking avenue verified against the 7 shipped presets (which
land at break/root ratios of 0.47–0.62, comfortably under the default bound
below).

```
taper_ratio = break_chord_m / root_chord_m
penalty = 0                                             if taper_ratio ≤ max_break_root_chord_ratio
        = (taper_ratio − max_break_root_chord_ratio)² × taper_realism_penalty_scale   otherwise
```

Soft, not a hard bound — a design can still cross it if the L/D gain is
worth the cost. Code: `optimization/objective.py` (added directly after
the fuselage-empty-stretch penalty, the sibling "closes a loophole" term).

The static-margin *target*-tracking term is `static_margin_penalty_scale`
(the soft preference nudging a compliant-but-suboptimal candidate toward
`requirements.target_static_margin`) — distinct from
`cg_envelope_penalty_scale`/`cg_envelope_reward`/`instability_failure_cost`
below, which are the *physical-validity* terms (§12/§17): a CG-envelope
violation or a static margin below `min_physical_static_margin` adds a
large, dominant-but-continuous penalty on top of the rest of the formula —
`L/D` is still computed and still contributes — rather than short-circuiting
the assembly with an early return. `cg_penalty_scale` (below) is a
**legacy, unused** field, kept only so old saved YAML configs referencing it
still load: it used to double-apply the exact same static-margin-vs-target
formula under a different name (a bug, not a distinct mechanism). The two
are now consolidated into the single `static_margin_penalty_scale` term.

### Penalty normalization rationale

All penalties are expressed as dimensionless fractions so they are **O(1–50)**
near a good design, comparable to the primary reward −L/D ≈ −15 to −22.  The
original implementation used raw physical magnitudes (e.g. kg², m²) which
caused penalties of O(1000), hiding any signal from L/D.

| Term | Normalization | Good-design magnitude |
|------|---------------|----------------------|
| Static-margin target | (SM − SM_target)² × 20.0 — a 5% MAC miss costs 0.05 | < 1 |
| Fuel deficit | m_fuel / MTOW — zero at balanced budget | 0 |
| Wing area | ΔS / S_max — zero at or below area limit | 0 |
| Wing loading | ΔWS / WS_min — zero at or above WS floor | 0 |
| Alpha | (Δα)² × w — only outside window [α_min, α_max] = [0°, 10°] | 0 |

### Tunable weights (ObjectiveWeights dataclass)

| Field | Default | Effect |
|-------|---------|--------|
| `ld_weight` | 1.0 | Multiplier on L/D reward |
| `alpha_penalty_scale` | 5.0 | Per deg² outside alpha window |
| `alpha_min_penalty_deg` | 0° | Lower alpha bound (penalise negative alpha) |
| `alpha_max_penalty_deg` | 10° | Upper alpha bound |
| `span_penalty_per_m` | 0.02 | Per metre of span |
| `cd0_penalty_scale` | 50.0 | Parasitic drag penalty |
| `area_penalty_scale` | 0.5 | Wing area soft-constraint |
| `wing_loading_penalty_scale` | 0.005 | Wing-loading floor |
| `cg_penalty_scale` | 200.0 | **Legacy, unused** — see the taper-realism note above; NOT read by the cost function |
| `cg_envelope_penalty_scale` | 400,000.0 | Graduated component of the CG-envelope violation penalty (§12/§17, additive, not an early return) |
| `cg_envelope_reward` | 5.0 | Reward applied once a candidate is confirmed CG-envelope-compliant |
| `fuel_penalty_scale` | 50.0 | Normalized fuel-deficit penalty |
| `fuel_volume_penalty_scale` | 300.0 | Penalty when the wing's Torenbeek tank-volume estimate can't physically hold the required fuel mass |
| `static_margin_penalty_scale` | 20.0 | Static-margin TARGET-tracking penalty (soft; recalibrated from an earlier 2.0 once it became the sole term for this, see above) |
| `min_hstab_area_fraction` / `min_vstab_area_fraction` | 0.15 / 0.07 | Minimum tail-area fraction of wing area before the tail-area penalty kicks in |
| `tail_area_penalty_scale` | 150.0 | Quadratic coefficient for the tail-area-deficit penalty |
| `min_hstab_volume_coef` | 0.75 | Vh lower bound, Etkin convention (Raymer typical ≥ 0.90) |
| `max_hstab_volume_coef` | 1.25 | Vh upper bound (prevents stretched-fuselage exploit) |
| `min_vstab_volume_coef` | 0.06 | Vv lower bound |
| `max_vstab_volume_coef` | 0.13 | Vv upper bound (typical max ≈ 0.12; 0.13 allows some margin) |
| `tail_volume_penalty_scale` | 200.0 | Quadratic coefficient for volume penalties |
| `max_break_root_chord_ratio` | 0.65 | Upper bound on break_chord_m / root_chord_m before the taper-realism penalty kicks in (see above) |
| `taper_realism_penalty_scale` | 250.0 | Quadratic coefficient for the taper-realism penalty |
| `te_root_angle_penalty_scale` | 100.0 | Quadratic coefficient once the wing-root-to-break trailing edge exceeds a 90° angle with the fuselage centerline (§12) |
| `min_wing_position_fraction` | 0.27 | Minimum wing-root-LE position as a fraction of fuselage length (keeps the wing out of the cockpit) |
| `wing_position_penalty_scale` | 300.0 | Penalty weight for a wing sitting forward of the minimum position |
| `payload_shortfall_penalty_scale` | 1,000.0 | Quadratic coefficient on the fractional shortfall vs. the target passenger count / cargo payload |
| `fineness_ratio_max` | 15.0 | Maximum fuselage length/diameter ratio before the slenderness penalty kicks in |
| `fineness_ratio_penalty_scale` | 500.0 | Penalty weight for exceeding the fineness-ratio ceiling |
| `thickness_floor` | 0.90 | Minimum airfoil thickness scale before the thin-airfoil penalty kicks in |
| `thickness_penalty_scale` | 20.0 | Penalty weight for the thin-airfoil floor |
| `fuselage_floor_m` | 60.0 | Minimum fuselage length before the too-short-fuselage penalty kicks in (also the scale used for the fuselage-empty-stretch penalty) |
| `fuselage_penalty_scale` | 5.0 | Penalty weight for both the too-short-fuselage and empty-stretch terms |
| `failure_cost` | 1,000.0 | Cost for geometry/mass/stability/aero build-or-solve crashes (true early-return hard reject) |
| `instability_failure_cost` | 1,000.0 | Severity scale for the graduated static-margin-floor penalty (§12's `J_SM_floor`) — not a flat returned cost, and kept distinct from `failure_cost` so `OptimizationHistory.reject_reason_counts` can tell "crashed" apart from "physically unstable" |

All weights are user-adjustable from Advanced Settings → Optimizer & weights.
Each float weight/scale field in the GUI is rendered as a **logarithmic slider**
spanning [0.001, 1000] for intuitive per-decade tuning.

Code: `optimization/objective.py`, `config/optimizer_config.py`.

---

## 17. Weight & balance (mass analysis)

Component masses are estimated using Torenbeek empirical formulas for
CS-25/FAR-25 transport aircraft.  The physical center of gravity is computed
as the mass-weighted centroid of all components:

```
x_CG = Σ(mᵢ · xᵢ) / Σ(mᵢ)     [longitudinal CG]
```

Key weight groups:
- **OEW** (Operating Empty Weight): structure + propulsion + systems + gear
- **MZFW** (Max Zero-Fuel Weight): OEW + payload
- **MTOW** = OEW + payload + fuel
- **Fuel budget**: `m_fuel = MTOW − MZFW` (negative ⇒ design overweight)

The physical CG is used as the moment reference to compute the actual static margin. The deviation of this static margin from the target static margin drives the `cg_penalty_scale` term in the objective (soft, target-tracking).

CG envelope visualization (`figure_cg_envelope`) shows the operational trajectory of the loading sequence: OEW → load payload (to MZFW) → load fuel (to MTOW).

The limits of the envelope are drawn dynamically:
- Forward aerodynamic limit (`fwd_limit_mac`) and stability limit (`aft_limit_mac` representing Neutral Point minus target static margin).
- Nose Landing Gear (NLG) and Main Landing Gear (MLG) structural strength limits, steering nose load limits, and the aft tip-over limit.

**Enforcement in the optimizer.** The physical CG at all three loading states
(OEW, MZFW, MTOW/current) must lie within `[fwd_limit_mac, aft_limit_mac]` —
checked every evaluation (`optimization/objective.py::_check_cg_envelope`),
not merely visualized. A violation adds a large, dominant `instability_
failure_cost` (+ a graduated cubic-above-5%-exceedance term) to the cost
formula (§16) so a non-compliant candidate can essentially never out-compete
a compliant one on cost alone — but `L/D` is still computed and still
contributes, deliberately **not** an early return (see §12's note on why: an
early-return version of this check was tried first and found, empirically,
to prevent the solver from locating a compliant region that genuinely
existed in the search space, because it discarded L/D for every non-compliant
candidate). A separate, independent check similarly penalises (rather than
early-rejects) any candidate whose physical static margin (measured at the
actual CG, via §9d's cruise-condition probe) falls below `requirements.
min_physical_static_margin` — an aircraft can sit inside the CG envelope and
still be judged too close to instability by this floor if the envelope
itself is configured loosely, so the two checks are complementary, not
redundant. Both feed into `OptimizationHistory.reject_reason_counts` for
diagnostics regardless of how they affect the cost.

Code: `physics/mass.py`, `reporting/visualization.py`, `optimization/objective.py`.

---

## 17a. Detailed payload layout (cargo & passenger)

The lumped payload model above (a single mass at `cabin_payload_density_kg_m`) is used
as the **first pass** everywhere `run_mass_analysis` runs (optimizer, baseline, final),
purely to get an OEW/x_oew estimate cheaply. The **optimizer**, the **baseline** pass,
and the **final** pass then all build a physically detailed layout
(`physics/payload.py::build_payload_layout`) from that estimate, and its true payload
mass and CG override the lumped Payload component via the optional `payload_layout=`
argument of a second `run_mass_analysis` call:

```
masses["Payload"] = Σ_i m_i           (mass-bearing items i)
x_payload         = (Σ_i m_i x_i) / Σ_i m_i
m_fuel            = MTOW − (OEW + masses["Payload"])
```

The optimizer only skipped this second pass in earlier versions (using the first-pass
lumped Payload CG directly for its CG-envelope check). That was a real bug, not a speed
optimization: `x_payload` under the lumped model is the geometric centre of the occupied
cabin length, while the detailed model's `x_payload` is the true mass-weighted centre of
actual seats/galleys/lavs/bags — measured to differ by 3.5-3.9% MAC across sample
designs, consistently in the same direction. A candidate the optimizer scored as
CG-compliant under the lumped estimate could therefore still land outside the envelope
once the final analysis recomputed it with the real layout — the exact mismatch a user
spotted from the CG-envelope plot. The second `run_mass_analysis` pass costs one extra
`build_payload_layout` call per candidate (~30 ms, measured — cabin/cargo layout
construction is pure Python/NumPy, no VLM solves), a worthwhile tradeoff since it makes
the optimizer's constraint the same number the plot actually shows.

Deck geometry (`CabinGeometry`) is sampled from the built fuselage: the usable floor
width at a station is `w_usable(x) = (width(x) − 2·t_wall) · k_deck`, where `k_deck`
accounts for a deck sitting off the section centre. A body with
`height_m ≥ 1.15·diameter_m` is treated as **double-deck** (A380): a main and an upper
passenger deck plus the lower hold; otherwise one main deck plus the lower hold.

### Cargo — ULD load solver (`cargo_loader.py`)

Cargo slots are generated on the main deck (if `use_main_deck`) and the forward/aft
lower-deck holds (excluding the wing-box span), each holding a ULD from `ULD_DATABASE`
(LD3 1.56×1.53 m, PMC 3.18×2.44 m, bulk). The required *payload* CG to put the whole
aircraft at the target CG follows from a two-mass balance:

```
x_payload,req = ((OEW + P)·x_target − OEW·x_OEW) / P          (P = cargo payload)
x_target      = LEMAC + (target_cg_pct_mac/100)·c̄
```

The solver (ported from `cargo_manager_example.py`) fills slots by a priority order,
then iteratively conserves total mass and shifts load from the heavy side toward the
light side until `|x_cg − x_payload,req| < 0.05 m`. Priorities: `target_cg` /
`min_pallets` (concentrate near `x_payload,req`), `door_proximity` (nearest the cargo
doors), `uniform`.

### Passenger — cabin layout engine (`cabin_layout.py`)

Seats are packed front→aft by class (First→Business→Premium→Economy). Seats-abreast is
taken from the local floor width:

```
n_aisles = 1 if w_usable < 3.6 m else 2
abreast  = floor( (w_usable − n_aisles·w_aisle) / w_seat )
```

Galleys (~1 per 100 pax + 1) and lavatories (~1 per 45 pax) are placed in monument bays
at the cabin ends, class boundaries, and mid-cabin door stations. Checked baggage
(`checked_bag_mass_kg` per pax) is routed to the forward/aft lower holds and split so
the bag CG matches the seating CG (airlines trim bags), making the payload CG driven by
the seating distribution.

**Monument distribution (`_monument_fill_order`).** Bays are visited front, rear, then
inward — `[0, n-1, 1, n-2, 2, ...]` — not front-to-back in list order, and this order is
*cycled* (not just truncated) when the galley/lav count exceeds the bay count. This
matches how real LOPAs work: a single monument goes at the front; two go at the front
and rear; extra ones (common on widebodies, where lavatory count routinely exceeds the
exit-derived bay count) pile up at the front/rear complexes first, only reaching into
mid-cabin bays once those are exhausted — never a plain front-to-back fill that leaves
the tail bare whenever the count is below the bay count (the previous behaviour).
Multiple monuments sharing one bay pack back-to-back from the wall and are *narrowed*
(never repositioned into an overlap) if the bay runs out of room before the far side is
reached.

**Emergency exits reuse the monument bays' own x-positions**, selected via
`_spread_bay_indices(n_pairs, n_bays)` — genuinely even spacing for the *specific* count
needed (unlike the cyclic monument order, which is only well-spread as a growing prefix,
not for an arbitrary fixed count) — so an exit always lands on the `_MONUMENT_LEN` gap
already carved out of the seating there, instead of an independently-computed position
that could drift out of alignment with where the seat rows actually leave room.

**Maximum certifiable capacity (`max_certifiable_capacity`).** Seats are packed
front-to-aft *only up to* a per-deck capacity ceiling derived from how many
emergency-exit pairs could realistically be installed — real aircraft are
exit-limited, not floor-space-limited, so filling the entire cabin length
with seats regardless of exits is physically wrong (a single-class max-density
A380 layout would otherwise seat 1400+, far past the real type's
853-passenger certified maximum). Per passenger deck:

```
n_pairs_max = clamp(1, 6, floor(deck_length / min_exit_pair_spacing_m))
deck_cap    = n_pairs_max · cap_side · 2          (cap_side derated for Type A only, see below)
```

`min_exit_pair_spacing_m` (default 10 m) approximates realistic door-pair
spacing on real aircraft. Both `cabin_layout.build_passenger_layout` (the
detailed, run-once layout) and `payload.simulate_passenger_counts` (the fast
preset auto-sizer feeding `requirements.num_passengers` for the optimizer's
lumped model) share this same ceiling, so the two stay consistent.

**Emergency exits (FAR/CS-25.807(g)).** The exit type is chosen from the fuselage size
(widebody → Type A, narrowbody → Type C/III) and the number of exit pairs *actually
installed*, per deck, for the (now capacity-capped) seated passenger count is

```
n_pairs = max(n_min, ⌈ deck_pax / cap_side ⌉),   n_min = 2 if deck_pax > 110 else 1
```

with the FAR/CS-25.807(g) maximum seats per exit, per side: Type A 110, B 75, C 55,
I 45, II 40, III 35, IV 9 — applied literally (undiscounted) here, since this formula
answers "how many exits does the regulation require for this many seated passengers,"
not "what's the realistic overall ceiling" (that's `max_certifiable_capacity` above).
A double-deck aircraft (A380) gets its own exit pairs on **each** passenger deck, not
just the main deck.

**Type-A capacity derating (`exit_capacity_realism_factor`, default 0.5).** Real
certified capacities come from a full 90-second evacuation demonstration, not simply
the sum of each door's individual FAR/CS-25.807(g) maximum rating. For widebodies with
several large Type-A doors, cross-aisle congestion over long aisles means the
demonstrated capacity comes in well below the naive sum (e.g. the 787-9's 4 Type-A
pairs would naively support 880, but its real exit limit is 420). Smaller exit types
(fewer, smaller doors, shorter aisles — narrowbodies) were found, by calibrating
against real published figures, to track close to their nominal rating in practice, so
only Type A is derated in `max_certifiable_capacity`. Calibrated against real
exit-limit/high-density capacities: 787-9 ≈420, A340-300 ≈375, A380-800 ≈853 (Type A,
derated), A320-200 ≈185, A220-300 ≈150 (Type C/III, undiscounted) — all six land within
roughly ±20% of ALAS's computed ceiling, an acceptable margin for a preliminary-design
approximation, not a certified evacuation-demonstration model.

Code: `physics/payload.py`, `physics/cargo_loader.py`, `physics/cabin_layout.py`,
`config/cabin_config.py`. Visualised by `reporting/visualization.py`
(`figure_cabin_payload`, `figure_cabin_payload_3d`).

---

## 17b. Baseline pass (mass & stability first)

Before optimization, `DesignPipeline._baseline_analysis` evaluates the *initial* design:
build (engines on) → `run_mass_analysis` (to find physical CG) → neutral point calculation (to find static margin) → detailed payload layout
→ `run_mass_analysis` with that layout → two-point static margin (§9, no full sweep).
The result (`BaselineReport`) is shown as the first Results tab and via the
"Analyze baseline" button, letting the user confirm a preset has a sensible CG and
static margin — i.e. that the mass model and payload distribution are correct — before
any optimization. It is deliberately cheap (no alpha sweep). Code: `pipeline.py`.

---

## 19. Route geometry & mass/altitude synchronisation (`routing/`, `reporting/route_globe.py`)

**Great-circle distance (haversine).** Between two airports at
`(lat1, lon1)`, `(lat2, lon2)` (degrees):

```
a = sin²(Δlat/2) + cos(lat1)·cos(lat2)·sin²(Δlon/2)
d = 2·R_earth·asin(√a)                      R_earth = 6,371,000 m
```

**Great-circle interpolation (slerp).** Intermediate waypoints at fraction
`f ∈ [0,1]` along the arc of angular length `d` (`route.py:_slerp`):

```
A = sin((1-f)·d)/sin(d),  B = sin(f·d)/sin(d)
x = A·cos(lat1)cos(lon1) + B·cos(lat2)cos(lon2)
y = A·cos(lat1)sin(lon1) + B·cos(lat2)sin(lon2)
z = A·sin(lat1) + B·sin(lat2)
lat = atan2(z, √(x²+y²)),  lon = atan2(y, x)
```

**Open-navdata airway routing.** `navdata_graph.py` builds a weighted graph
(nodes = enroute fixes, edge weight = haversine distance between fix pairs
sharing an airway) and runs Dijkstra between the fixes nearest the departure
and arrival airports — a standard shortest-path formulation, not an
aerodynamic/physics model.

**Mass/altitude synchronisation onto a route.** Direct port of
`route_globe.m`'s approach: integrate true airspeed over time (trapezoidal
rule) to get cumulative flown distance, normalise it to the route's total
great-circle/airway length, keep only strictly-increasing distance samples
(drops near-stationary points), then linearly interpolate (`np.interp`) mass
and altitude from the SUAVE mission CSV onto each route waypoint's distance
along the path (`route_globe.py:sync_mass_to_route`).

**Globe rendering.** Points are placed on/above a sphere of radius
`R_earth = 6,371 km` at `r = R_earth + altitude_km + 30 km` (the +30 km is a
fixed visibility offset above the surface, matching `route_globe.m`), then
converted from (lat, lon, r) to Cartesian via the standard spherical→Cartesian
transform. Not a physics model — pure geometry for visualization.

## 20. SUAVE turbofan cycle parameters (`config/engines.py`, `vehicle_builder.py`)

SUAVE's `Turbofan` network needs per-stage pressure ratios; `EngineSpec` only
stores the figures that are realistically known per engine
(`overall_pressure_ratio`, publicly published for most; `fan_pressure_ratio`
and `turbine_inlet_temp_k`, engineering estimates). The low-pressure
compressor ratio is fixed at a generic value (matches `suave_example.py`'s
original GE9X assumption) and the high-pressure compressor ratio is solved to
hit the published overall pressure ratio:

```
HPC_pressure_ratio = overall_pressure_ratio / LPC_pressure_ratio     (LPC fixed at 1.20)
```

All other component polytropic/mechanical efficiencies (inlet 0.98, fan 0.93,
compressors 0.91/0.93, turbines 0.95, mechanical 0.99) are shared across every
engine — standard Mattingly-level textbook assumptions, not engine-specific
data, applied uniformly the same way `suave_example.py` applied them to GE9X
alone.

`EngineSpec` also carries `cruise_tsfc_kg_kgf_hr` (engineering estimate per
engine, same disclaimer as above) — used only by the Breguet payload-range
diagram (§15i), not by the SUAVE mission bridge itself (SUAVE derives fuel
burn from the physics-based turbofan network above, not a flat TSFC constant).

### 20a. Turbofan sizing reference point (`thrust.total_design`) — a real bug and its fix

SUAVE's `turbofan_sizing(turbofan, mach_number, altitude)` sizes the
network's reference mass flow/area such that, **at the given (mach_number,
altitude) and throttle=1.0**, the network's own solved cycle reproduces
`thrust.total_design` exactly. That reference condition and that thrust
value must describe the *same* operating point — `total_design` is NOT
necessarily the engine's sea-level-static rated thrust unless the sizing
call also uses sea-level-static conditions. Confirmed against SUAVE's own
bundled `regression/scripts/Vehicles/Boeing_737.py`: it sizes
`total_design = 2*24000 N` (24 kN/engine) at `altitude=35,000 ft,
mach_number=0.78` — i.e. the actual **cruise thrust required**, nowhere
near the CFM56's real ~120 kN sea-level-static rating.

`vehicle_builder.py` calls `turbofan_sizing(turbofan, cruise_mach,
cruise_altitude)` (a cruise reference point), so `total_design` must be the
cruise thrust, not the static rating: feeding `EngineConfig.thrust_kn` (the
static rating, §20 above) straight in here would tell SUAVE the engine can
produce its full static thrust AT CRUISE ALTITUDE too, oversizing the sized
engine by roughly the real static-to-cruise thrust lapse ratio and driving
every mission segment's solved throttle far too low (a flat ~0.2 throughout
cruise, versus the expected ~0.7-1.0 for a reasonably-sized engine at its
cruise design point). `integration/suave_vehicle.py`'s
`_cruise_thrust_kn_per_engine` instead computes per-engine cruise thrust
directly from the aircraft's own trimmed cruise condition,

```
thrust_required_N = MTOW_kg * g / (L/D)_trimmed          (steady level flight: thrust = drag = weight/(L/D))
cruise_thrust_kn_per_engine = thrust_required_N / n_engines / 1000
```

and threaded through the vehicle-request dict as `cruise_thrust_kn`, which
`vehicle_builder.py` uses for `total_design` (falling back to the static
rating only if no trimmed design point exists yet). Deliberately NOT routed
through `physics/propulsion.py`'s on-design cycle's own cruise-point
estimate (§20c) — that module's own docstring flags it as a different,
not-independently-validated fidelity level from SUAVE's numerical solve, so
using it here would size SUAVE's engine against a second approximate model
instead of this aircraft's own already-computed aerodynamics. Verified on a
real mission run: cruise throttle went from a flat ~0.2 to ~0.92-1.0 at
cruise start, easing down as fuel burns.

### 20b. Mission-output derived quantities (`export_data.py`)

Two columns are derived rather than read directly from SUAVE's
`segment.conditions`, mirroring the same derivations SUAVE's own
`Mission_Plots.py` uses internally for its `plot_aircraft_velocities`/
`plot_altitude_sfc_weight` panels:

```
EAS [m/s] = TAS · sqrt(density / 1.225)                         (equivalent airspeed, ISA SL reference)
SFC [kg/(kgf·hr)] = (mass_flow_rate · 3600) / (thrust / g)      (thrust-specific fuel consumption)
```

`SFC`'s unit (kg fuel per kgf-thrust per hour) is numerically the same ratio
as SUAVE's own imperial lb/lbf/hr (both are mass-flow-per-unit-weight-force),
via `kgf = thrust_N / g`.

## 20c. On-design turbofan cycle analysis (`physics/propulsion.py`)

A separate-flow (unmixed), two-spool parametric cycle model for the
Propulsion Analysis Results tab / Engine Designer preview -- reuses the
*same* component efficiencies as §20 above
(`config/propulsion_config.py::PropulsionCycleConfig`, defaults identical to
`vehicle_builder.py`'s hardcoded numbers) so this fast, closed-form estimate
and SUAVE's numerically-solved mission engine start from the same
component-level physics, while remaining a genuinely different fidelity
level (see the Model Comparison tab's AeroSandbox/SUAVE/MSES precedent for
the same idea). All temperatures below are stagnation ("total") temperatures.

**Freestream / inlet** (`T0, p0, a0` from `asb.Atmosphere`, `M0` = flight Mach):

```
T_t0 = T0 · (1 + (gamma_c-1)/2 · M0²)
p_t0 = p0 · (1 + (gamma_c-1)/2 · M0²)^(gamma_c/(gamma_c-1)) · pi_inlet
```

**Fan branch** (parallel to the core compressors, off the same `T_t0, p_t0`
-- matching SUAVE's turbofan topology, where the fan is its own component,
not chained after the LPC):

```
T_t13 = T_t0 · pi_f^((gamma_c-1)/(gamma_c·eta_fan))          [pi_f = fan_pressure_ratio]
p_t13 = p_t0 · pi_f · pi_fan_nozzle
```

**Core compressors** (LPC then HPC; `pi_c` = overall/core pressure ratio =
`EngineConfig.overall_pressure_ratio`, split exactly as in §20):

```
pi_LPC = lpc_pressure_ratio_split (fixed, default 1.20)
pi_HPC = pi_c / pi_LPC
T_t25  = T_t0  · pi_LPC^((gamma_c-1)/(gamma_c·eta_LPC))
T_t3   = T_t25 · pi_HPC^((gamma_c-1)/(gamma_c·eta_HPC))
p_t3   = p_t0 · pi_LPC · pi_HPC
```

**Combustor** (`T_t4` = `EngineConfig.turbine_inlet_temp_k`, the design input;
`f` = fuel-air ratio, standard Mattingly-style energy balance):

```
p_t4 = p_t3 · pi_burner
f = (cp_hot·T_t4 - cp_cold·T_t3) / (eta_burner·h_PR - cp_hot·T_t4)
```
Infeasible (no physical solution) if `T_t4 <= T_t3` or the denominator `<= 0`.

**HPT** (drives the HPC only) **and LPT** (drives the LPC *and*, scaled by
bypass ratio `B`, the fan -- the defining trait of a turbofan: most of the
low-pressure spool's work goes into the fan, not the core):

```
Delta_T_HPT = cp_cold·(T_t3 - T_t25) / ((1+f)·cp_hot·eta_mech)
T_t45 = T_t4 - Delta_T_HPT
p_t45 = p_t4 · (T_t45/T_t4)^(gamma_h/((gamma_h-1)·eta_HPT))

Delta_T_LPT = [cp_cold·(T_t25-T_t0) + B·cp_cold·(T_t13-T_t0)] / ((1+f)·cp_hot·eta_mech)
T_t5 = T_t45 - Delta_T_LPT
p_t5 = p_t45 · (T_t5/T_t45)^(gamma_h/((gamma_h-1)·eta_LPT))
```
Infeasible if either temperature drop would take `T_t45` or `T_t5` <= 0.

**Nozzles** (core, hot; fan, cold) -- ideal expansion to ambient `p0` unless
choked, in which case the exit is sonic at the throat's critical pressure:

```
p* / p_t = (2/(gamma+1))^(gamma/(gamma-1))                    [critical pressure ratio]
p_exit = p*  if p* >= p0  (choked),  else  p0
T_exit = T_t - eta_nozzle·(T_t - T_t·(p_exit/p_t)^((gamma-1)/gamma))
V_exit = sqrt(gamma·R·T_exit)   if choked,   else   sqrt(2·cp·(T_t - T_exit))
```

**Thrust, specific thrust, TSFC.** The pressure-thrust term of a choked
nozzle is folded into an *equivalent* exit velocity,
`V_eq = V_exit + (p_exit - p0)/(rho_exit·V_exit)`, which reproduces the
momentum-plus-pressure thrust equation exactly (`(1+f)·(V9_eq - V0) ==
(1+f)·(V9-V0) + (1+f)·(p9-p0)/(rho9·V9)`) while also being what the
efficiency energy balance below needs:

```
F/mdot_core = (1+f)·(V9_eq - V0) + B·(V19_eq - V0)
SFn [m/s]   = (F/mdot_core) / (1+B)             (specific thrust per unit TOTAL mass flow)
TSFC        = f / (F/mdot_core)                  [kg/(N.s)], reported in mg/(N.s)
```

**Efficiencies** (thermal x propulsive = overall, using the *same* equivalent
velocities -- required for a choked nozzle's energy balance to stay
consistent with its thrust equation; using the bare exit velocities instead
can silently push `eta_p` above 1, an early bug this exact fix corrected):

```
KE_gain    = (1+f)·(V9_eq² - V0²)/2 + B·(V19_eq² - V0²)/2
eta_th     = KE_gain / (f · h_PR)
eta_p      = (F/mdot_core · V0) / KE_gain          (0 by definition when V0 = 0)
eta_o      = (F/mdot_core · V0) / (f · h_PR)  ==  eta_th · eta_p   (exact identity)
```

**Mass-flow anchor** (`anchor_mass_flow_kg_s`) -- lets the tab show
dimensional thrust, not just specific thrust, without a real corrected-flow/
face-area schedule: the total design mass flow is defined so the cycle's
*static* (`M0=0, h=0`) specific thrust reproduces `EngineConfig.thrust_kn`
exactly, `mdot_total = thrust_kn·1000 / SFn_static`; dimensional thrust at
any other condition is then `SFn(condition) · mdot_total`. A conceptual-
design-level normalisation, not an independent validation -- clearly
captioned as such on the Cycle Summary figure.

Validated against all 7 registered engines at their published cruise design
point (`0 <= eta_th, eta_p, eta_o <= 1` for every one, the `eta_th·eta_p ==
eta_o` identity holding to floating-point precision, and computed TSFC
running a systematic ~20-25% above each engine's published reference value
-- expected for generic assumed component efficiencies vs. real, further-
optimized hardware, not a correctness bug).

Code: `physics/propulsion.py`, `config/propulsion_config.py`.

---

## 20d. Structural analysis (wingbox FEM, `physics/structural_sizing.py`,
`physics/structural_analysis.py`, `physics/structural_loads.py`)

Generalizes `Reference Scripts/00_sizing.py`/`05_validation.py` to an
arbitrary number of spars and any ALAS design. Downstream/
informational only — never feeds back into §17's mass model or the
optimizer.

**Load cases** (`structural_loads.load_cases`). Reuses `DesignRequirements`
directly, not new fields:

```
n_ult_pos = ultimate_load_factor * additional_safety_factor        (pull-up)
n_ult_neg = limit_load_factor_neg * 1.5 * additional_safety_factor  (push-down)
n_level   = 1.0 * additional_safety_factor
F_total   = n * mtow_kg * g / 2       (signed total aero force, per semi-wing)
```

identical `n_ult_pos`/`n_ult_neg` derivation to §15f's V-n diagram, so the
structural loads always match the V-n diagram shown elsewhere.

**Spanwise load distribution** (elliptic, classic preliminary-design
simplification, matching the reference's own validated approach):

```
q(y) = q0 * sqrt(1 - (y/b_semi)^2),   q0 = 4*F_total / (pi*b_semi)
```

**Shear/moment** via cantilever (free tip, fixed root) cumulative
trapezoidal integration of a net distributed load `q_net(y)`:

```
V(y) = ∫_y^{b_semi} q_net(y') dy'         (sizing: q_net = q_aero, no relief)
M(y) = ∫_y^{b_semi} V(y') dy'              (analysis: q_net = q_aero - n*g*m'(y), + relief)
```

where `m'(y)` is the sized structure's own mass per unit length (skin +
spar caps + spar webs), plus a point-load correction at each wing-mounted
engine station `y_eng`: `M(y) += -n*m_eng*g*(y_eng - y)` for `y <= y_eng`.

**Direct strength cap sizing** (margin of safety = 0 by construction at the
root, for the governing load case) — moment/shear split across N spars
weighted by local section depth `H_i(y) / sum_j H_j(y)`:

```
A_cap,i = (frac_i * |M(0)|) / (F_allow,cap * 0.85*H_i(0))
w_cap,i = min(0.5*c(0), 0.6*H_i(0)),   t_cap,i = min(A_cap,i / w_cap,i, 0.20*H_i(0))
t_web,i = max(t_web_min, (frac_i * |V(0)|) / (tau_allow,web * 0.85*H_i(0))),   tau_allow = F_allow,web / (2*sqrt(3))
```

Cap taper law (a kept manufacturing convention, not re-derived): full root
section up to `eta_lock`, linear taper to `tip_fraction` at the tip,
re-clamped every station to `t_cap <= H(y)/3` and `w_cap >= t_cap`.

**Optional partial-span center spar** (`StructuresConfig.
center_spar_enabled`, a widebody-style root-to-kink reinforcement spar at
`center_spar_chord_fraction`, default 0.50): sized by the exact same
formulas above, except its local section depth `H_i(y)` is forced to 0 for
`y > y_break` *before* the `H_i(y)/sum_j H_j(y)` moment-share weighting is
computed. This single substitution is sufficient on its own: the spar's
`frac_i` (and hence `A_cap,i`, `t_web,i`, mass, and margin-of-safety) all
zero out past the break automatically, with the front/rear spars picking
up the moment share it no longer carries there — no other equation above
needs a special case for it. Verified this can *reduce* total wingbox mass
overall, since the front/rear spars' reduced root moment share shrinks
their cap area over their *entire* span (via the same taper law), while
the center spar's own added material only spans the inboard fraction of
the wing.

**Rib spacing** (Euler panel-buckling criterion, generalized from the
reference's fixed front/rear box to the outermost two spars — verified to
match the reference's own already-fixed formula exactly, and confirmed
`num_ribs` here is the *same value* `geometry/wing_mesh_bdf.py` meshes,
not an independently-recomputed one):

```
N_x = (|M(0)| / H_mid(0)) / b_box(0)         (running compressive load, skin between outer spars)
sig_panel = N_x / t_skin
L_rib = rho_gyro * sqrt(c_buckling * pi^2 * E_skin / sig_panel),   floor 0.5 m
num_ribs = max(10, ceil(b_semi / L_rib) + 1)   unless overridden
```

`t_skin` is a fixed constant (`t_skin_min_m`) regardless of aircraft size
or actual shear/torsion load — no shear-flow or panel-buckling-based skin
sizing exists. Investigated directly as a candidate cause of a real
FEM-vs-real-aircraft stiffness/mass mismatch (A320-200: FEM wingbox ran
~14% over the Torenbeek mass estimate and noticeably under-deflected vs. a
reported real ultimate-load wingtip flex): sweeping `t_skin_min_m` 6mm-
>1mm dropped semi-wing mass 3,824->2,265 kg and raised ultimate tip
deflection 3.13->4.18 m — confirms skin thickness is a real, measurable
contributor to both effects, but not sufficient alone to close the full
gap even at an unrealistic thickness. A real shear/torsion-buckling-based
skin sizing model is the actual fix; not implemented (see `docs/
architecture.md` §11f).

**Bending stiffness** `EI(y)` (spar caps as symmetric I-sections + skin
torsion-box parallel-axis contribution):

```
I_cap,i(y) = bf*H^3/12                      if H <= 2*tf
           = tw*(H-2tf)^3/12 + 2*bf*tf*((H-tf)/2)^2   otherwise   (tw≈0, cap-only)
I_skin(y)  = 2 * b_box(y) * t_skin * (mean_i(H_i(y))/2)^2
EI(y) = E_cap * sum_i I_cap,i(y)  +  E_skin * I_skin(y)
```

**Method A/B — Castigliano tip deflection / Euler-Bernoulli spanwise curve**
(unit-load / virtual-work theorem, direct port of the reference's own
validated approach, <20% error vs. real NASTRAN there):

```
delta(s) = ∫_0^s  M(y)*m_bar(y) / EI(y)  dy,      m_bar(y) = s - y  (for y <= s, else 0)
delta_tip = delta(b_semi)
```

Known fidelity gap (shared with the reference, documented not silently
assumed away): EI includes only spar caps + skin torsion-box contribution
(not the closed-box coupled GJ/torsion), and ribs redistributing load
spanwise aren't modeled in this 1-D beam idealization.

**Method C — Rayleigh quotient natural frequencies** (classical cantilever
trial mode shapes, first 4 modes, `beta_i*L` = 1.8751/4.6941/7.8548/10.9955):

```
phi_i(y) = cosh(beta_i*y) - cos(beta_i*y) - sigma_i*(sinh(beta_i*y) - sin(beta_i*y))
f_i = (1/2*pi) * sqrt( ∫ EI*phi_i''^2 dy  /  ∫ m'(y)*phi_i^2 dy )
```

with each wing-mounted engine's mass smeared onto its nearest spanwise
station for the modal mass integral.

**Matching a Rayleigh mode to its real NASTRAN counterpart**
(`nastran_runner._read_modes`/`visualization._nearest_freq_index`): a real
SOL 103 solve requests `n_modes` (default 30) eigenvalues, well beyond the
handful of global-bending modes the 4-entry cantilever trial-shape table
above approximates — the rest are torsion/local-panel modes with no
Rayleigh equivalent, interleaved in frequency order. Pairing "Rayleigh
mode `i`" with NASTRAN's `i`-th frequency by raw list position therefore
routinely compares unrelated modes (confirmed on real data: NASTRAN's own
filtered list started `[0.76, 1.40, 2.38, 7.43, ...]` Hz against Rayleigh's
`[2.96, 7.60, 38.01, 38.11]` Hz — an obviously wrong match at every
position). Matched instead by nearest frequency, independently per
Rayleigh mode, against NASTRAN's full filtered (`f > 0.5` Hz, discarding
near-rigid-body modes) list:

```
match(i) = argmin_j | f_rayleigh,i - f_nastran,j |
```

The matched NASTRAN mode's own front-spar T3 (out-of-plane) displacement
shape, normalized to peak=1, is extracted alongside its frequency so the
Normal Modes results tab can plot it next to the Rayleigh trial shape for
the same mode index.

**Method D — Miles equation** (NASTRAN-only, no analytical fallback: needs
a real sine-sweep frequency-response function `|H(f)|` as input):

```
RMS = |H_peak| * sqrt(pi * f_peak * S_F / (4*zeta))
```

where `S_F` is the applied acceleration PSD (`psd_base_g2_per_hz * g^2`)
and `zeta` is `modal_damping_ratio`.

## 21. References

- D. P. Raymer, *Aircraft Design: A Conceptual Approach* — component drag buildup,
  form factors, skin friction, matching chart / field-length empirical formulas
  (Ch. 17 & 21).
- W. H. Mason, *Configuration Aerodynamics* — Korn equation / drag-divergence.
- J. D. Anderson, *Fundamentals of Aerodynamics* — VLM, induced drag, Oswald
  efficiency.
- AeroSandbox documentation — VLM, geometry, and atmosphere implementations.
- FAR Part 25 / CS-25 — V speed definitions (§25.103, §25.107, §25.121, §25.125,
  §25.149); emergency-exit types & passenger limits (§25.807); exit access / aisle
  width (§25.813, §25.815).
- Stanford AA241 "Cabin Layout and Fuselage Geometry" — seats-abreast vs. cabin
  width, seat pitch/width by class, galley/lavatory provisioning ratios.
- E. Torenbeek, *Synthesis of Subsonic Airplane Design* — Appendix C: component
  mass estimation formulas.
- J. D. Mattingly, *Elements of Gas Turbine Propulsion* — turbofan cycle analysis,
  component polytropic efficiency baselines (§20).
- SUAVE 2.5.2 (Stanford Aerospace Design Lab) — mission segment solver,
  turbofan energy network, `suave_example.py`'s own vehicle/mission assumptions.
