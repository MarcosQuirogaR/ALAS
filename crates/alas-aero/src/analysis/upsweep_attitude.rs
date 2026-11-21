// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuselage upsweep drag at a body attitude: the build-up charges it at
//! alpha 0 ([`super::parasite_items::upsweep_frontal_cd`]); a trimmed state
//! adds its change at the attitude it flies.

use alas_atmo::Atmosphere;
use alas_geom::aircraft::fuselage::Fuselage;

use super::parasite::body_length_and_diameter;
use super::parasite_items::{upsweep_drag_area, AftBody};
use super::AeroAnalysis;

impl AeroAnalysis<'_> {
    /// The aft body of `fuselage` from the configured aft-body start.
    pub(super) fn aft_body(&self, fuselage: &Fuselage) -> Option<AftBody> {
        let (length, _) = body_length_and_diameter(fuselage);
        let nose_x = fuselage.xsecs.first().map_or(0.0, |xsec| xsec.xyz_c[0]);
        AftBody::of(
            fuselage,
            Some(nose_x + self.geometry.fuselage.aft_body_start_m(length)),
        )
    }

    /// Upsweep drag coefficient at afterbody flow angle `afterbody_alpha_deg`
    /// ([`Self::afterbody_flow`]), the viscous margin included. Zero without a
    /// body, for the reference-compatibility build-up, which carries no
    /// upsweep, and for Poll and Schumann's turbofan build-up, whose fuselage
    /// `FIFS` holds the afterbody ([`Self::poll_build_up`]).
    fn upsweep_cd_at(&self, afterbody_alpha_deg: f64) -> f64 {
        let aft = match self.plane.fuselages.first() {
            Some(fuselage) if !self.reference_compatibility && !self.poll_build_up() => {
                self.aft_body(fuselage)
            }
            _ => None,
        };
        aft.map_or(0.0, |aft| {
            upsweep_drag_area(aft.upsweep_rad, aft.max_area_m2, afterbody_alpha_deg)
                / self.plane.s_ref
                * self.drag.margin()
        })
    }

    /// The upsweep share of the build-up's zero-lift parasite drag (alpha 0).
    pub fn upsweep_cd0(&self) -> f64 {
        self.upsweep_cd_at(0.0)
    }

    /// Change of the upsweep drag coefficient from the build-up's alpha 0
    /// value at fuselage angle of attack `body_alpha_deg` and aircraft lift
    /// coefficient `cl`: the isolated-fuselage data at the afterbody's flow
    /// angle behind the wing.
    pub fn upsweep_attitude_cd(&self, body_alpha_deg: f64, cl: f64) -> f64 {
        if self.upsweep_cd0() == 0.0 {
            return 0.0;
        }
        let afterbody_alpha_deg = self.afterbody_flow().alpha_deg(body_alpha_deg, cl);
        self.upsweep_cd_at(afterbody_alpha_deg) - self.upsweep_cd_at(0.0)
    }

    /// [`Self::drag_components`] of a trimmed state flown at body angle of
    /// attack `body_alpha_deg`: the parasite term carries the upsweep at that
    /// attitude ([`Self::upsweep_attitude_cd`]). The lift-dependent viscous
    /// term scales the zero-lift profile drag, upsweep included, as in the
    /// drag table.
    pub fn drag_components_at_attitude(
        &self,
        mach: f64,
        altitude_m: f64,
        cl: f64,
        cd_induced: f64,
        atmosphere: Option<&Atmosphere>,
        body_alpha_deg: f64,
    ) -> super::DragComponents {
        let mut components = self.drag_components(mach, altitude_m, cl, cd_induced, atmosphere);
        components.cd_parasite += self.upsweep_attitude_cd(body_alpha_deg, cl);
        components
    }
}
