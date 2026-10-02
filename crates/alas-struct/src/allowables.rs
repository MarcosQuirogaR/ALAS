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
    if material.category == "composite" && material.name.starts_with("CFRP") {
        material.f_allow_pa.min(0.004 * material.e_pa)
    } else {
        material.f_allow_pa
    }
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
}
