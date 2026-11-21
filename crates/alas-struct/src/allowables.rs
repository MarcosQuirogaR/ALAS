// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product allowables shared by native and finite-element strength assessment.

use alas_config::materials::MaterialSpec;

/// Preliminary ultimate normal-stress allowable, Pa.
///
/// Carbon laminate proxies are limited to 0.40% ultimate strain, including a
/// nominally unidirectional cap: coupon tensile strength cannot serve as the
/// compression-after-impact allowable of a transport wing cover. See Niu,
/// *Composite Airframe Structures*, 1992, design allowables, and CMH-17-3G,
/// Vol. 3, 2012, damage tolerance. This is an explicit preliminary assumption,
/// not a measured laminate allowable. Metal and GLARE retain their declared
/// allowable. The frozen reference path deliberately uses the database value.
pub fn bending_allowable_pa(material: &MaterialSpec) -> f64 {
    if is_carbon_laminate(material) {
        material.f_allow_pa.min(0.004 * material.e_pa)
    } else {
        material.f_allow_pa
    }
}

const KSI: f64 = 6.894_757_293_168e6;

/// Factor of safety between limit and ultimate loads, CS 25.303. The strength
/// cases are ultimate loads; the limit stress of a case is its stress over
/// this factor. This is the regulation's factor, not a design margin.
pub const LIMIT_TO_ULTIMATE: f64 = 1.5;

/// Tabulated room-temperature allowables, A basis where Niu gives one, ksi:
/// `(yield, F_tu, F_su)`, with yield the lower of the L-direction `F_cy` and
/// `F_ty`, for a part of gauge `gauge_m` (`None`: the sheet column the
/// default 6 mm skin falls in). M. C. Y. Niu, *Airframe Structural Design*,
/// Conmilit Press, 1988:
///
/// * `Al 7075-T6` (Fig. 4.3.5, p. 105): sheet 0.040-0.125 in `F_tu` 78,
///   `F_ty` 70, `F_cy` 69, `F_su` 47; sheet 0.126-0.249 in 78 / 71 / 70 / 47;
///   T651 plate 0.250-0.499 in 77 / 69 / 67 / 41 ksi.
/// * `Al 2024-T3` (Fig. 4.3.3, p. 103): sheet 0.010-0.128 in `F_tu` 64,
///   `F_ty` 47, `F_cy` 39, `F_su` 39; sheet 0.129-0.249 in 65 / 47 / 39 / 40;
///   T351 plate 0.250-0.499 in 64 / 48 / 39 / 38 ksi.
/// * `Ti-6Al-4V` annealed: `F_ty` 126, `F_cy` 132, `F_tu` 134 ksi (Fig. 4.2.1,
///   p. 96); `F_su` 76 ksi (A basis lower bound, Fig. 4.4.1, p. 109).
/// * `Steel 300M` (0.42C): `F_ty` 230, `F_cy` 247, `F_tu` 280 ksi (Fig. 4.2.1,
///   p. 96); `F_su` 168 ksi (S basis, Fig. 4.5.1, p. 111).
///
/// A gauge above the thickest column read (0.499 in, 12.7 mm) takes that
/// plate column: the thicker plate columns were not read from the source.
fn tabulated_ksi(material: &MaterialSpec, gauge_m: Option<f64>) -> Option<(f64, f64, f64)> {
    let inches = gauge_m.map(|gauge| gauge / 0.0254);
    let band = |sheet_upper_in: f64| match inches {
        None => 1,
        Some(t) if t <= sheet_upper_in => 0,
        Some(t) if t <= 0.249 => 1,
        Some(_) => 2,
    };
    match material.name.as_str() {
        "Al 7075-T6" => {
            Some([(69.0, 78.0, 47.0), (70.0, 78.0, 47.0), (67.0, 77.0, 41.0)][band(0.125)])
        }
        "Al 2024-T3" => Some(
            [(39.0, 64.0, 39.0), (39.0, 65.0, 40.0), (39.0, 64.0, 38.0)]
                [if gauge_m.is_none() { 0 } else { band(0.128) }],
        ),
        "Ti-6Al-4V" => Some((126.0, 134.0, 76.0)),
        "Steel 300M" => Some((230.0, 280.0, 168.0)),
        _ => None,
    }
}

/// Combined-stress limit of a shell's own von Mises stress under the ultimate
/// strength cases, Pa.
///
/// The design allowable [`bending_allowable_pa`] of `Al 7075-T6` (380 MPa) is
/// the crippling cut-off of a stringer-stiffened cover, a panel allowable that
/// applies to the panel's mean stress along its stringers (see
/// [`crate::mesh::CoverPanel`]), not to a point of the skin sheet. The skin
/// sheet itself must satisfy CS 25.305: no permanent deformation at limit load
/// (25.305(a): the limit von Mises stress within the material yield) and no
/// failure at ultimate load (25.305(b): the ultimate von Mises stress within
/// `F_tu`). The cases are ultimate, so the two read
/// `sigma_vm <= LIMIT_TO_ULTIMATE * yield` and `sigma_vm <= F_tu`; the lesser
/// governs (for the aluminium alloys, `F_tu`). Values in [`tabulated_ksi`].
///
/// Niu tabulates no yield for the other alloys in the database (7050-T7451,
/// 6061-T6, 2195, 4130), and composites have no yield: those keep their
/// design allowable, which is the lower, more conservative figure.
pub fn shell_von_mises_allowable_pa(material: &MaterialSpec) -> f64 {
    von_mises_allowable(material, None)
}

/// [`shell_von_mises_allowable_pa`] for a skin of gauge `gauge_m`, from the
/// column of [`tabulated_ksi`] that gauge falls in, Pa.
pub fn shell_von_mises_allowable_at_pa(material: &MaterialSpec, gauge_m: f64) -> f64 {
    von_mises_allowable(material, Some(gauge_m))
}

fn von_mises_allowable(material: &MaterialSpec, gauge_m: Option<f64>) -> f64 {
    tabulated_ksi(material, gauge_m).map_or_else(
        || bending_allowable_pa(material),
        |(yield_ksi, ultimate_ksi, _)| (LIMIT_TO_ULTIMATE * yield_ksi).min(ultimate_ksi) * KSI,
    )
}

/// Ultimate shear strength of a skin, Pa: the tabulated `F_su` (see
/// [`tabulated_ksi`]); for a carbon laminate the pure-shear point of its
/// strain envelope ([`laminate_shear_strength_pa`]); else the shear that
/// reaches the design allowable [`bending_allowable_pa`] in von Mises terms,
/// `f_allow / sqrt(3)`.
///
/// It bounds the panel allowable shear stress of the compression-shear
/// interaction of a skin-stringer cover (Niu 1988, p. 119: the skin's
/// allowable shear stress is estimated from diagonal-tension data, whose upper
/// bound is the material's ultimate shear).
pub fn skin_shear_ultimate_pa(material: &MaterialSpec) -> f64 {
    shear_ultimate(material, None)
}

/// [`skin_shear_ultimate_pa`] for a skin of gauge `gauge_m`, from the column
/// of [`tabulated_ksi`] that gauge falls in, Pa.
pub fn skin_shear_ultimate_at_pa(material: &MaterialSpec, gauge_m: f64) -> f64 {
    shear_ultimate(material, Some(gauge_m))
}

fn shear_ultimate(material: &MaterialSpec, gauge_m: Option<f64>) -> f64 {
    if is_carbon_laminate(material) {
        return laminate_shear_strength_pa(material);
    }
    tabulated_ksi(material, gauge_m).map_or_else(
        || bending_allowable_pa(material) / 3.0_f64.sqrt(),
        |(_, _, shear_ksi)| shear_ksi * KSI,
    )
}

/// Whether `material` is a carbon-laminate proxy, the materials
/// [`bending_allowable_pa`] holds to the damage-tolerant design strain.
pub fn is_carbon_laminate(material: &MaterialSpec) -> bool {
    material.category == "composite" && material.name.starts_with("CFRP")
}

/// In-plane Poisson ratio `nu_LT` of the carbon/epoxy ply, AS4/3501-6 lamina,
/// NASA-TM-104055 (1991), Table 1 (the lamina of the declared cap laminate,
/// `alas_config::preset_structures::TRANSPORT_CAP_SOURCE`).
pub const CARBON_PLY_POISSON: f64 = 0.301;

/// Largest principal-stress difference `sigma_1 - sigma_2` a carbon laminate
/// skin carries, Pa: the shear truncation of the maximum-strain envelope.
///
/// Hart-Smith's truncated maximum-strain model (L. J. Hart-Smith, Composites
/// Science and Technology 58, 1998, pp. 1151-1178) cuts the maximum-strain
/// envelope in the shear quadrants by the fibre's own shear failure,
/// `eps_L - eps_T = (1 + nu_LT) eps_0`. The plies of a quasi-isotropic
/// laminate share its in-plane strain, and over the ply angles `eps_L - eps_T`
/// reaches the principal strain difference `eps_1 - eps_2`, which for the
/// isotropic proxy is `(1 + nu)(sigma_1 - sigma_2) / E`. With `eps_0` the
/// damage-tolerant design strain of [`bending_allowable_pa`] (barely visible
/// impact damage, compression after impact) the limit is
/// `E eps_0 (1 + nu_LT) / (1 + nu)`; the whole envelope is scaled to that
/// strain, so the truncation stays the fibre's shear line through it.
pub fn laminate_principal_difference_limit_pa(material: &MaterialSpec) -> f64 {
    bending_allowable_pa(material) * (1.0 + CARBON_PLY_POISSON) / (1.0 + material.nu)
}

/// Pure in-plane shear strength of a carbon laminate skin on its design strain
/// envelope, Pa: half the principal-difference limit
/// ([`laminate_principal_difference_limit_pa`]); the pure-shear point lies on
/// the truncation line, inside the maximum-strain cut-offs.
pub fn laminate_shear_strength_pa(material: &MaterialSpec) -> f64 {
    0.5 * laminate_principal_difference_limit_pa(material)
}

/// Largest direct stress `|sigma_x|` a carbon laminate skin carries alongside
/// an in-plane shear `shear_pa` (no transverse direct stress), Pa, on the
/// design strain envelope of [`laminate_principal_difference_limit_pa`]: the
/// lesser of
///
/// * the truncation, `sqrt(sigma_x^2 + 4 tau^2) <= D`, so
///   `|sigma_x| <= sqrt(D^2 - 4 tau^2)`;
/// * the maximum-strain cut-off on the larger principal strain,
///   `(sigma_1 - nu sigma_2) / E <= eps_0` with
///   `sigma_1,2 = s/2 +- sqrt(s^2/4 + tau^2)`, which is the quadratic
///   `nu s^2 + F (1 - nu) s + (1 + nu)^2 tau^2 - F^2 <= 0`, `F = E eps_0`.
///
/// Tension and compression share the design strain, so the limit holds for
/// either sign of `sigma_x`. Zero where the shear alone exhausts the laminate.
pub fn laminate_direct_stress_limit_pa(material: &MaterialSpec, shear_pa: f64) -> f64 {
    let f = bending_allowable_pa(material);
    let tau = shear_pa.abs();
    let truncation = laminate_principal_difference_limit_pa(material);
    let truncated = (truncation * truncation - 4.0 * tau * tau).max(0.0).sqrt();
    let nu = material.nu;
    let constant = (1.0 + nu).powi(2) * tau * tau - f * f;
    let cut_off = if constant >= 0.0 {
        0.0
    } else if nu.abs() < 1.0e-12 {
        -constant / f
    } else {
        let linear = f * (1.0 - nu);
        (-linear + (linear * linear - 4.0 * nu * constant).sqrt()) / (2.0 * nu)
    };
    truncated.min(cut_off.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::materials;

    #[test]
    fn carbon_allowables_obey_the_declared_damage_tolerant_strain() {
        for name in ["CFRP UD", "CFRP QI"] {
            let material = materials::get(name).expect("registered material");
            assert_eq!(bending_allowable_pa(material) / material.e_pa, 0.004);
        }
        let metal = materials::get("Al 7075-T6").expect("registered material");
        assert_eq!(bending_allowable_pa(metal), metal.f_allow_pa);
    }

    #[test]
    fn shell_limit_is_the_lesser_of_ftu_and_limit_yield_where_tabulated() {
        // 7075-T6: F_tu 78 ksi = 537.8 MPa governs 1.5 x 70 ksi = 723.9 MPa.
        let al = materials::get("Al 7075-T6").expect("registered material");
        assert!((shell_von_mises_allowable_pa(al) / 1.0e6 - 537.8).abs() < 0.1);
        assert!(shell_von_mises_allowable_pa(al) > bending_allowable_pa(al));
        assert!((skin_shear_ultimate_pa(al) / 1.0e6 - 324.1).abs() < 0.1);
        // 2024-T3: 1.5 x 39 ksi = 58.5 ksi governs F_tu 64 ksi.
        let al2 = materials::get("Al 2024-T3").expect("registered material");
        assert!((shell_von_mises_allowable_pa(al2) / KSI - 58.5).abs() < 1.0e-9);
        let cfrp = materials::get("CFRP QI").expect("registered material");
        assert_eq!(
            shell_von_mises_allowable_pa(cfrp),
            bending_allowable_pa(cfrp)
        );
        assert_eq!(
            skin_shear_ultimate_pa(cfrp),
            laminate_shear_strength_pa(cfrp)
        );
    }

    #[test]
    fn metal_skin_allowables_follow_the_gauge_column() {
        let al = materials::get("Al 7075-T6").expect("registered material");
        // 6 mm (0.236 in) is sheet; 6.5 mm (0.256 in) is T651 plate.
        assert_eq!(
            skin_shear_ultimate_at_pa(al, 0.006),
            skin_shear_ultimate_pa(al)
        );
        assert_eq!(
            shell_von_mises_allowable_at_pa(al, 0.006),
            shell_von_mises_allowable_pa(al)
        );
        assert!((skin_shear_ultimate_at_pa(al, 0.0065) / KSI - 41.0).abs() < 1.0e-9);
        assert!((shell_von_mises_allowable_at_pa(al, 0.0065) / KSI - 77.0).abs() < 1.0e-9);
        assert!((skin_shear_ultimate_at_pa(al, 0.003) / KSI - 47.0).abs() < 1.0e-9);
        let al2 = materials::get("Al 2024-T3").expect("registered material");
        assert!((skin_shear_ultimate_pa(al2) / KSI - 39.0).abs() < 1.0e-9);
        assert!((skin_shear_ultimate_at_pa(al2, 0.006) / KSI - 40.0).abs() < 1.0e-9);
        assert!((skin_shear_ultimate_at_pa(al2, 0.010) / KSI - 38.0).abs() < 1.0e-9);
        // Composites and untabulated alloys do not depend on the gauge.
        let cfrp = materials::get("CFRP QI").expect("registered material");
        assert_eq!(
            skin_shear_ultimate_at_pa(cfrp, 0.02),
            skin_shear_ultimate_pa(cfrp)
        );
    }

    #[test]
    fn the_laminate_envelope_is_the_design_strain_truncated_in_shear() {
        let cfrp = materials::get("CFRP QI").expect("registered material");
        let f = bending_allowable_pa(cfrp);
        // No shear: the truncation through the ply's own uniaxial points,
        // (1 + 0.301) / (1 + 0.31) of the design strain stress (218.5 of
        // 220 MPa at 55 GPa): the laminate's larger Poisson ratio puts its
        // uniaxial point 0.7 % outside the ply's shear line.
        let uniaxial = laminate_direct_stress_limit_pa(cfrp, 0.0);
        assert_eq!(uniaxial, laminate_principal_difference_limit_pa(cfrp));
        assert!(uniaxial < f && uniaxial > 0.99 * f);
        // Pure shear: half the principal difference, about F/2, below the von
        // Mises F/sqrt(3) the isotropic shell check uses.
        let shear = laminate_shear_strength_pa(cfrp);
        assert!((shear / f - 0.5 * 1.301 / 1.31).abs() < 1.0e-12);
        assert!(shear < f / 3.0_f64.sqrt());
        assert_eq!(laminate_direct_stress_limit_pa(cfrp, shear), 0.0);
        assert_eq!(laminate_direct_stress_limit_pa(cfrp, 2.0 * shear), 0.0);
        // Every point of the envelope satisfies both the truncation and the
        // larger-principal-strain cut-off, one of them as an equality, and lies
        // inside the von Mises limit of the shell check.
        for step in 1..100 {
            let tau = shear * f64::from(step) / 100.0;
            let s = laminate_direct_stress_limit_pa(cfrp, tau);
            let radius = (0.25 * s * s + tau * tau).sqrt();
            let (s1, s2) = (0.5 * s + radius, 0.5 * s - radius);
            let difference = (s1 - s2) / laminate_principal_difference_limit_pa(cfrp);
            let strain = (s1 - cfrp.nu * s2) / f;
            assert!(difference <= 1.0 + 1.0e-12 && strain <= 1.0 + 1.0e-12);
            assert!(
                difference.max(strain) > 1.0 - 1.0e-9,
                "on the envelope at {tau}"
            );
            assert!((s * s + 3.0 * tau * tau).sqrt() <= f);
            assert!(s < laminate_direct_stress_limit_pa(cfrp, tau - 1.0e3));
        }
        // Metals keep their own measures.
        let al = materials::get("Al 7075-T6").expect("registered material");
        assert!(!is_carbon_laminate(al));
        assert!((skin_shear_ultimate_pa(al) / 1.0e6 - 324.1).abs() < 0.1);
    }
}
