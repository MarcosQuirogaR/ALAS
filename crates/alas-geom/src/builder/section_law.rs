// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The spanwise section law of the main wing when it names sections at
//! interior stations (`alas_config::WingSectionStation`).
//!
//! The law runs through the root section at the centreline, the named
//! sections at their stations and the tip airfoil at the tip, and blends
//! linearly in span between consecutive ones, the blend
//! [`Airfoil::blend_with_another_airfoil`] makes. A named section is built
//! like the root section (the design vector's bumps and thickness and camber
//! scales); the tip airfoil is taken as declared, as without stations. With
//! no station there is no law and the loft keeps the root-kink-tip rule.
//! Frame: `eta = y / semispan`.

use super::{AircraftBuilder, BuildError};
use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::spacing::linspace;
use crate::aircraft::wing::WingXSec;
use crate::airfoil_library::{build_section, DEFAULT_MORPH_N_POINTS};
use alas_config::{DesignVector, STATION_SPAN_TOLERANCE};

/// The main wing's root section, tip airfoil and section law, as the profile
/// passes take them.
pub(super) type Sections<'a> = (&'a Airfoil, &'a Airfoil, Option<&'a SectionLaw>);

/// The sections of the law, centreline to tip: each as built and repanelled
/// to the blend's point count.
pub(super) struct SectionLaw {
    anchors: Vec<(f64, Airfoil, Airfoil)>,
}

impl SectionLaw {
    /// The section at `eta`: a section of the law itself within the station
    /// tolerance of its station, else the blend of the two either side.
    pub(super) fn at(&self, eta: f64) -> Airfoil {
        if let Some((_, section, _)) = self
            .anchors
            .iter()
            .find(|(station, _, _)| (station - eta).abs() <= STATION_SPAN_TOLERANCE)
        {
            return section.clone();
        }
        let outboard = self
            .anchors
            .iter()
            .position(|(station, _, _)| *station > eta)
            .unwrap_or(self.anchors.len() - 1)
            .max(1);
        let ((eta_a, a, foil_a), (eta_b, b, foil_b)) =
            (&self.anchors[outboard - 1], &self.anchors[outboard]);
        let blend = ((eta - eta_a) / (eta_b - eta_a)).clamp(0.0, 1.0);
        // The coordinates and name `blend_with_another_airfoil` gives, from the
        // sections repanelled once.
        Airfoil {
            name: format!(
                "{:.0}% {}, {:.0}% {}",
                (1.0 - blend) * 100.0,
                a.name,
                blend * 100.0,
                b.name
            ),
            coordinates: foil_a
                .coordinates
                .iter()
                .zip(&foil_b.coordinates)
                .map(|(&(ax, ay), &(bx, by))| {
                    (
                        (1.0 - blend) * ax + blend * bx,
                        (1.0 - blend) * ay + blend * by,
                    )
                })
                .collect(),
        }
    }

    /// Maximum local t/c of the section at `eta` on the 101-station sampling
    /// the aerodynamics and the thickness multipliers use.
    pub(super) fn max_tc_at(&self, eta: f64) -> f64 {
        self.at(eta).max_thickness(&linspace(0.0, 1.0, 101))
    }

    /// Give every defining section the law's section at its station.
    pub(super) fn apply(&self, semispan_m: f64, xsecs: &mut [WingXSec]) {
        for xsec in xsecs {
            xsec.airfoil = self.at(xsec.xyz_le[1] / semispan_m);
        }
    }
}

impl AircraftBuilder {
    /// The main wing's section law, or `None` when the wing names no section
    /// stations.
    pub(super) fn section_law(
        &self,
        dv: &DesignVector,
        root_section: &Airfoil,
        tip_airfoil: &Airfoil,
    ) -> Result<Option<SectionLaw>, BuildError> {
        let stations = &self.geometry.wing.section_stations;
        if stations.is_empty() {
            return Ok(None);
        }
        let mut sections = vec![(0.0, root_section.clone())];
        for station in stations {
            let declared = Self::resolve(&station.airfoil)?;
            sections.push((
                station.span_fraction,
                build_section(dv, &declared.coordinates)?,
            ));
        }
        sections.push((1.0, tip_airfoil.clone()));
        let mut anchors = Vec::with_capacity(sections.len());
        for (eta, section) in sections {
            let repanelled = section.repanel(DEFAULT_MORPH_N_POINTS)?;
            anchors.push((eta, section, repanelled));
        }
        Ok(Some(SectionLaw { anchors }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{GeometryConfig, WingSectionStation, WingThicknessStation};

    fn named(stations: &[(f64, &str)]) -> GeometryConfig {
        let mut g = GeometryConfig::default();
        g.wing.root_airfoil = "naca2415".to_owned();
        g.wing.tip_airfoil = "naca2410".to_owned();
        g.wing.section_stations = stations
            .iter()
            .map(|&(span_fraction, airfoil)| WingSectionStation {
                span_fraction,
                airfoil: airfoil.to_owned(),
            })
            .collect();
        g
    }

    fn build(g: GeometryConfig) -> crate::aircraft::airplane::Airplane {
        AircraftBuilder::new(Some(g))
            .build(Some(&DesignVector::default()), false)
            .expect("wing builds")
    }

    #[test]
    fn a_named_station_is_a_loft_section_carrying_its_section() {
        let plane = build(named(&[(0.2, "naca0012"), (0.6, "naca4412")]));
        let wing = &plane.wings[0];
        let semispan = wing.xsecs.last().expect("tip").xyz_le[1];
        for (eta, expected) in [(0.2, 0.12), (0.6, 0.12)] {
            let section = wing
                .xsecs
                .iter()
                .find(|xsec| (xsec.xyz_le[1] / semispan - eta).abs() < 1.0e-9)
                .expect("the station is a mesh edge");
            let tc = section.airfoil.max_thickness(&linspace(0.0, 1.0, 101));
            assert!((tc - expected).abs() < 2.0e-3, "eta {eta}: t/c {tc}");
        }
        // Between the stations the section is their blend: no camber at 0.2,
        // 4 % at 0.6, so the camber at 40 % chord rises between them.
        let camber: Vec<f64> = wing
            .xsecs
            .iter()
            .filter(|xsec| {
                let eta = xsec.xyz_le[1] / semispan;
                eta > 0.25 && eta < 0.55
            })
            .map(|xsec| xsec.airfoil.local_camber(&[0.4])[0])
            .collect();
        assert!(camber.len() > 1, "{camber:?}");
        assert!(camber.windows(2).all(|w| w[1] > w[0]), "{camber:?}");
        assert!(camber.iter().all(|&c| c > 0.0 && c < 0.04), "{camber:?}");
    }

    #[test]
    fn the_thickness_stations_multiply_the_named_section() {
        let mut g = named(&[(0.4, "naca0012")]);
        g.wing.thickness_stations = vec![WingThicknessStation {
            span_fraction: 0.4,
            thickness_scale: 0.75,
        }];
        let plane = build(g);
        let wing = &plane.wings[0];
        let semispan = wing.xsecs.last().expect("tip").xyz_le[1];
        let section = wing
            .xsecs
            .iter()
            .find(|xsec| (xsec.xyz_le[1] / semispan - 0.4).abs() < 1.0e-9)
            .expect("the station is a mesh edge");
        let tc = section.airfoil.max_thickness(&linspace(0.0, 1.0, 101));
        assert!((tc - 0.09).abs() < 2.0e-3, "t/c {tc}");
    }

    #[test]
    fn an_unknown_section_name_is_a_typed_build_error() {
        assert!(matches!(
            AircraftBuilder::new(Some(named(&[(0.5, "no-such-section")])))
                .build(Some(&DesignVector::default()), false),
            Err(BuildError::UnresolvedAirfoil(_))
        ));
    }
}
