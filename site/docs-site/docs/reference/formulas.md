# Formulas & theory

The narrative chapters show you *what* ALAS computed for AVE; this
page is *how*, with AVE's own numbers worked through each formula as a
concrete check. It's organized in the order a design actually flows
through the pipeline.

## Cruise design point

The required lift coefficient at any flight condition follows directly
from the lift equation, solved for CL:

$$
C_L = \frac{W}{q \, S}, \qquad q = \tfrac{1}{2} \rho V^2
$$

At AVE's cruise point (ISA density $\rho \approx 0.316\ \text{kg/m}^3$ at
11,887 m, $V \approx 248\ \text{m/s}$ at M0.84), $q \approx 9{,}700$ Pa. With
$S = 531.4\ \text{m}^2$ (the optimized AVE), the level-flight lift coefficient
at MTOW (358,670 kg, $W \approx 3.52 \times 10^6$ N) is $C_L \approx 0.68$. The
CL that [Aerodynamic analysis](../aerodynamic-analysis.md) reports is evaluated
at the **mid-cruise mass**, the takeoff mass less half the cruise fuel
($358{,}670 - 0.5 \times 144{,}969 \approx 286{,}200$ kg), which gives
$C_L \approx 0.543$ (`alas-pipeline`, `cruise_mass.rs`).

## Inviscid aerodynamics: vortex-lattice method

The wing (and tail, and fuselage) are discretized into a lattice of
horseshoe vortices; requiring flow tangency at each panel's control point
gives a linear system for the vortex strengths, from which lift and
**induced** drag fall out directly from the Trefftz-plane analysis: no
empirical correction needed for this part. VLM is inviscid and
linearized-subsonic by construction, which is exactly why two more terms
are needed before the drag polar is trustworthy at a real transonic cruise
condition.

## Parasite drag: Raymer component buildup

Viscous (skin-friction) drag is estimated per component from a turbulent
flat-plate friction coefficient, corrected for compressibility and
geometry:

$$
C_{f} = \frac{0.455}{(\log_{10} Re)^{2.58} \, (1 + 0.144 M^2)^{0.65}}
$$

This is the compressible Prandtl–Schlichting relation. Each component's
contribution to $C_{D0}$ is then $C_f \times FF \times Q \times
S_{wet}/S_{ref}$, where $FF$ is a form factor (accounting for pressure
drag from thickness/shape: different formulas for wing sections vs. the
fuselage), $Q$ is an interference factor (nacelle-wing interference, for
instance), and $S_{wet}$ is that component's wetted area. Summed across
wing, fuselage, empennage, and nacelles, this is where AVE's
$C_{D0} = 0.0152$ at the design point comes from (the quadratic polar fit
through the whole sweep has a larger intercept, 0.0211).

## Transonic wave drag: the Korn equation

Above the drag-divergence Mach number, compressibility drag rises sharply;
VLM alone has no way to predict this. ALAS uses the Korn equation,
a widely-used empirical closure relating wave drag to thickness ratio,
sweep, and CL:

$$
M_{dd} = \frac{\kappa_A}{\cos\Lambda} - \frac{t/c}{\cos^2\Lambda} - \frac{C_L}{10\cos^3\Lambda}
$$

$$
C_{D,wave} = \begin{cases} 0 & M \le M_{dd} \\ 20(M - M_{dd})^4 & M > M_{dd} \end{cases}
$$

where $\kappa_A$ is an airfoil-technology factor (supercritical sections
like AVE's SC(2)-0714 get a higher value than conventional sections,
reflecting their higher drag-divergence Mach for the same thickness) and
$\Lambda$ is sweep. This is the term that makes sweep and thickness real
trade-offs in [Design space & optimizer](../design-space-and-optimizer.md)
rather than free variables: thin, swept wings buy drag-divergence margin
at a structural-weight cost the mass model then has to account for.

## Induced drag and Oswald efficiency

$$
C_{D,i} = \frac{C_L^2}{\pi \, AR \, e}
$$

$e$ (Oswald efficiency: 0.83 for AVE from the induced term of the drag
breakdown, 0.60 from the polar fit, which also absorbs CL-dependent viscous
drag) is derived from the VLM
solution's actual spanwise lift distribution rather than assumed: a
distribution close to elliptical (the theoretical minimum-induced-drag
case) gives $e$ close to 1.0; AVE's twist, taper, and sweep together push
it slightly below that ideal, which is normal for a real swept wing with
practical taper (a pure elliptical planform is rarely structurally or
manufacturing-sensible).

## Static margin and the neutral point

The neutral point is where the aircraft's total pitching-moment
coefficient becomes independent of angle of attack: physically, the
point where lift could be applied without changing trim. Static margin is
its distance from the CG, normalized by MAC:

$$
SM = \frac{x_{NP} - x_{CG}}{\overline{c}}
$$

The optimized AVE: $x_{NP} = 65.3\%\,MAC$, $x_{CG} = 22.7\%\,MAC$,
$\overline{c} = 9.72\ \text{m}$, giving $SM \approx 43\%$, a known residual of the
model rather than a realistic margin; see
[Weight, balance & stability](../weight-balance-and-stability.md#static-margin-and-the-neutral-point)
for the full picture including the CG envelope this constrains.

## Mass: the FLOPS transport weight equations

Component masses (wing, fuselage, tails, gear, nacelles, installed propulsion,
systems, furnishings, operating items) come from the NASA FLOPS
conventional-transport weight equations (Wells, Horvath and McCullers,
NASA/TM-2017-219627 Vol. I, with the 2018 errata): regression-based
equations, each a function of the relevant sizing loads and geometry (wing mass
scales with span, taper, sweep, area and the design gross mass at the ultimate
load factor; fuselage mass with length, width and depth; and so on),
calibrated by NASA against real transport aircraft. Registered aircraft can
use declared cabin-equipment and pylon methods, and turboprops a declared
shaft-power installation model; each resolved method is recorded in the mass
ledger. The older Torenbeek and fraction-based groups remain only as an
explicit comparison architecture and are never mixed with the FLOPS groups.

The equations are evaluated at an explicit design gross mass and design
landing mass: a registered aircraft keeps its declared MTOW and certified
landing mass under any mission, and a clean-sheet design couples to its closed
takeoff mass. Evaluating the published equations the way FLOPS does is a
numerical-parity statement; it is not a validation against a weighed aircraft,
and nothing in the model is calibrated against one. This is also what makes
the independent FEM cross-check in
[Structural analysis](../structural-analysis.md#mass-estimates-of-different-scope)
meaningful: two unrelated methods agreeing is evidence, not a circular check.

## The V-n diagram

Maneuvering speed is where the positive limit load factor intersects the
stall boundary:

$$
n_{lim} = \frac{ultimate\_load\_factor}{1.5}, \qquad V_A = V_S\sqrt{n_{lim}}
$$

(CS-25.303's 1.5 ultimate-to-limit safety factor, applied in reverse to
recover the limit load from ALAS's `ultimate_load_factor` input.) For
AVE, $n_{lim} = 3.75/1.5 = 2.5$, and with $V_S \approx 165\ \text{kt}$,
$V_A \approx 261\ \text{kt}$, matching
[Structural analysis](../structural-analysis.md#the-v-n-diagram)'s figure
exactly. Design cruise speed $V_C$ is derived as $V_D / 1.25$
(CS-25.335(b)'s minimum required margin), rather than carried as a
separate input.

## Turbofan on-design cycle

The core cycle is a straightforward Brayton-cycle station analysis
(freestream → inlet → fan/compressor → combustor → turbine → nozzle), with
each station's stagnation temperature and pressure computed from the
previous one via isentropic relations and a component efficiency:

$$
T_{t,out} = T_{t,in}\left(1 + \frac{\pi^{(\gamma-1)/\gamma} - 1}{\eta_c}\right) \quad \text{(compression)}
$$

$$
T_{t,out} = T_{t,in}\left(1 - \eta_t\left(1 - \pi^{-(\gamma-1)/\gamma}\right)\right) \quad \text{(expansion)}
$$

Overall pressure ratio (60 for AVE's GE9X-class engine), fan pressure
ratio (1.45), and turbine inlet temperature (1,670 K) are the three inputs
that, run through this chain, produce the station temperatures in
[Propulsion analysis](../propulsion-analysis.md#the-on-design-cycle) and,
from an energy and momentum balance across the whole engine, thermal
efficiency, propulsive efficiency, specific thrust, and TSFC.

## Takeoff rotation: the forward-CG limit

The most-forward CG at which the nose wheel can still be lifted at the
rotation speed $V_R = 1.10\,V_S$ comes from the moment balance about the
main-gear ground contact $P$ (Sadraey, *Aircraft Design: A Systems Engineering
Approach*, Wiley 2012, sec. 9.6.2, eqs. 9.36-9.54a), nose-up positive, with
heights above the ground plane and $x$ aft:

$$
I_P\,\ddot\theta = L_{wf}(x_P - x_{ac}) + M_{ac} + L_h (x_P - x_h)
  - W (x_P - x_{cg}) + T (h_{cg} - h_T) - \mu (W - L)\,h_{cg}
$$

$$
I_P = I_{yy,cg} + m\left[(x_P - x_{cg})^2 + h_{cg}^2\right]
$$

Dividing every moment by $W = q_R S\,C_{L,R}$ and writing $d = x_P - x_{cg}$,
$\kappa = \ddot\theta/g$, the balance is the quadratic
$\kappa d^2 + d - r = 0$ with $r = A - \kappa\,(k_y^2 + h_{cg}^2)$, where $A$ is
the sum of the non-weight moments over $W$ and $k_y$ is the pitch radius of
gyration. The forward limit is the larger root,
$d = 2r / (1 + \sqrt{1 + 4\kappa r})$.

The required pitch acceleration $\ddot\theta$ is 5 deg/s$^2$ (Sadraey sec.
12.3, transports 4-6 deg/s$^2$). The tail lift at rotation follows sec. 12.6
(eqs. 12.55-12.76):

$$
C_{L,h} = a_h\left(i_h - \varepsilon + \tau_e\,\tfrac{b_e}{b_h}\,\delta_{e,\max}\right),
\qquad
\tau_e = 1 - \frac{\theta_f - \sin\theta_f}{\pi},\ \cos\theta_f = 2\tfrac{c_f}{c} - 1
$$

with the fuselage level, the DATCOM tail lift-curve slope $a_h$, the wing
downwash $\varepsilon = 2C_{L}/(\pi A)$ reduced by Wieselsberger's ground-effect
factor $\sigma = (16h/b)^2 / (1 + (16h/b)^2)$, the elevator effectiveness
$\tau_e$ reduced by the USAF DATCOM large-deflection correction (Fig.
6.1.1.1-40), $\delta_{e,\max} = -25^\circ$ (or
`landing_gear.elevator_up_travel_deg`), and the download limited to
$C_{L,h,\max} = 0.9\,c_{l,\max}\cos\Lambda_{c/4}$. A trimmable stabiliser
replaces the built incidence $i_h$ by
`-landing_gear.takeoff_stabilizer_nose_up_deg` whenever that gives more
download (4.3 deg nose-up on the registered trimmable-stabiliser presets,
from Airbus Safety First, "Incorrect pitch trim setting at takeoff").
Drag is omitted; its moment is bounded by about 1 %MAC.

## Tail-down, tip-back and main-gear placement

The tail-scrape (tail-down) angle is the largest nose-up rotation about the
main gear before a lower-fuselage point aft of it touches the ground:

$$
\theta_\text{scrape} = \min_{x > x_\text{mlg}}
  \arctan\frac{z_\text{bottom}(x) - z_\text{ground}}{x - x_\text{mlg}}
$$

The belly profile comes from the tailcone loft or, when
`geometry.fuselage.belly_upsweep_length_m` is set, from a straight rising lower
line starting that far aft of the nose tip. The tip-back (longitudinal
tip-over) angle, measured from the vertical at the static ground line, is

$$
\theta_\text{tip-back} = \arctan\frac{x_\text{mlg,aft} - x_\text{cg,aft}}{h_{cg}}
$$

at the most-aft main-gear axle and the most-aft design CG, and must satisfy
$\theta_\text{tip-back} \ge \max(\theta_{\min},\ \theta_\text{scrape})$
(Torenbeek, *Synthesis of Subsonic Airplane Design*, ch. 10), where
$\theta_{\min}$ is `landing_gear.min_tip_back_deg` (default 0). The tail-scrape
angle must in turn reach the required rotation attitude
`landing_gear.required_rotation_angle_deg` (default 10 deg), so the tail does
not touch the runway at lift-off.

For a candidate that redesigns the aircraft, the whole main-gear group is
translated aft or forward from the published stations to the feasible station
nearest them, subject to: the tip-back requirement above at the most-aft,
highest CG; the static nose-gear reaction between its steering minimum and its
handling maximum in every loading state; and the nose wheel liftable at
rotation (the forward-CG limit above). The nose-gear station and the leg
spacing stay at their published values (Raymer, *Aircraft Design: A Conceptual
Approach*, 6th ed., sec. 11.2; Currey, *Aircraft Landing Gear Design*, ch. 3).
Registered aircraft keep their published stations. These are
conceptual-design relations, not certified limits.

## The optimizer's objective

Covered in full in
[Design space & optimizer](../design-space-and-optimizer.md#the-objective-function):
the mission-sized objective (block fuel by default) minimised by
differential_evolution under hard constraints. See that chapter for the
takeoff-mass modes and the constraint rules.

## Further sources

This page is written for a reader following AVE through the pipeline, not
as exhaustive derivations. Every formula above is the one actually
implemented; the full derivations, edge cases and the source of every
empirical constant are in the repository file `docs/methods.md`.
