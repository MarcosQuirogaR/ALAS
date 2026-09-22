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

At AVE's cruise point ($\rho \approx 0.365\ \text{kg/m}^3$ at 11,887 m,
$V \approx 248\ \text{m/s}$ at M0.84), $q \approx 11{,}200$ Pa. With
$W = 358{,}670 \times 9.81 \approx 3.52 \times 10^6$ N and
$S = 529.0\ \text{m}^2$, that gives $C_L \approx 0.59$-$0.66$ depending on
exactly which weight point you evaluate at, consistent with the 0.656
[Aerodynamic analysis](../aerodynamic-analysis.md) reports at MTOW.

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
$C_{D0} = 0.0205$ comes from.

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

$e$ (Oswald efficiency, 0.859 for AVE's baseline) is derived from the VLM
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

AVE's baseline: $x_{NP} = 51.8\%\,MAC$, $x_{CG} = 20.3\%\,MAC$,
$\overline{c} = 9.63\ \text{m}$, giving $SM = 31.5\%$; see
[Weight, balance & stability](../weight-balance-and-stability.md#static-margin-and-the-neutral-point)
for the full picture including the CG envelope this constrains.

## Mass: Torenbeek's statistical methods

Component masses (wing, fuselage, gear, …) come from Torenbeek's
regression-based structural-weight equations: each a function of the
relevant sizing loads and geometry (wing mass scales with span, taper,
sweep, and ultimate load factor; fuselage mass with length, diameter, and
pressurization differential; and so on), calibrated against decades of
real aircraft data rather than derived from first principles. This is
deliberately the *same* family of method a conceptual-design textbook
would use, which is what makes the independent FEM cross-check in
[Structural analysis](../structural-analysis.md#mass-two-independent-estimates-cross-checked)
meaningful: two unrelated methods agreeing is real evidence, not a
circular check.

## The V-n diagram

Maneuvering speed is where the positive limit load factor intersects the
stall boundary:

$$
n_{lim} = \frac{ultimate\_load\_factor}{1.5}, \qquad V_A = V_S\sqrt{n_{lim}}
$$

(CS-25.303's 1.5 ultimate-to-limit safety factor, applied in reverse to
recover the limit load from ALAS's `ultimate_load_factor` input.) For
AVE, $n_{lim} = 3.75/1.5 = 2.5$, and with $V_S \approx 165\ \text{kt}$,
$V_A \approx 262\ \text{kt}$, matching
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

## The optimizer's objective

Covered in full in
[Design space & optimizer](../design-space-and-optimizer.md#the-objective-function):
a weighted L/D reward minus a long sum of constraint penalties, searched
with SciPy's `differential_evolution`. Not repeated here since that
chapter's framing (which penalty maps to which physical constraint) is
the more useful version of the same information.

## Where this diverges from the private engineering docs

This page is written for a reader following AVE through the pipeline, not
as exhaustive derivations: every formula above is the one actually
implemented, but the full derivations, edge cases, and every empirical
constant's literature source live in ALAS's own internal engineering
documentation, not duplicated here.
