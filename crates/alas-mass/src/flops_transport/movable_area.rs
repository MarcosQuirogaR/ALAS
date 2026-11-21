// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! FLOPS `SFLAP`: the movable-surface planform area the built wing carries.
//!
//! [`super::product`] resolves the declared architecture into
//! [`super::FlopsTransportInputs`]; this module answers the one question in
//! that set that is pure geometry: how much movable area the configured
//! control-surface runs actually cut from the built planform, and states
//! the convention it resolves `SFLAP` under.

use alas_config::ControlSurfacesConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

use super::product::main_wing;

/// The projected planform area of both semispans of a wing between two spanwise
/// control-surface stations, m^2.
///
/// [`alas_config::ControlSurfacesConfig`] declares each run as "a fraction of
/// wing semi-span from the root", and the semi-span it means is the
/// **laterally projected** one: `alas-opt`'s `transport_planform`
/// `flap_area_fraction`, the other consumer of these same fields, resolves
/// the stations in `y` and normalises by [`Wing::projected_area`]. This
/// routine uses the identical measure so the two never disagree about where
/// a flap ends.
///
/// Working in projected `y` also disposes of a winglet without a special
/// case: a near-vertical tip device spans almost no `y`, so it contributes
/// almost nothing to either the band or the reference area, exactly as it
/// contributes nothing to the wing reference area FLOPS is given. A loft path
/// measured in the YZ plane would instead charge the winglet's full height
/// against the semi-span and push every declared station inboard.
///
/// On a tapered or cranked planform the band's area is not the span fraction
/// times the wing's: it is the chord integral over the band.
///
/// Returns `None` (never a clamped or silently reordered answer) when
/// `start` or `end` is non-finite, outside `0..=1`, or out of order, or when
/// the planform spans no projected `y`.
pub(super) fn chord_band_area(wing: &Wing, start: f64, end: f64) -> Option<f64> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    if !(0.0..=1.0).contains(&start) || !(0.0..=1.0).contains(&end) || end < start {
        return None;
    }
    // Projected lateral extent of each lofted segment, root to tip. A
    // near-vertical winglet segment contributes ~0 here by construction.
    let spans: Vec<f64> = wing
        .xsecs
        .windows(2)
        .map(|pair| (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs())
        .collect();
    let total: f64 = spans.iter().sum();
    let full: f64 = wing
        .xsecs
        .windows(2)
        .zip(&spans)
        .map(|(pair, &span)| span * (pair[0].chord + pair[1].chord) / 2.0)
        .sum();
    if !total.is_finite() || total <= 0.0 || !full.is_finite() || full <= 0.0 {
        return None;
    }
    if end == start {
        return Some(0.0);
    }
    let (a, b) = (start * total, end * total);
    let mut banded = 0.0;
    let mut reached = 0.0;
    for (pair, &span) in wing.xsecs.windows(2).zip(&spans) {
        let segment_start = reached;
        reached += span;
        if span <= 0.0 {
            continue;
        }
        let lo = a.max(segment_start);
        let hi = b.min(reached);
        if hi <= lo {
            continue;
        }
        // Chord varies linearly in the segment parameter t in [0, 1].
        let t_lo = (lo - segment_start) / span;
        let t_hi = (hi - segment_start) / span;
        let (c0, c1) = (pair[0].chord, pair[1].chord);
        banded += span * (c0 * (t_hi - t_lo) + (c1 - c0) * (t_hi * t_hi - t_lo * t_lo) / 2.0);
    }
    let sides = if wing.symmetric { 2.0 } else { 1.0 };
    Some(sides * banded)
}

/// FLOPS `SFLAP` for the built aircraft, m^2, under the **wing-only**
/// convention.
///
/// ### The convention, and why it is a choice rather than a proof
///
/// NASA/TM-2017-219627 Vol. I prints `SFLAP` as the "total movable wing
/// surface area including flaps, elevators, spoilers, etc." (Appendix D, and
/// the same wording at equations 35 and 97). That enumeration **explicitly
/// names elevators**, so a plain reading of the source includes the tail
/// movables, and this implementation does not.
///
/// The wing-only convention is adopted deliberately, for compatibility with
/// NASA Aviary, the agency's own implementation of these
/// equations: `flops_based/surface_controls.py` computes
/// `surface_flap_area = flap_ratio * wing_area` and adds no tail term, and
/// its `LargeSingleAisle1FLOPS` case carries `CONTROL_SURFACE_AREA = 137
/// ft^2` against a `1370 ft^2` wing, exactly the declared 0.1 ratio. Matching
/// Aviary keeps this implementation checkable against the only published
/// numeric reference available.
///
/// Two further observations are consistent with the choice but do **not**
/// establish it as physically correct:
///
/// * The memorandum's only published default is the ratio `FLAPR = 0.333` of
///   the *wing* reference area, and the registered aircraft's declared wing
///   movables are 0.23-0.42 of wing area (A320 0.368, 747-400 0.327). A default
///   ratio is a starting value, not evidence about what the fitted coefficients
///   absorbed.
/// * Equation 35's `W2` is a wing structural term, so tail area sits oddly in
///   it, but equation 97's `WSC` is an all-aircraft flight-controls group,
///   where tail movables plainly belong, and FLOPS feeds both from the same
///   `SFLAP`. The source is simply not self-consistent here.
///
/// **Consequence to be aware of:** relative to a literal reading of the
/// nomenclature this under-counts `SFLAP` by roughly a quarter on these
/// presets, which lowers `WSC` by about 16 % and `W2` by about 9 %. Switching
/// conventions is a one-line change here; it is recorded rather than hidden.
///
/// ### What is computed
///
/// One `SFLAP` feeds both the surface-controls group (equation 97) and the
/// wing shear and control-surface material `W2` (equation 35), as FLOPS does.
/// Each movable's area is its chord fraction times the projected planform area
/// of the band its declared span stations cut from the box
/// ([`Wing::carried_box`]), integrated over the real chord distribution rather
/// than assumed proportional to span. It is a physical area: it reads neither
/// the reference area, which is a quoted number with the blend bodies in it, nor
/// the blends, and no movable sits in either. A non-finite or out-of-range chord
/// fraction or span station is rejected, not clamped.
pub(super) fn movable_surface_area(
    plane: &Airplane,
    controls: &ControlSurfacesConfig,
) -> Option<f64> {
    let wing = main_wing(plane)?;
    // No movable surface sits in the body. FLOPS receives planform areas on the
    // aircraft reference plane: the bands are projected, as the unfolded area would
    // add a dihedral-dependent bias.
    let wing = wing.carried_box();
    let wing: &Wing = &wing;
    let mut movable = 0.0;
    for (chord, start, end) in [
        (
            controls.inboard_aileron_chord_fraction,
            controls.inboard_aileron_span_start_frac,
            controls.inboard_aileron_span_end_frac,
        ),
        (
            controls.slat_chord_fraction,
            controls.slat_span_start_frac,
            controls.slat_span_end_frac,
        ),
        (
            controls.aileron_chord_fraction,
            controls.aileron_span_start_frac,
            controls.aileron_span_end_frac,
        ),
        (
            controls.spoiler_chord_fraction,
            controls.spoiler_span_start_frac,
            controls.spoiler_span_end_frac,
        ),
    ] {
        if !chord.is_finite() || !(0.0..=1.0).contains(&chord) {
            return None;
        }
        movable += chord * chord_band_area(wing, start, end)?;
    }
    // The flap bands exclude the inboard aileron between two flaps, which the
    // aileron term above already counts.
    let flap_chord = controls.flap_chord_fraction;
    if !flap_chord.is_finite() || !(0.0..=1.0).contains(&flap_chord) {
        return None;
    }
    // The whole flap run is checked, as the bands of a reversed run are empty.
    chord_band_area(
        wing,
        controls.flap_span_start_frac,
        controls.flap_span_end_frac,
    )?;
    for (start, end) in controls.flap_bands() {
        movable += flap_chord * chord_band_area(wing, start, end)?;
    }
    (movable.is_finite() && movable > 0.0).then_some(movable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::GeometryConfig;
    use alas_geom::builder::AircraftBuilder;

    /// A 30 m semispan wing tapering 6 m to 1.5 m through one break, so the
    /// chord distribution is far from the constant one a span-fraction rule
    /// assumes.
    fn tapered_wing() -> alas_geom::aircraft::wing::Wing {
        use alas_geom::aircraft::airfoil::Airfoil;
        use alas_geom::aircraft::wing::{Wing, WingXSec};
        let airfoil = Airfoil::from_name("naca2412").unwrap_or_else(|| panic!("valid NACA name"));
        Wing {
            name: "Main Wing".to_owned(),
            symmetric: true,
            reference: None,
            planform_datum: None,
            dynamic_pressure_ratio: 1.0,
            reference_xsecs: None,
            tip_te_flare_m: 0.0,
            twist_about_pitch_axis: false,
            xsecs: vec![
                WingXSec::new([0.0, 0.0, 0.0], 6.0, 0.0, airfoil.clone()),
                WingXSec::new([2.0, 9.0, 0.0], 4.5, 0.0, airfoil.clone()),
                WingXSec::new([8.0, 30.0, 0.0], 1.5, 0.0, airfoil),
            ],
        }
    }

    /// The same planform given 6 degrees of dihedral outboard of the break
    /// and a near-vertical 2.5 m winglet. Projected `y` is unchanged from
    /// [`tapered_wing`] out to the tip, so every declared station and every
    /// band fraction must be unchanged too.
    fn dihedral_and_winglet_wing() -> alas_geom::aircraft::wing::Wing {
        use alas_geom::aircraft::airfoil::Airfoil;
        use alas_geom::aircraft::wing::{Wing, WingXSec};
        let airfoil = Airfoil::from_name("naca2412").unwrap_or_else(|| panic!("valid NACA name"));
        let rise = 21.0 * 6.0_f64.to_radians().tan();
        Wing {
            name: "Main Wing".to_owned(),
            symmetric: true,
            reference: None,
            planform_datum: None,
            dynamic_pressure_ratio: 1.0,
            reference_xsecs: None,
            tip_te_flare_m: 0.0,
            twist_about_pitch_axis: false,
            xsecs: vec![
                WingXSec::new([0.0, 0.0, 0.0], 6.0, 0.0, airfoil.clone()),
                WingXSec::new([2.0, 9.0, 0.0], 4.5, 0.0, airfoil.clone()),
                WingXSec::new([8.0, 30.0, rise], 1.5, 0.0, airfoil.clone()),
                // Winglet: 2.5 m of height for 0.05 m of span.
                WingXSec::new([8.6, 30.05, rise + 2.5], 0.9, 0.0, airfoil),
            ],
        }
    }

    /// The share of the whole wing a band holds, for the integration tests below.
    fn chord_band_fraction(wing: &Wing, start: f64, end: f64) -> Option<f64> {
        Some(chord_band_area(wing, start, end)? / chord_band_area(wing, 0.0, 1.0)?)
    }

    /// A movable is a physical area: the number the reference area is quoted at does
    /// not reach it.
    #[test]
    fn the_movables_read_the_bands_of_the_box_and_not_the_reference_area() {
        let preset = alas_config::presets::registry()
            .iter()
            .find(|preset| preset.name == "B747-400")
            .unwrap_or_else(|| panic!("registered preset"));
        let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .unwrap_or_else(|error| panic!("preset builds: {error}"));
        let controls = ControlSurfacesConfig::default();
        let kept = movable_surface_area(&plane, &controls)
            .unwrap_or_else(|| panic!("the preset has movables"));

        let mut quoted = plane.clone();
        let reference = quoted.wings[0]
            .reference
            .as_mut()
            .unwrap_or_else(|| panic!("the preset has a reference wing"));
        reference.area_m2 *= 1.02;
        let moved = movable_surface_area(&quoted, &controls)
            .unwrap_or_else(|| panic!("the preset has movables"));
        assert!(
            (moved - kept).abs() < 1e-12 * kept,
            "{moved} against {kept}"
        );
    }

    #[test]
    fn the_full_span_band_is_the_whole_planform_and_an_empty_band_is_zero() {
        let wing = tapered_wing();
        let full = chord_band_fraction(&wing, 0.0, 1.0).unwrap_or_else(|| panic!("integrates"));
        assert!((full - 1.0).abs() < 1e-12, "full-span band fraction {full}");
        assert_eq!(chord_band_fraction(&wing, 0.4, 0.4), Some(0.0));
        // Bands that partition the span must add back to the whole.
        let a = chord_band_fraction(&wing, 0.0, 0.37).unwrap_or_else(|| panic!("integrates"));
        let b = chord_band_fraction(&wing, 0.37, 1.0).unwrap_or_else(|| panic!("integrates"));
        assert!((a + b - 1.0).abs() < 1e-12, "{a} + {b}");
    }

    #[test]
    fn an_invalid_span_or_chord_fraction_is_rejected_rather_than_clamped() {
        let wing = tapered_wing();
        // Reversed, out of range and non-finite stations are all faults in
        // the declared configuration, not values to be quietly repaired.
        assert_eq!(chord_band_fraction(&wing, 0.7, 0.3), None);
        assert_eq!(chord_band_fraction(&wing, -0.1, 0.5), None);
        assert_eq!(chord_band_fraction(&wing, 0.5, 1.2), None);
        assert_eq!(chord_band_fraction(&wing, f64::NAN, 0.5), None);
        assert_eq!(chord_band_fraction(&wing, 0.2, f64::INFINITY), None);
        // A planform with no projected span cannot define a station at all.
        let mut collapsed = wing.clone();
        for xsec in &mut collapsed.xsecs {
            xsec.xyz_le[1] = 0.0;
        }
        assert_eq!(chord_band_fraction(&collapsed, 0.1, 0.6), None);

        // The same rejection must reach SFLAP through the public path.
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry))
            .build(None, true)
            .unwrap_or_else(|error| panic!("default geometry builds: {error}"));
        for bad in [
            ControlSurfacesConfig {
                flap_span_end_frac: 0.05,
                flap_span_start_frac: 0.10,
                ..ControlSurfacesConfig::default()
            },
            ControlSurfacesConfig {
                aileron_chord_fraction: 1.5,
                ..ControlSurfacesConfig::default()
            },
            ControlSurfacesConfig {
                slat_chord_fraction: f64::NAN,
                ..ControlSurfacesConfig::default()
            },
        ] {
            assert_eq!(movable_surface_area(&plane, &bad), None);
        }
    }

    #[test]
    fn dihedral_and_a_winglet_do_not_move_the_declared_span_stations() {
        // The control-surface fractions are laterally projected semi-span
        // fractions, the same measure `alas-opt`'s transport planform uses.
        // A YZ loft-path measure would charge the winglet's 2.5 m of height
        // against the semi-span and shift every station inboard.
        let flat = tapered_wing();
        let bent = dihedral_and_winglet_wing();
        for (start, end) in [(0.10, 0.62), (0.0, 1.0), (0.66, 0.95), (0.08, 0.95)] {
            let a = chord_band_fraction(&flat, start, end).unwrap_or_else(|| panic!("integrates"));
            let b = chord_band_fraction(&bent, start, end).unwrap_or_else(|| panic!("integrates"));
            assert!(
                (a - b).abs() < 2.0e-3,
                "band {start}-{end}: flat {a} vs dihedral+winglet {b}"
            );
        }
        // The winglet itself carries essentially no projected planform, so
        // the outboard band is not inflated by it.
        let tip = chord_band_fraction(&bent, 0.99, 1.0).unwrap_or_else(|| panic!("integrates"));
        assert!(
            tip < 0.01,
            "the winglet must not dominate the tip band: {tip}"
        );
    }

    #[test]
    fn a_movable_band_integrates_the_real_chord_rather_than_the_span_fraction() {
        // Inboard of the break the chord is 6 - (1.5/9) y and outboard
        // 4.5 - (3/21)(y - 9). A flap from 10 to 62 percent of the 30 m
        // semispan covers y in [3, 18.6]: the exact chord integral is
        // 3 to 9:    integral of 6 - y/6      = 36 - (81 - 9)/12  = 30
        // 9 to 18.6: integral of 4.5 - (y-9)/7 = 43.2 - 92.16/14  = 36.617142857...
        // over a full-planform half-area of
        // 0 to 9:  9 * (6 + 4.5) / 2 = 47.25
        // 9 to 30: 21 * (4.5 + 1.5) / 2 = 63.0
        let wing = tapered_wing();
        let exact_band = 30.0 + (43.2 - 92.16 / 14.0);
        let exact_total = 47.25 + 63.0;
        let fraction =
            chord_band_fraction(&wing, 0.10, 0.62).unwrap_or_else(|| panic!("integrates"));
        assert!(
            (fraction - exact_band / exact_total).abs() < 1e-12,
            "{fraction} vs {}",
            exact_band / exact_total
        );
        // The band carries about eleven percent more area than its span
        // fraction, which is the bias a span-fraction area would carry.
        let span_fraction = 0.62 - 0.10;
        assert!(
            fraction > span_fraction * 1.05,
            "a tapered wing must not reduce to the span fraction: {fraction} vs {span_fraction}"
        );
    }

    #[test]
    fn the_movable_surface_area_is_the_integrated_wing_bands_and_excludes_the_tails() {
        let geometry = GeometryConfig::default();
        let plane = AircraftBuilder::new(Some(geometry))
            .build(None, true)
            .unwrap_or_else(|error| panic!("default geometry builds: {error}"));
        let controls = ControlSurfacesConfig::default();
        let sflap = movable_surface_area(&plane, &controls)
            .unwrap_or_else(|| panic!("the default aircraft has movable surfaces"));

        // Reconstruct the same sum term by term, so the equation-97 and
        // equation-35 input is auditable.
        let wing = main_wing(&plane).unwrap_or_else(|| panic!("main wing"));
        let carried = wing.carried_box();
        let expected: f64 = [
            (
                controls.slat_chord_fraction,
                controls.slat_span_start_frac,
                controls.slat_span_end_frac,
            ),
            (
                controls.flap_chord_fraction,
                controls.flap_span_start_frac,
                controls.flap_span_end_frac,
            ),
            (
                controls.aileron_chord_fraction,
                controls.aileron_span_start_frac,
                controls.aileron_span_end_frac,
            ),
            (
                controls.spoiler_chord_fraction,
                controls.spoiler_span_start_frac,
                controls.spoiler_span_end_frac,
            ),
        ]
        .into_iter()
        .map(|(chord, start, end)| {
            chord * chord_band_area(&carried, start, end).unwrap_or_else(|| panic!("integrates"))
        })
        .sum();
        assert!(
            (sflap - expected).abs() < 1e-12,
            "SFLAP {sflap} must be exactly the wing bands {expected}"
        );
        // Adding the elevator and rudder bands would push the quantity well
        // past the published FLAPR default the equation coefficients were
        // fitted around; this pins that they stay out.
        let hstab = plane
            .wings
            .iter()
            .find(|surface| surface.name == "Horizontal Stabilizer")
            .unwrap_or_else(|| panic!("horizontal stabilizer"));
        let elevator = controls.elevator_chord_fraction
            * chord_band_area(
                hstab,
                controls.elevator_span_start_frac,
                controls.elevator_span_end_frac,
            )
            .unwrap_or_else(|| panic!("integrates"));
        assert!(elevator > 0.0, "the fixture must have a real elevator");
        assert!((sflap - (expected + elevator)).abs() > 1e-6);
        // FLOPS's only published default is FLAPR = 0.333 of the wing
        // reference area; a resolved SFLAP far from it would mean the
        // control-surface configuration, not the integration, is wrong.
        let ratio = sflap / wing.reference_area();
        assert!(
            (0.20..=0.45).contains(&ratio),
            "SFLAP is {ratio:.3} of the wing area, outside the FLAPR neighbourhood"
        );
    }

    /// `SFLAP` of the declared runs on the built wing against the printed totals
    /// (Jane's All the World's Aircraft 2008-09, AREAS blocks), and the one rule on
    /// all 14: no aircraft is short of or above a physical movable share.
    // The areas are printed data in m^2; 3.14 m^2 is one of them, not pi.
    #[allow(clippy::approx_constant)]
    #[test]
    fn the_declared_runs_reproduce_the_printed_movable_totals() {
        let area = |name: &str| {
            let preset = alas_config::presets::get(name).unwrap();
            let config =
                alas_config::AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&preset.design_vector), true)
                .unwrap();
            (
                movable_surface_area(&plane, &config.control_surfaces).unwrap(),
                plane.s_ref,
            )
        };
        for (name, printed) in [
            ("A320-200", 2.74 + 21.10 + 12.64 + 8.64),
            ("A400M", 2.0 * (3.94 + 20.24 + 6.90)),
            ("ATR72-600", 3.75 + 12.28 + 1.34),
            ("B747-400", 20.90 + 78.69 + 43.85 + 12.78 + 15.46),
            ("C295", 3.14 + 10.87),
        ] {
            let (built, _) = area(name);
            assert!(
                (built / printed - 1.0).abs() < 0.03,
                "{name} {built} {printed}"
            );
        }
        for name in [
            "A320-200",
            "A340-300",
            "A380-800",
            "A350-900",
            "B747-400",
            "B787-9",
            "DC-10",
            "A220-300",
            "C919",
            "E195-E2",
            "ATR72-600",
            "A400M",
            "LJ60",
            "C295",
        ] {
            let (built, s_ref) = area(name);
            let share = built / s_ref;
            assert!((0.20..0.45).contains(&share), "{name} {share}");
        }
    }
}
