// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The FLOPS detailed wing bending-material factor: equations 18-32 of
//! NASA/TM-2017-219627 Vol. I, a numerical integration of the bending
//! material an idealized beam needs to carry a prescribed spanwise load,
//! with the engine inertia-relief factor of equations 27-30.
//!
//! The integration runs from the tip inboard, as FLOPS does. Stations are
//! taken nondimensional: spanwise position as a fraction of the semispan and
//! chord as a fraction of the semispan, the form FLOPS reads them in
//! (`ETAW`, `CHD`), so the factor is dimensionless and the strip weights
//! `(DY + 2 ETA) DY` of the sweep average sum to one over a full semispan.
//! The result feeds [`super::structure::WingBendingFactor::Detailed`].
//!
//! Two recurrences are typeset ambiguously in the memorandum (equations 29
//! and 30). They are resolved the way the FLOPS source resolves them, as
//! read in NASA Aviary's `wing_detailed.py` (Apache-2.0) and checked against
//! the FLOPS-run `LargeSingleAisle1FLOPS` validation case recorded in an
//! internal FLOPS/Aviary validation report: the sweep-bucket
//! bracket of equation 31 divides the unadjusted factor, and the engine
//! moment arm accumulates the per-strip load-path secant. The arm sums over
//! every pod outboard of a station (equation 30's `EETA`), which for one pod
//! per side is the single-engine arm FLOPS evaluates.

/// One spanwise integration station of the built wing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingStation {
    /// Spanwise position, fraction of the semispan (0 at the centreline).
    pub eta: f64,
    /// Local chord divided by the semispan.
    pub chord_per_semispan: f64,
    /// Local thickness-to-chord ratio.
    pub thickness_to_chord: f64,
    /// Local load intensity factor, 0-1.
    pub load_intensity: f64,
    /// Sweep of the load path at the station, degrees.
    pub load_path_sweep_deg: f64,
}

/// The detailed bending factor and its inertia-relief companion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetailedBendingFactor {
    /// `BTB`: the factor before the sweep-bucket adjustment (equation 26).
    pub unadjusted: f64,
    /// `BT`: the adjusted factor (equation 31).
    pub bt: f64,
    /// `BTE`: the engine inertia-relief factor (equation 27).
    pub bte: f64,
    /// `ASW`: the distance-weighted average load-path sweep, degrees.
    pub average_sweep_deg: f64,
}

/// An elliptical load intensity, `sqrt(1 - eta^2)`, the distribution FLOPS
/// documents as its default when no pressure loads are supplied.
pub fn elliptical_load_intensity(eta: f64) -> f64 {
    (1.0 - eta.clamp(0.0, 1.0).powi(2)).max(0.0).sqrt()
}

/// Equations 18-32: the detailed bending factor for `stations` ordered from
/// the centreline outboard, with `engine_eta` the spanwise positions of the
/// wing-mounted engine pods as fractions of the semispan.
///
/// Returns `None` for fewer than two stations, a zero total load, or a
/// nonpositive chord or thickness at any station.
pub fn detailed_bending_factor(
    stations: &[WingStation],
    engine_eta: &[f64],
    aspect_ratio: f64,
    aeroelastic_tailoring: f64,
    strut_bracing: f64,
) -> Option<DetailedBendingFactor> {
    if stations.len() < 2 {
        return None;
    }
    if stations
        .iter()
        .any(|station| station.chord_per_semispan <= 0.0 || station.thickness_to_chord <= 0.0)
    {
        return None;
    }
    // Tip first, as the FLOPS accumulation runs.
    let mut order: Vec<&WingStation> = stations.iter().collect();
    order.sort_by(|a, b| b.eta.total_cmp(&a.eta));

    let mut weighted_sweep = 0.0;
    let mut weight_sum = 0.0;
    let mut shear = 0.0; // EL: load outboard of the current strip
    let mut moment = 0.0; // EM: sweep-modified moment at the strip's inboard station
    let mut engine_arm = 0.0; // EEM: sweep-modified pod moment arm, per unit pod weight
    let mut bma_prev: Option<f64> = None;
    let mut ea_prev: Option<f64> = None;
    let mut area_moment = 0.0; // PMtot
    let mut relief = 0.0; // trapezoid sum for BTE

    for pair in order.windows(2) {
        let outboard = pair[0];
        let inboard = pair[1];
        let dy = outboard.eta - inboard.eta;
        if dy <= 0.0 {
            continue;
        }
        let csw = 1.0 / inboard.load_path_sweep_deg.to_radians().cos();
        weighted_sweep += (dy + 2.0 * inboard.eta) * dy * inboard.load_path_sweep_deg;
        weight_sum += (dy + 2.0 * inboard.eta) * dy;

        let (c_out, c_in) = (outboard.chord_per_semispan, inboard.chord_per_semispan);
        let (p_out, p_in) = (outboard.load_intensity, inboard.load_intensity);
        let delp = dy * (c_out * (2.0 * p_out + p_in) + c_in * (2.0 * p_in + p_out)) / 6.0;
        let delm = dy * dy * (c_out * (3.0 * p_out + p_in) + c_in * (p_in + p_out)) / 12.0;
        moment += (delm + dy * shear) * csw;
        shear += delp;
        let stiffness = c_in * inboard.thickness_to_chord;
        let bma = moment * csw / stiffness;
        if let Some(previous) = bma_prev {
            area_moment += (previous + bma) * dy / 2.0;
        }
        bma_prev = Some(bma);

        // Equation 30: the strip's contribution to the moment arm of every
        // pod outboard of its inboard station, the full strip width for a
        // pod beyond the strip and the partial distance for a pod inside it.
        let delme: f64 = engine_eta
            .iter()
            .filter(|&&eta| eta > inboard.eta)
            .map(|&eta| (eta - inboard.eta).min(dy))
            .sum();
        engine_arm += delme * csw;
        let ea = engine_arm * csw / stiffness;
        if let Some(previous) = ea_prev {
            relief += (previous + ea) * dy / 2.0;
        }
        ea_prev = Some(ea);
    }
    if shear <= 0.0 || weight_sum <= 0.0 {
        return None;
    }
    let unadjusted = 4.0 * area_moment / shear;
    let bte = 8.0 * relief;
    // Equation 18 is a plain weighted sum: the strip weights add to one when
    // the stations span the semispan, and FLOPS does not renormalise a
    // station set that stops short of the tip (the FLOPS-run case in the
    // validation data ends at 93.67 percent and reproduces only this way).
    let average_sweep_deg = weighted_sweep;
    let sa = average_sweep_deg.to_radians().sin();
    let caya = (aspect_ratio - 5.0).max(0.0);
    let bt = unadjusted
        / (aspect_ratio.powf(0.25 * strut_bracing)
            * (1.0
                + (0.5 * aeroelastic_tailoring - 0.16 * strut_bracing) * sa * sa
                + 0.03 * caya * (1.0 - 0.5 * aeroelastic_tailoring) * sa));
    Some(DetailedBendingFactor {
        unadjusted,
        bt,
        bte,
        average_sweep_deg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangular_wing(n: usize, chord_per_semispan: f64, tc: f64) -> Vec<WingStation> {
        (0..n)
            .map(|i| {
                let eta = i as f64 / (n - 1) as f64;
                WingStation {
                    eta,
                    chord_per_semispan,
                    thickness_to_chord: tc,
                    load_intensity: elliptical_load_intensity(eta),
                    load_path_sweep_deg: 0.0,
                }
            })
            .collect()
    }

    #[test]
    fn the_unadjusted_factor_matches_the_closed_form_for_an_unswept_rectangular_wing() {
        // For a rectangular wing with an elliptical load, the beam statics
        // integrate in closed form: total load c*pi/4, root moment c/3,
        // integrated moment c*pi/32, so BTB = 4 * (pi/32) / (c * tc * pi/4)
        // = 1 / (2 c tc).
        let c = 0.222;
        let tc = 0.1;
        let stations = rectangular_wing(401, c, tc);
        let factor = detailed_bending_factor(&stations, &[], 9.0, 0.0, 0.0)
            .unwrap_or_else(|| panic!("a rectangular wing integrates"));
        let expected = 1.0 / (2.0 * c * tc);
        assert!(
            (factor.unadjusted - expected).abs() / expected < 2e-3,
            "{} vs {expected}",
            factor.unadjusted
        );
        assert_eq!(factor.average_sweep_deg, 0.0);
        assert_eq!(factor.bte, 0.0);
        assert!((factor.bt - factor.unadjusted).abs() < 1e-12);
    }

    #[test]
    fn engines_outboard_produce_a_positive_relief_factor_that_grows_with_their_span_station() {
        let stations = rectangular_wing(201, 0.25, 0.12);
        let inboard = detailed_bending_factor(&stations, &[0.3], 8.0, 0.0, 0.0)
            .unwrap_or_else(|| panic!("integrates"));
        let outboard = detailed_bending_factor(&stations, &[0.6], 8.0, 0.0, 0.0)
            .unwrap_or_else(|| panic!("integrates"));
        assert!(inboard.bte > 0.0);
        assert!(outboard.bte > inboard.bte);
    }

    #[test]
    fn sweep_enters_through_the_load_path_secant_and_the_bucket_adjustment() {
        let straight = rectangular_wing(101, 0.25, 0.12);
        let swept: Vec<WingStation> = straight
            .iter()
            .map(|station| WingStation {
                load_path_sweep_deg: 30.0,
                ..*station
            })
            .collect();
        let a = detailed_bending_factor(&straight, &[], 9.0, 0.0, 0.0).unwrap_or_else(|| panic!());
        let b = detailed_bending_factor(&swept, &[], 9.0, 0.0, 0.0).unwrap_or_else(|| panic!());
        assert!((b.average_sweep_deg - 30.0).abs() < 1e-9);
        assert!(b.unadjusted > a.unadjusted);
        assert!(
            b.bt < b.unadjusted,
            "the sweep bucket divides the factor for a high-aspect-ratio wing"
        );
    }

    #[test]
    fn degenerate_inputs_return_none() {
        assert!(detailed_bending_factor(&[], &[], 9.0, 0.0, 0.0).is_none());
        let mut bad = rectangular_wing(11, 0.25, 0.12);
        bad[3].thickness_to_chord = 0.0;
        assert!(detailed_bending_factor(&bad, &[], 9.0, 0.0, 0.0).is_none());
    }
}
