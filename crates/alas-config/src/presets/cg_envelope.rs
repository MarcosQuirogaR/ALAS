// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Manufacturer planning CG envelopes attached to exact preset variants.
//!
//! A planning manual is useful evidence for preliminary weight-and-balance
//! checks, but it is not the operational authority for an individual
//! aeroplane. Keeping the document location and its WBM limitation beside the
//! vertices prevents a caller from turning a plotted curve into an
//! unsupported certification claim. Missing limits stay missing: in
//! particular, interpolation never manufactures an aft flight limit beyond
//! the last value Airbus publishes for the A220 record below.

/// One forward/aft CG-limit pair, in percent mean aerodynamic chord.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CgLimits {
    /// Most-forward published CG position.
    pub forward_pct_mac: f64,
    /// Most-aft published CG position, or `None` where the source omits it.
    pub aft_pct_mac: Option<f64>,
}

/// One mass station of a manufacturer planning envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CgEnvelopeVertex {
    /// Aircraft mass at this vertex, in kilograms.
    pub mass_kg: f64,
    /// In-flight forward and aft limits.
    pub flight: CgLimits,
    /// On-ground forward and aft limits.
    pub ground: CgLimits,
}

/// Operating condition selecting one curve from the planning table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgEnvelopeCondition {
    /// Aeroplane in flight.
    Flight,
    /// Aeroplane on the ground.
    Ground,
}

/// Revision-locked location of a published CG table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CgEnvelopeSource {
    /// Manufacturer document identifier and title.
    pub document: &'static str,
    /// Publication revision or issue date.
    pub revision: &'static str,
    /// Exact table/page or data-module location.
    pub location: &'static str,
}

/// Manufacturer longitudinal frame used to express a planning CG envelope.
///
/// ALAS model geometry retains its own aerodynamic reference values. These
/// source values are used only when comparing the model's nose-relative CG
/// coordinate with a manufacturer curve expressed in percent MAC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanningMacReference {
    /// Primary-source location for the datum and MAC dimensions.
    pub source: CgEnvelopeSource,
    /// Leading edge of mean aerodynamic chord aft of the aircraft nose.
    pub lemac_from_aircraft_nose_m: f64,
    /// Manufacturer mean aerodynamic chord.
    pub mean_aerodynamic_chord_m: f64,
}

/// A manufacturer planning curve that is explicitly subordinate to the WBM.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanningCgEnvelope {
    /// Exact primary-source location for the vertices.
    pub source: CgEnvelopeSource,
    /// Aircraft/configuration applicability stated by the source.
    pub applicability: &'static str,
    /// Operational authority named by the planning document.
    pub controlling_document: &'static str,
    /// Source frame in which the published percent-MAC limits are defined.
    pub mac_reference: PlanningMacReference,
    vertices: &'static [CgEnvelopeVertex],
}

impl PlanningCgEnvelope {
    /// Published vertices in ascending mass order.
    pub fn vertices(&self) -> &'static [CgEnvelopeVertex] {
        self.vertices
    }

    /// Linearly interpolate the published planning limits at `mass_kg`.
    ///
    /// Returns `None` outside the source table's mass range or for a non-finite
    /// mass. An omitted aft limit remains `None` at that vertex and throughout
    /// an interval that would otherwise have to interpolate through it.
    pub fn limits_at(&self, condition: CgEnvelopeCondition, mass_kg: f64) -> Option<CgLimits> {
        let first = self.vertices.first()?;
        let last = self.vertices.last()?;
        if !mass_kg.is_finite() || mass_kg < first.mass_kg || mass_kg > last.mass_kg {
            return None;
        }

        if let Some(vertex) = self.vertices.iter().find(|vertex| {
            let scale = mass_kg.abs().max(vertex.mass_kg.abs()).max(1.0);
            (mass_kg - vertex.mass_kg).abs() <= 8.0 * f64::EPSILON * scale
        }) {
            return Some(select_limits(*vertex, condition));
        }

        let pair = self
            .vertices
            .windows(2)
            .find(|pair| pair[0].mass_kg < mass_kg && mass_kg < pair[1].mass_kg)?;
        let lower = select_limits(pair[0], condition);
        let upper = select_limits(pair[1], condition);
        let fraction = (mass_kg - pair[0].mass_kg) / (pair[1].mass_kg - pair[0].mass_kg);

        Some(CgLimits {
            forward_pct_mac: interpolate(lower.forward_pct_mac, upper.forward_pct_mac, fraction),
            aft_pct_mac: lower
                .aft_pct_mac
                .zip(upper.aft_pct_mac)
                .map(|(lower, upper)| interpolate(lower, upper, fraction)),
        })
    }
}

fn select_limits(vertex: CgEnvelopeVertex, condition: CgEnvelopeCondition) -> CgLimits {
    match condition {
        CgEnvelopeCondition::Flight => vertex.flight,
        CgEnvelopeCondition::Ground => vertex.ground,
    }
}

fn interpolate(lower: f64, upper: f64, fraction: f64) -> f64 {
    lower + fraction * (upper - lower)
}

const A220_300_VERTICES: [CgEnvelopeVertex; 10] = [
    vertex(36_287.0, 12.0, Some(31.0), 14.0, 29.0),
    vertex(54_658.0, 12.0, Some(37.1), 13.3, 35.8),
    vertex(56_699.0, 13.0, Some(37.1), 13.2, 35.8),
    vertex(57_140.0, 13.2, Some(37.0), 13.2, 35.8),
    vertex(58_967.0, 14.1, Some(37.0), 14.1, 35.8),
    vertex(60_781.0, 15.0, Some(36.9), 15.0, 35.8),
    vertex(61_235.0, 15.2, Some(36.9), 15.2, 35.5),
    vertex(65_771.0, 17.5, Some(33.1), 17.5, 32.0),
    vertex(67_585.0, 18.4, Some(31.6), 18.4, 30.6),
    vertex(68_039.0, 18.6, None, 18.6, 30.3),
];

const fn vertex(
    mass_kg: f64,
    flight_forward_pct_mac: f64,
    flight_aft_pct_mac: Option<f64>,
    ground_forward_pct_mac: f64,
    ground_aft_pct_mac: f64,
) -> CgEnvelopeVertex {
    CgEnvelopeVertex {
        mass_kg,
        flight: CgLimits {
            forward_pct_mac: flight_forward_pct_mac,
            aft_pct_mac: flight_aft_pct_mac,
        },
        ground: CgLimits {
            forward_pct_mac: ground_forward_pct_mac,
            aft_pct_mac: Some(ground_aft_pct_mac),
        },
    }
}

pub(super) const A220_300_PLANNING_CG_ENVELOPE: PlanningCgEnvelope = PlanningCgEnvelope {
    source: CgEnvelopeSource {
        document: "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00",
        revision: "May 2026",
        location: "J08-41-02/03, Table 3",
    },
    applicability: "BD-500-1A11, S/N 55001-59999 planning configuration",
    controlling_document: "actual aircraft Weight and Balance Manual (WBM)",
    mac_reference: PlanningMacReference {
        source: CgEnvelopeSource {
            document: "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00",
            revision: "May 2026",
            location: "J06-20-01 p.14; J08-41-03-01 p.2",
        },
        // FS0 is 168.0 in forward of the nose and LEMAC is FS 818.998 in.
        lemac_from_aircraft_nose_m: (818.998 - 168.0) * 0.0254,
        mean_aerodynamic_chord_m: 148.86 * 0.0254,
    },
    vertices: &A220_300_VERTICES,
};

// Each expect is the assertion that a mass deliberately selected from inside
// this static source table resolves to the corresponding published limits.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_a220_table_vertices_are_returned_without_rounding_or_smoothing() {
        let envelope = A220_300_PLANNING_CG_ENVELOPE;
        let low = envelope
            .limits_at(CgEnvelopeCondition::Flight, 36_287.0)
            .expect("published lower vertex");
        let mtow = envelope
            .limits_at(CgEnvelopeCondition::Flight, 67_585.0)
            .expect("published MTOW vertex");
        let ramp = envelope
            .limits_at(CgEnvelopeCondition::Ground, 68_039.0)
            .expect("published MRW ground vertex");

        assert_eq!(low.forward_pct_mac, 12.0);
        assert_eq!(low.aft_pct_mac, Some(31.0));
        assert_eq!(mtow.forward_pct_mac, 18.4);
        assert_eq!(mtow.aft_pct_mac, Some(31.6));
        assert_eq!(ramp.forward_pct_mac, 18.6);
        assert_eq!(ramp.aft_pct_mac, Some(30.3));
    }

    #[test]
    fn a220_planning_frame_converts_the_published_fuselage_stations_from_the_nose() {
        let reference = A220_300_PLANNING_CG_ENVELOPE.mac_reference;

        assert_eq!(reference.lemac_from_aircraft_nose_m, 16.535_349_2);
        assert_eq!(reference.mean_aerodynamic_chord_m, 3.781_044);
        assert_eq!(
            reference.source.location,
            "J06-20-01 p.14; J08-41-03-01 p.2"
        );
    }

    #[test]
    fn interpolation_is_linear_between_adjacent_published_mass_stations() {
        let envelope = A220_300_PLANNING_CG_ENVELOPE;
        let midpoint = (36_287.0 + 54_658.0) / 2.0;

        let flight = envelope
            .limits_at(CgEnvelopeCondition::Flight, midpoint)
            .expect("mass lies inside the table");
        let ground = envelope
            .limits_at(CgEnvelopeCondition::Ground, midpoint)
            .expect("mass lies inside the table");

        assert_eq!(flight.forward_pct_mac, 12.0);
        assert!((flight.aft_pct_mac.expect("published aft bounds") - 34.05).abs() < 1.0e-12);
        assert!((ground.forward_pct_mac - 13.65).abs() < 1.0e-12);
        assert!((ground.aft_pct_mac.expect("published aft bounds") - 32.4).abs() < 1.0e-12);
    }

    #[test]
    fn a_mass_reconstructed_from_component_sums_still_hits_its_table_vertex() {
        let envelope = A220_300_PLANNING_CG_ENVELOPE;
        let reconstructed_mtow = f64::from_bits(67_585.0_f64.to_bits() + 4);
        let limits = envelope
            .limits_at(CgEnvelopeCondition::Flight, reconstructed_mtow)
            .expect("reconstructed MTOW vertex");

        assert_eq!(limits.aft_pct_mac, Some(31.6));
    }

    #[test]
    fn the_missing_mrw_flight_aft_limit_is_not_invented_by_interpolation() {
        let envelope = A220_300_PLANNING_CG_ENVELOPE;
        let ramp = envelope
            .limits_at(CgEnvelopeCondition::Flight, 68_039.0)
            .expect("published MRW forward limit");
        let between_mtow_and_ramp = envelope
            .limits_at(CgEnvelopeCondition::Flight, 67_812.0)
            .expect("mass lies inside the table");

        assert_eq!(ramp.forward_pct_mac, 18.6);
        assert_eq!(ramp.aft_pct_mac, None);
        assert_eq!(between_mtow_and_ramp.aft_pct_mac, None);
    }

    #[test]
    fn interpolation_refuses_extrapolation_and_non_finite_mass() {
        let envelope = A220_300_PLANNING_CG_ENVELOPE;
        for mass_kg in [36_286.0, 68_040.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                envelope.limits_at(CgEnvelopeCondition::Flight, mass_kg),
                None
            );
        }
    }
}
