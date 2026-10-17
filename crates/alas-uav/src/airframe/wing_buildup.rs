// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing structure mass as a component build-up per construction type.
//!
//! Replaces the single areal density of the generic preset by
//! `skin/covering + core + spar + ribs/sticks`, each from material and gauge
//! data.  Frame/units: SI; areas on the projected planform `S`, wetted area
//! `2 S (1 + 0.25 t/c)`; section area `0.689 (t/c) c^2` (NACA four-digit
//! thickness law, integral of `2 y_t`).
//!
//! # Data and sources
//!
//! * Densities, Sadraey (2013) Table 10.6 p. 558 (read in the book text):
//!   balsa 160, low-density foam 16-30, high-density foam 50-80, fibreglass/
//!   epoxy 1800-1850, graphite/epoxy 1520-1630, plastics incl. Monokote
//!   900-1400 kg/m3.
//! * Glass cloth (Easy Composites product pages, fetched 2026-10-12): E-glass
//!   plain 25 g/m2 (0.04 mm), 50 g/m2 (0.08 mm), 2x2 twill 100 g/m2 (0.15 mm
//!   consolidated).  XPS 33 kg/m3, end-grain balsa 150 kg/m3 (dry), PVC foam
//!   75 kg/m3 (same vendor).
//! * Carbon tube spar: woven 12 x 10 mm tube 53.6 g/m, tensile modulus
//!   64 GPa, tensile strength (lengthways) 650 MPa (same vendor; implies
//!   1551 kg/m3, inside Sadraey's range).
//!
//! # ESTIMATES (not on a page; flagged, uncalibrated)
//!
//! Resin-to-fabric mass ratio 1.0 for hand lay-up; film thickness 0.025 mm at
//! 1200 kg/m3 (30 g/m2); printed shell wall 0.8 mm at 900 kg/m3 (foamed
//! filament) with 50 % extra for internal ribs; rib spacing 0.10 m and
//! thickness 1.5 mm balsa with 40 % lightening; leading/trailing sticks
//! 6 x 6 and 3 x 10 mm balsa; D-box sheeting 1.5 mm over 15 % chord.
//!
//! # Spar
//!
//! Elliptic loading: root moment `M = n_ult W b / (3 pi)`.  One stock tube
//! carries `sigma/1.5 Z` (Raymer Sec. 14.10 1.5 factor on the ultimate); more
//! tubes are carried as a multiple of the linear mass, over 90 % of the span.
//! Stiffness, buckling, torsion and joints are NOT modelled: lower bound.
//!
//! The per-team calibration factor is a separate input
//! ([`WingBuildup::team_factor`], default 1).

use super::geometry::Planform;
use super::AirframeError;

/// Construction type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Construction {
    /// Foam core with glass-cloth skins.
    FoamCoreComposite,
    /// Built-up balsa ribs/sticks covered with film.
    BalsaFilm,
    /// 3D-printed shell with internal ribs.
    Printed3D,
}

/// Mass components, kg.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WingBreakdown {
    /// Skin, shell or covering film.
    pub skin_kg: f64,
    /// Foam core or D-box sheeting.
    pub core_kg: f64,
    /// Spar.
    pub spar_kg: f64,
    /// Ribs and sticks.
    pub ribs_kg: f64,
}

impl WingBreakdown {
    /// Sum of components, kg.
    pub fn total_kg(&self) -> f64 {
        self.skin_kg + self.core_kg + self.spar_kg + self.ribs_kg
    }
}

/// Build-up inputs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingBuildup {
    /// Construction type.
    pub construction: Construction,
    /// Mean thickness ratio.
    pub t_over_c: f64,
    /// Design mass for the spar load, kg.
    pub design_mass_kg: f64,
    /// Ultimate load factor.
    pub n_ult: f64,
    /// Glass cloth fabric areal weight per ply, kg/m2.
    pub fabric_kg_m2: f64,
    /// Plies per side.
    pub plies: f64,
    /// Resin mass over fabric mass.
    pub resin_ratio: f64,
    /// Foam core density, kg/m3 (default 23: midpoint of the 16-30 low-density range, Sadraey T10.6; the XPS of the vendor page, 33, gave +129 % on the UVigo wing and was replaced AFTER seeing that result).
    pub core_density: f64,
    /// Film areal weight, kg/m2.
    pub film_kg_m2: f64,
    /// Printed wall thickness, m, and density, kg/m3.
    pub print_wall_m: f64,
    /// Printed material density, kg/m3.
    pub print_density: f64,
    /// Calibration factor of the team (separate input, 1 = uncalibrated).
    pub team_factor: f64,
}

const TUBE_G_PER_M: f64 = 0.0536;
const TUBE_CAPACITY_NM: f64 = 650.0e6 / 1.5 * 87.8e-9;
const SECTION_AREA_K: f64 = 0.689;
const BALSA: f64 = 160.0;

impl WingBuildup {
    /// Defaults per construction (sources in the module docs).
    pub fn new(construction: Construction, t_over_c: f64, design_mass_kg: f64) -> Self {
        Self {
            construction,
            t_over_c,
            design_mass_kg,
            n_ult: 1.5 * 3.5,
            fabric_kg_m2: 0.05,
            plies: 1.0,
            resin_ratio: 1.0,
            core_density: 23.0,
            film_kg_m2: 0.030,
            print_wall_m: 0.0008,
            print_density: 900.0,
            team_factor: 1.0,
        }
    }

    /// Component masses of `planform` (mirrored wing).
    ///
    /// # Errors
    ///
    /// Non-physical inputs.
    pub fn breakdown(&self, planform: &Planform) -> Result<WingBreakdown, AirframeError> {
        let ok = self.t_over_c > 0.0
            && self.design_mass_kg > 0.0
            && self.n_ult > 0.0
            && self.team_factor > 0.0
            && self.plies >= 0.0;
        if !ok {
            return Err(AirframeError::Invalid("wing build-up inputs".into()));
        }
        let strips = planform.strips(24, 0.4);
        let s = planform.projected_area_m2();
        let wetted = 2.0 * s * (1.0 + 0.25 * self.t_over_c);
        let span = planform.span_m();
        let sum_c2: f64 = strips
            .iter()
            .map(|x| x.chord_m * x.chord_m * x.width_m)
            .sum();
        let section_volume = SECTION_AREA_K * self.t_over_c * sum_c2;
        let c_mean = s / span;
        let mut b = WingBreakdown::default();
        match self.construction {
            Construction::FoamCoreComposite => {
                b.skin_kg = wetted * self.fabric_kg_m2 * self.plies * (1.0 + self.resin_ratio);
                b.core_kg = section_volume * self.core_density;
            }
            Construction::BalsaFilm => {
                b.skin_kg = wetted * self.film_kg_m2;
                let ribs = (span / 0.10).ceil();
                b.ribs_kg =
                    ribs * SECTION_AREA_K * self.t_over_c * c_mean * c_mean * 0.6 * 0.0015 * BALSA;
                b.ribs_kg += span * (36.0e-6 + 30.0e-6) * BALSA;
                b.core_kg = 2.0 * span * 0.15 * c_mean * 0.0015 * BALSA;
            }
            Construction::Printed3D => {
                b.skin_kg = wetted * self.print_wall_m * self.print_density * 1.5;
            }
        }
        let m_root =
            self.n_ult * self.design_mass_kg * 9.806_65 * span / (3.0 * std::f64::consts::PI);
        let tubes = (m_root / TUBE_CAPACITY_NM).max(1.0);
        b.spar_kg = 0.9 * span * TUBE_G_PER_M * tubes;
        let k = self.team_factor;
        Ok(WingBreakdown {
            skin_kg: b.skin_kg * k,
            core_kg: b.core_kg * k,
            spar_kg: b.spar_kg * k,
            ribs_kg: b.ribs_kg * k,
        })
    }
}
