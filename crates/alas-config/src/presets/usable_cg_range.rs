// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Published CG-envelope widths that set a registered preset's minimum usable
//! CG range at each mass.
//!
//! `DesignRequirements::cg_range_pct_mac` is one number, the clean-sheet
//! default. A registered type's certified (or published operational) envelope
//! is narrow at its heavy corner and wide at mid weights, so a single number
//! either over-asks at the heavy corner or, taken as the heavy-corner minimum,
//! lets an adapted design shrink its mid-weight range far below what the real
//! aircraft offers. The requirement is therefore compared like with like: at
//! each mass the usable-range check is evaluated, the published width at that
//! same mass.

use super::cg_envelope::{CgEnvelopeSource, CgEnvelopeVertex, CgLimits};

/// A published CG envelope used as the mass-indexed minimum usable CG range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsableCgRangeSource {
    /// Exact primary-source location of the envelope.
    pub source: CgEnvelopeSource,
    /// What the curve is (certified AFM limits, a planning curve, airport
    /// planning operational limits) and how it was read.
    pub basis: &'static str,
    /// Manufacturer mean aerodynamic chord on which the source's percent-MAC
    /// limits are defined, in metres. Widths are lengths along the fuselage,
    /// so a model with another chord compares metres, not percentages.
    pub mean_aerodynamic_chord_m: f64,
    vertices: &'static [CgEnvelopeVertex],
}

impl UsableCgRangeSource {
    /// A source over `vertices`, which must be in ascending mass order.
    pub(super) const fn new(
        source: CgEnvelopeSource,
        basis: &'static str,
        mean_aerodynamic_chord_m: f64,
        vertices: &'static [CgEnvelopeVertex],
    ) -> Self {
        Self {
            source,
            basis,
            mean_aerodynamic_chord_m,
            vertices,
        }
    }

    /// Published vertices in ascending mass order.
    pub fn vertices(&self) -> &'static [CgEnvelopeVertex] {
        self.vertices
    }

    /// Published usable CG width at `mass_kg`, in percent of the
    /// manufacturer MAC: the narrower of the in-flight and on-ground
    /// envelopes, each interpolated linearly between the published mass
    /// stations. A curve whose aft limit the source omits at either end of
    /// the interval contributes no width there.
    ///
    /// Outside the published mass range the width at the nearest published
    /// station is returned: the source states nothing beyond its table, and
    /// the end width is the closest published requirement (the gated states
    /// of a registered preset lie inside its table; an adapted design can
    /// stray slightly past either end). `None` for a non-finite mass or when
    /// no curve has an aft limit at that mass.
    pub fn width_pct_mac(&self, mass_kg: f64) -> Option<f64> {
        let first = self.vertices.first()?;
        let last = self.vertices.last()?;
        if !mass_kg.is_finite() {
            return None;
        }
        let mass_kg = mass_kg.clamp(first.mass_kg, last.mass_kg);
        let (lower, upper, fraction) = match self
            .vertices
            .windows(2)
            .find(|pair| pair[0].mass_kg <= mass_kg && mass_kg <= pair[1].mass_kg)
        {
            Some(pair) if pair[1].mass_kg > pair[0].mass_kg => (
                pair[0],
                pair[1],
                (mass_kg - pair[0].mass_kg) / (pair[1].mass_kg - pair[0].mass_kg),
            ),
            Some(pair) => (pair[0], pair[0], 0.0),
            None => (*first, *first, 0.0),
        };
        [
            width(lower.flight, upper.flight, fraction),
            width(lower.ground, upper.ground, fraction),
        ]
        .into_iter()
        .flatten()
        .reduce(f64::min)
    }

    /// [`Self::width_pct_mac`] as a length along the fuselage, in metres.
    pub fn width_m(&self, mass_kg: f64) -> Option<f64> {
        self.width_pct_mac(mass_kg)
            .map(|pct| pct / 100.0 * self.mean_aerodynamic_chord_m)
    }
}

fn width(lower: CgLimits, upper: CgLimits, fraction: f64) -> Option<f64> {
    let aft = lower.aft_pct_mac? + fraction * (upper.aft_pct_mac? - lower.aft_pct_mac?);
    let forward =
        lower.forward_pct_mac + fraction * (upper.forward_pct_mac - lower.forward_pct_mac);
    Some(aft - forward)
}

/// A220-300 (BD-500-1A11): the planning table of
/// [`super::cg_envelope::A220_300_PLANNING_CG_ENVELOPE`], whose MAC equals the
/// preset's published reference MAC.
pub(super) const A220_300: UsableCgRangeSource = UsableCgRangeSource::new(
    CgEnvelopeSource {
        document: "Airbus A220 Aircraft Recovery Publication BD500-3AB48-10400-00",
        revision: "May 2026",
        location: "J08-41-02/03, Table 3",
    },
    "manufacturer planning envelope (in flight and on ground), subordinate to the \
     aircraft's Weight and Balance Manual",
    148.86 * 0.0254,
    &super::cg_envelope::A220_300_VERTICES,
);

const fn station(mass_lb: f64, flight: (f64, Option<f64>), ground: (f64, f64)) -> CgEnvelopeVertex {
    CgEnvelopeVertex {
        mass_kg: mass_lb * 0.453_592_37,
        flight: CgLimits {
            forward_pct_mac: flight.0,
            aft_pct_mac: flight.1,
        },
        ground: CgLimits {
            forward_pct_mac: ground.0,
            aft_pct_mac: Some(ground.1),
        },
    }
}

/// Learjet 60 with the optional 23,500 lb take-off weight: every vertex is a
/// printed fuselage-station/%MAC limit under the figure (values at another
/// curve's breakpoint are linear in weight between printed ones). The chart's
/// weight axis starts at 13,000 lb. No in-flight envelope is published above
/// the 23,500 lb MTOW, so the 23,750 lb ramp station carries no flight aft
/// limit; the ground-handling envelope continues to it.
const LJ60_VERTICES: [CgEnvelopeVertex; 7] = [
    station(13_000.0, (-4.0, Some(17.5)), (-4.0, 33.5)),
    station(16_000.0, (-4.0, Some(17.5)), (-4.0, 33.5)),
    station(16_500.0, (-4.0, Some(19.0)), (-4.0, 33.5)),
    station(18_500.0, (2.0, Some(25.0)), (2.0, 33.5)),
    station(22_987.0, (15.461, Some(25.0)), (15.461, 33.5)),
    station(23_500.0, (17.0, Some(25.0)), (17.0, 25.3)),
    station(23_750.0, (17.75, None), (17.75, 21.3)),
];

/// Learjet 60 (optional 23,500 lb take-off weight, Learjet ECR 3845).
pub(super) const LJ60: UsableCgRangeSource = UsableCgRangeSource::new(
    CgEnvelopeSource {
        document: "Learjet 60 Airplane Flight Manual FM-123",
        revision: "FAA Approved 1-15-93, Change 37",
        location: "Section I, p. 1-7, Figure 1-4 (optional 23,500 lb take-off weight)",
    },
    "certified AFM envelope (in flight and ground handling), read from the printed limits \
     (+-0.1 % MAC); MAC 80.09 in (LEMAC F.S. 365.085 in)",
    80.09 * 0.0254,
    &LJ60_VERTICES,
);

/// DC-10 Series 30/30CF/40/40CF: the operational-limits envelope of the
/// airport-planning landing-gear loading chart, digitised twice from the
/// scanned figure and reconciled (the two reads agree within 0.4 % MAC). One
/// envelope, so flight equals ground. The forward edge has a printed bump
/// near 441,000 lb; the aft edge is constant at 28.7 % MAC from 273,200 lb
/// and slants to 19.8 % MAC at the 583,000 lb ramp weight.
const DC10_VERTICES: [CgEnvelopeVertex; 8] = [
    station(260_200.0, (7.7, Some(25.85)), (7.7, 25.85)),
    station(273_200.0, (7.7, Some(28.7)), (7.7, 28.7)),
    station(406_400.0, (7.7, Some(28.7)), (7.7, 28.7)),
    station(420_700.0, (11.2, Some(28.7)), (11.2, 28.7)),
    station(441_000.0, (11.9, Some(28.7)), (11.9, 28.7)),
    station(458_500.0, (10.7, Some(28.7)), (10.7, 28.7)),
    station(469_300.0, (10.0, Some(27.9)), (10.0, 27.9)),
    station(583_000.0, (9.3, Some(19.8)), (9.3, 19.8)),
];

/// DC-10-30 (572,000 lb option).
pub(super) const DC10_30: UsableCgRangeSource = UsableCgRangeSource::new(
    CgEnvelopeSource {
        document: "DC-10 Airplane Characteristics for Airport Planning, DAC-67803A",
        revision: "Rev A",
        location: "Section 7.4, Figure 7.4.2 (Series 30, 30CF, 40 and 40CF), printed p. 141",
    },
    "airport-planning operational limits, narrower than the certified envelope (so the \
     requirement is conservative), digitised from a scanned chart (+-0.4 % MAC); MAC \
     300.682 in from FAA TCDS A22WE",
    300.682 * 0.0254,
    &DC10_VERTICES,
);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1.0e-9)
    }

    #[test]
    fn the_a220_width_is_the_narrower_envelope_at_each_published_station() {
        // MTOW 67,585 kg: flight 18.4-31.6 (13.2), ground 18.4-30.6 (12.2).
        assert!(close(A220_300.width_pct_mac(67_585.0), 12.2));
        // 58,967 kg: flight 14.1-37.0 (22.9), ground 14.1-35.8 (21.7).
        assert!(close(A220_300.width_pct_mac(58_967.0), 21.7));
        // MRW 68,039 kg: the flight aft limit is omitted, so ground 18.6-30.3.
        assert!(close(A220_300.width_pct_mac(68_039.0), 11.7));
    }

    #[test]
    fn widths_interpolate_inside_and_hold_the_end_station_outside_the_table() {
        // Midway between 65,771 kg (ground 14.5) and 67,585 kg (ground 12.2).
        assert!(close(A220_300.width_pct_mac(66_678.0), 13.35));
        assert!(close(A220_300.width_pct_mac(30_000.0), 15.0));
        assert!(close(A220_300.width_pct_mac(70_000.0), 11.7));
        assert_eq!(A220_300.width_pct_mac(f64::NAN), None);
    }

    #[test]
    fn the_lj60_and_dc10_widths_follow_their_published_envelopes() {
        // LJ60 at the 23,500 lb MTOW: flight 17.0-25.0 (8.0), ground 17.0-25.3.
        assert!(close(LJ60.width_pct_mac(23_500.0 * 0.453_592_37), 8.0));
        // Above MTOW only the ground envelope: 17.75-21.3 at the ramp weight.
        assert!(close(LJ60.width_pct_mac(23_750.0 * 0.453_592_37), 3.55));
        // MLW 19,500 lb: forward 5.0, flight aft 25.0.
        assert!(close(LJ60.width_pct_mac(19_500.0 * 0.453_592_37), 20.0));
        // DC-10 at 406,400 lb: 7.7-28.7.
        assert!(close(DC10_30.width_pct_mac(406_400.0 * 0.453_592_37), 21.0));
        // Narrowest at the ramp weight: 9.3-19.8.
        assert!(close(DC10_30.width_pct_mac(583_000.0 * 0.453_592_37), 10.5));
    }

    #[test]
    fn the_width_in_metres_is_on_the_manufacturer_chord() {
        assert!(close(A220_300.width_m(67_585.0), 0.122 * 148.86 * 0.0254));
    }
}
