# Aerodynamic exposed-area correction

The hybrid parasite buildup uses gross projected wing area for coefficient
normalization and exposed wing area for skin friction. With
`drag_model.exclude_buried_main_wing_area = true` (the product default), the
main-wing center section inside the body is subtracted before applying the
configured wing wetted-area factor. Setting it to `false` retains the gross
wing-area convention; `AeroAnalysis::new_reference_compatibility` also retains
the frozen convention independently of this flag.

The basis is the exposed-area relation in [NASA NDARC Theory, v1.6, wing drag
model](https://rotorcraft.arc.nasa.gov/Publications/files/NDARCTheory_v1_6_938.pdf),
`S_wet = 2(S - c*w_fus)`. ALAS integrates the piecewise-linear chord over the
buried span, interpolates the local fuselage cross-section at the root quarter
chord, and evaluates its superellipse width at wing-root height. Thus a detached
high wing is not shielded merely because it projects onto the body. The
aircraft reference area is unchanged.

This is a local extruded-body approximation, not a mesh intersection: it does
not resolve fairings, thickness, changing body contour over the chord, twist
in the buried-area metric, or dihedral crossing the body surface. It is limited
to symmetric main wings and centered fuselages. Tail wetted-area conventions
are unchanged. OpenVSP's [Comp Geom](https://www.nasa.gov/reference/openvsp-comp-geom/)
offers an intersected-surface calculation for higher-fidelity checks.

Reproduce the numerical change with
`cargo run -p alas-aero --example parasite_area_correction`. Analytic tests cover
a rectangular center section, a tapered center section, vertical and horizontal
detachment, and an oversized body; the analysis test verifies flag selection,
legacy replay, and unchanged induced and wave terms. These establish numerical
verification, not aircraft drag calibration or physical validation.

For the current preset geometry, the isolated flag comparison at each preset's
cruise Mach and altitude gives the following dimensionless parasite coefficients
(these are not fitted total-polar intercepts):

| Preset | Gross convention | Exposed convention | Change |
| --- | ---: | ---: | ---: |
| AVE | 0.01764529 | 0.01666136 | -5.576% |
| A340-300 | 0.01851220 | 0.01750726 | -5.429% |
| A380-800 | 0.01468718 | 0.01368473 | -6.825% |
| B787-9 | 0.01899801 | 0.01780423 | -6.284% |
| A320-200 | 0.02196036 | 0.02052914 | -6.517% |
| A220-300 | 0.02281666 | 0.02157961 | -5.422% |
| ATR72-600 | 0.02460001 | 0.02460001 | 0.000% |
| DC-10 | 0.01844081 | 0.01715870 | -6.953% |

The ATR main-wing root is above the modeled fuselage; its projected overlap is
therefore not subtracted. These modest changes do not remove the reported
25-63% differences against published drag estimates. Further agreement requires
equivalent observables and flight conditions, geometry checks, and independent
calibration evidence; no multiplier is fitted to those reported differences.

## Interpreting drag and lift comparisons

Sun, Hoekstra and Ellerbroek's [2020 drag-polar
paper](https://pure.tudelft.nl/ws/portalfiles/portal/71038050/published_OpenAP_drag_polar.pdf)
estimates polar coefficients from flight surveillance with a stochastic total
energy model. Its coefficients are model-based estimates, not directly
measured component drag. A fitted total-polar intercept is not necessarily the
parasite buildup alone, and effective Oswald efficiency from a total polar is
not the same observable as pointwise inviscid span efficiency from a solver.

`PolarSweep` exposes both the geometric solve angle and the Prandtl-Glauert
relabelled reporting angle. A slope on the latter axis is an analytical
compressibility approximation, not a compressible VLM solution. Low-speed
wing-alone estimates must not be scored against a whole-aircraft cruise slope.

There is no universal swept-wing efficiency ceiling below 0.968. The planar
elliptic-loading bound is conditional; nonplanar lifting systems can exceed
unity for a fixed projected reference span. See [NASA's nonplanar lifting-line
discussion](https://ntrs.nasa.gov/api/citations/19920016018/downloads/19920016018.pdf).
