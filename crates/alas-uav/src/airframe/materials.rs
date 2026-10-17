// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Spar material sets for the design evaluation (UAV-PLAN M4, owner decision D9).
//!
//! Two selectable sources:
//!
//! * [`SparMaterials::LiteratureDefault`]: the named carbon/epoxy set below.
//!   LITERATURE DEFAULT, NOT MEASURED.  Its constants are order-of-magnitude
//!   values for a T300/5208-class prepreg (Jones, *Mechanics of Composite
//!   Materials*), not checked against the page and not a database record.  Any
//!   margin computed with it is a screening value only.
//! * [`SparMaterials::Database`]: two `alas-uav-db` laminate rows (cap and web).
//!   Used once the database carries cited ply data; a missing field is an error.

use alas_uav_db::parts::LaminateSpec;

use super::clt::{LaminaProps, Ply, PlyStrength};
use super::spar::{SparDesign, SparGeometry};
use super::AirframeError;

/// Name of the literature default set (recorded in results and messages).
pub const LITERATURE_DEFAULT_NAME: &str = "literature-default-cfrp-t300-5208 (not measured)";

/// Source note of the literature default set.
pub const LITERATURE_DEFAULT_SOURCE: &str =
    "Jones, Mechanics of Composite Materials: T300/5208-class order of magnitude; \
     strengths typical, knock-down 0.8; not checked against the page; not measured";

/// Ply thickness of the literature default, m (0.125 mm prepreg).
const DEFAULT_PLY_THICKNESS_M: f64 = 0.125e-3;
/// Areal mass of one default ply, kg/m^2 (rho 1600 kg/m^3 times thickness).
const DEFAULT_PLY_AREAL_MASS_KG_M2: f64 = 0.2;

/// Source of the spar laminates.
#[derive(Debug, Clone, PartialEq)]
pub enum SparMaterials {
    /// The named literature default set (NOT MEASURED).
    LiteratureDefault,
    /// Cap and web rows from `alas-uav-db`.
    Database(Box<DatabaseRows>),
}

/// Cap and web laminate rows of the database source.
#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseRows {
    /// Cap laminate row (fibres along the span).
    pub cap: LaminateSpec,
    /// Web laminate row (carries the shear through its in-plane strength).
    pub web: LaminateSpec,
}

fn default_props() -> Result<LaminaProps, AirframeError> {
    LaminaProps::new(181.0e9, 10.3e9, 7.17e9, 0.28)
}

fn default_strength(knockdown: f64) -> PlyStrength {
    PlyStrength {
        xt_pa: knockdown * 1.5e9,
        xc_pa: knockdown * 1.2e9,
        yt_pa: knockdown * 4.0e7,
        yc_pa: knockdown * 1.5e8,
        s12_pa: knockdown * 6.0e7,
    }
}

/// Build the spar design for `geometry` from a material source.
///
/// The literature default uses a 4-ply `[0]` cap and a `[+45/-45]` web of the
/// same ply, with strengths knocked down by 0.8.  Both are LITERATURE
/// DEFAULT values, not measured.
///
/// # Errors
///
/// A missing database field, an invalid safety factor, or invalid constants.
pub fn spar_design(
    geometry: SparGeometry,
    materials: &SparMaterials,
    safety_factor: f64,
    stations: usize,
) -> Result<SparDesign, AirframeError> {
    match materials {
        SparMaterials::LiteratureDefault => {
            let props = default_props()?;
            let ply = |angle_deg: f64| Ply {
                angle_deg,
                thickness_m: DEFAULT_PLY_THICKNESS_M,
                areal_mass_kg_m2: DEFAULT_PLY_AREAL_MASS_KG_M2,
                props,
            };
            if !(safety_factor.is_finite() && safety_factor >= 1.0) {
                return Err(AirframeError::Invalid(
                    "ultimate safety factor must be >= 1".into(),
                ));
            }
            Ok(SparDesign {
                geometry,
                cap: vec![ply(0.0); 4],
                cap_strength: default_strength(0.8),
                web: vec![ply(45.0), ply(-45.0)],
                web_strength: default_strength(0.8),
                stations,
                safety_factor,
            })
        }
        SparMaterials::Database(rows) => SparDesign::from_db(
            geometry,
            &rows.cap,
            &[0.0; 4],
            &rows.web,
            &[45.0, -45.0],
            safety_factor,
            stations,
        ),
    }
}

#[cfg(test)]
// Unit tests: a failed unwrap or expect is the assertion failing on fixtures the test builds.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn literature_default_builds_a_valid_spar_and_is_labelled() {
        let g = SparGeometry {
            semispan_m: 1.2,
            root_depth_m: 0.05,
            tip_depth_m: 0.05,
            root_cap_width_m: 0.02,
            tip_cap_width_m: 0.02,
        };
        let d = spar_design(g.clone(), &SparMaterials::LiteratureDefault, 1.5, 100).unwrap();
        assert_eq!(d.cap.len(), 4);
        assert!(LITERATURE_DEFAULT_NAME.contains("not measured"));
        assert!(LITERATURE_DEFAULT_SOURCE.contains("not measured"));
        assert!(spar_design(g, &SparMaterials::LiteratureDefault, 0.5, 100).is_err());
    }
}
