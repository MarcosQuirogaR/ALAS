// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drawn cabin section of a single-deck body: where the passenger floor
//! and the underfloor hold sit in the outer section, and the hold's polygon.
//!
//! A body with a public section drawing carries its measured values in
//! `FuselageConfig::cabin_section`. A body without one takes the class section
//! below, whose ratios are the medians of the drawn bodies. The frozen reference frame,
//! the double-decker and the hump body keep their own deck tables.

use alas_config::{CabinSectionConfig, HoldSectionConfig, SECTION_DEPTH_SHARE};

use super::{CabinGeometry, CabinGeometryError};

/// Ratio of the drawn lining width to the lofted lining's chord at the armrests
/// and at the shoulders, for a body without a drawing: the medians of the drawn
/// bodies. Armrests (five bodies, each a published or vector-measured width):
/// A340 0.988, ATR 72 1.003, E195-E2 1.016, A320 1.026, A220 1.031. Shoulders
/// (the four bodies whose shoulder width is measured; the E195-E2's is a
/// transfer and not counted): A340 0.990, ATR 72 1.005, A320 1.037, A220 1.045.
/// The spread (0.988-1.031 and 0.990-1.045) is the uncertainty of the rule.
const CLASS_LINING_SCALE: [f64; 2] = [1.016, 1.021];

/// Depth of the passenger floor below the outer crown as a share of the
/// section height, for a body without a drawn value: the median of the six
/// jets with an underfloor hold whose drawings were measured (A320 0.582,
/// A220-300 0.618, E195-E2 0.631, 787-9 0.562, A340-300 0.514, DC-10 0.565;
/// median 0.5735).
const FLOOR_DEPTH_RATIO: f64 = 0.57;

/// Standing cabin height under the crown structure, m: the floor-to-inner-crown
/// height of a passenger cabin. The only printed value in the evidence set for a
/// small body is the Embraer E195-E2 cabin height of 2.00 m (APM 5824 fig. 2.19,
/// `refs/E2_APM.pdf`; the internal cabin-datum evidence note). No printed
/// cabin height was found for the ATR 72-600: its outer-crown floor depth is read
/// from the drawing as 2.14 m (evidence table), and the ATR text in
/// `refs/ewade.txt` gives only the 2.570 m cabin circle.
const CABIN_STANDING_HEIGHT_M: f64 = 2.00;

/// Crown allowance between the inner and outer crown, m: the E195-E2 floor depth
/// below the outer crown (2.115 m, APM fig. 2.19 with the printed 3.35 m height)
/// less its 2.00 m cabin height. Not a sourced class value for other bodies.
const CROWN_ALLOWANCE_M: f64 = 0.115;

/// Shallowest passenger floor below the outer crown, m, for a body without a drawn
/// floor depth: the head-room form `h_cabin + t_crown` ([`CABIN_STANDING_HEIGHT_M`]
/// plus [`CROWN_ALLOWANCE_M`]). It is a single-body class value, not a sourced
/// class rule: neither CS-25 nor FAR 25 sets a cabin standing height (25.815 governs
/// aisle width only), and the evidence table's ATR floor (2.14 m below the outer
/// crown) is not reproduced by it.
/// KNOWN LIMIT (open, card 05F, partial: needs drawn small sections): for regional
/// sections of about 2.3-2.7 m this value lowers the floor toward the keel (0.92 H
/// on a 2.3 m body). The real floor depths of the ERJ-145, Saab 2000 and CRJ200
/// class are not in the evidence set, so the regional case is not validated. No
/// registered preset with a default floor is in that range.
const MIN_FLOOR_DEPTH_M: f64 = CABIN_STANDING_HEIGHT_M + CROWN_ALLOWANCE_M;

/// Clearance the floor keeps above the inner lining's keel, m: the floor
/// structure and the seat-leg zone under the seat pan. It is what keeps a small
/// body's default floor inside the lining. At 0.22 m the Learjet 60's lining
/// (1.80 m wide, 1.72 m high, wall 0.09 m) leaves the floor a 1.20 m chord, the
/// width its two-abreast club row (1.22 m over the seat outlines) stands on.
/// ENGINEERING VALUE, not a sourced class rule: no printed floor-to-keel
/// dimension was found for a business jet.
pub(super) const FLOOR_KEEL_CLEARANCE_M: f64 = 0.22;

/// Default passenger floor depth below the outer crown, m, for a body without a
/// drawn floor depth: the 0.57 ratio, raised to [`MIN_FLOOR_DEPTH_M`] and capped
/// at the inner lining height less [`FLOOR_KEEL_CLEARANCE_M`], measured down
/// from the outer crown (`height - wall - clearance`). The cap is the only
/// upper bound: on the Learjet 60 (section 1.904 m tall) the head-room bound
/// of 2.115 m put the floor below the belly, and the former `0.995 * height`
/// cap left it on the keel with no floor chord.
fn default_floor_depth(height: f64, wall: f64) -> f64 {
    let keel_limit = height - wall - FLOOR_KEEL_CLEARANCE_M;
    (FLOOR_DEPTH_RATIO * height)
        .max(MIN_FLOOR_DEPTH_M)
        .min(keel_limit)
}

/// Floor-beam depth (cabin floor to hold ceiling) as a share of the section
/// height: the median of the A340 (0.37 m of 5.64 m), DC-10 (0.33 m of 6.02 m)
/// and E195-E2 (0.14 m of 3.35 m) drawn holds.
const FLOOR_BEAM_RATIO: f64 = 0.055;

/// Section height above which the body takes the wide-body hold ratios, m.
const WIDE_BODY_HEIGHT_M: f64 = 5.0;

/// Hold height as a share of the section height, wide bodies: the median of
/// the A340 (1.71 m, 0.303), DC-10 (1.676 m, 0.278) and 787 (1.695 m, 0.285)
/// drawn holds.
const WIDE_HOLD_HEIGHT_RATIO: f64 = 0.285;

/// Hold height of a single-aisle body without a drawn hold, m: the 46 in
/// (1.168 m) of the LD-3-45W container that family carries (Boeing ULD
/// specification TBC-LL-0035F), the lowest hold that takes it. The A320 AC
/// publishes only the cargo door's 1.24 m clear height, not the compartment's.
const SINGLE_AISLE_HOLD_HEIGHT_M: f64 = 1.168;

/// Flat hold floor width as a share of the section width: the median of the
/// A340 (0.564), DC-10 (0.530) and 787 (0.549) drawn holds.
const FLOOR_WIDTH_RATIO: f64 = 0.549;

/// Width between the hold side walls as a share of the section width: the
/// median of the A340 (0.706), DC-10 (0.692) and 787 (0.723) drawn holds.
const SIDE_WIDTH_RATIO: f64 = 0.706;

/// Chamfer height as a share of the hold height: the median of the A340
/// (0.19), DC-10 (0.27) and 787 (0.20) drawn holds.
const CHAMFER_HEIGHT_RATIO: f64 = 0.20;

/// Narrowest horizontal gap between the flat hold floor and the outer skin
/// among the drawn holds (A340-300: 3.62 m of skin chord, 3.18 m of floor), m.
pub(super) const MIN_HOLD_LINING_M: f64 = 0.22;

/// Most a hold polygon is raised to bring its lower corners inside the lining,
/// m: a bound for the search, far above the few centimetres the wall and frames
/// account for.
const MAX_RAISE_TO_LINING_M: f64 = 0.5;

/// A hold corner is placed this far inside the lining, m, so that it stands
/// strictly inside it whatever the last digits of the lining curve give: a
/// numerical margin, not a design clearance.
const LINING_MARGIN_M: f64 = 0.001;

/// Halvings of that bound: far below a micrometre.
const BISECTIONS: usize = 60;

/// Shallowest hold, m: 0.71 m (28 in), the clear height the Airbus A220 ACP
/// forward-hold clearance section D-D prints from the ceiling to the lowest
/// side-wall point, the shallowest clear height drawn in any hold read (the
/// full A220 hold, section A-A, is 1.05 m deep). A class section that leaves
/// less has no underfloor hold, as on a turboprop.
pub(crate) const MIN_HOLD_HEIGHT_M: f64 = 0.71;

/// A hold's polygon and its place in the outer section, resolved at the
/// middle of the constant cabin section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldSection {
    /// Half the width of the flat hold floor, m.
    pub floor_half_m: f64,
    /// Half the width between the side walls, m.
    pub side_half_m: f64,
    /// Half the width the side wall reaches above the chamfer, m, bounded by the skin
    /// less the lining. Equal to `side_half_m` for a drawn hold (vertical walls); a class
    /// hold's wall follows the skin outward above its foot, so the container tops of a
    /// certified type (A320 LD3-45W, 96 in, FAST 39 p.23) are not cut by a vertical wall
    /// standing at the skin's narrowest point.
    pub wall_top_half_m: f64,
    /// Height of the chamfer from the floor corner to the side wall, m.
    pub chamfer_height_m: f64,
    /// Hold height, floor to ceiling, m.
    pub height_m: f64,
    /// Horizontal gap between the flat floor's edge and the outer skin, m:
    /// the lining the polygon is kept inside the skin by where the section
    /// tapers.
    pub lining_m: f64,
}

impl HoldSection {
    /// Half the hold's width `height_m` above its floor, m; zero below it.
    pub fn half_width_above_floor(&self, height_m: f64) -> f64 {
        if height_m < 0.0 {
            0.0
        } else if height_m >= self.chamfer_height_m {
            self.wall_top_half_m
        } else {
            self.floor_half_m
                + (self.side_half_m - self.floor_half_m) * height_m / self.chamfer_height_m
        }
    }
}

impl CabinGeometry {
    /// Place a single-deck body's passenger floor and underfloor hold from its
    /// drawn section (`drawn`) or the class section, and give the hold its
    /// polygon. Unset or a hold that fits nowhere leaves the body without a
    /// polygon and the hold rigidly below the floor, as a turboprop has it.
    pub(super) fn apply_cabin_section(
        &mut self,
        drawn: Option<CabinSectionConfig>,
    ) -> Result<(), CabinGeometryError> {
        let x_ref = 0.5 * (self.cabin_start_x + self.cabin_end_x);
        let height = self.height_at(x_ref);
        let width = self.width_at(x_ref);
        let zc = self.zc_at(x_ref);
        let crown = zc + 0.5 * height;
        let (low, high) = SECTION_DEPTH_SHARE;
        let check = |name: &str, depth: f64| -> Result<f64, CabinGeometryError> {
            if depth.is_finite() && depth > low * height && depth < high * height {
                Ok(depth)
            } else {
                Err(CabinGeometryError::SectionOutsideSkin(format!(
                    "{name} {depth:.3} m, section {height:.3} m tall"
                )))
            }
        };
        let floor_depth = match drawn.and_then(|section| section.floor_below_crown_m) {
            Some(depth) => check("floor depth", depth)?,
            None => default_floor_depth(height, self.wall),
        };
        let ihh = self.internal_half_height(x_ref);
        let frac_of = |depth: f64| (crown - depth - zc) / ihh;
        let Some(main) = self.passenger_decks.first_mut() else {
            return Ok(());
        };
        let shift = frac_of(floor_depth) - main.floor_frac;
        main.floor_frac += shift;
        // The armrest and shoulder chords are the cabin width the drawings
        // print (the A320's 3.63 m at 25 in); the table's deck allowance on top
        // of them would count the lining twice.
        main.width_factor = 1.0;
        self.floor_datum_applied = true;
        // The lining the seats are held to: the drawn widths at the armrests
        // and at the shoulders, kept as ratios to the lofted lining so that
        // the nose and tail taper still narrows them.
        let floor_ref = crown - floor_depth;
        let scale_at = |above: f64, drawn: Option<f64>, class: f64| {
            drawn.map_or(class, |width| {
                let chord = self.lining_chord_at_z(x_ref, floor_ref + above);
                if chord > 0.0 {
                    width / chord
                } else {
                    class
                }
            })
        };
        let widths = drawn.and_then(|section| section.cabin_widths);
        self.lining_scale = [
            scale_at(
                super::ARMREST_ABOVE_FLOOR_M,
                widths.map(|w| w.armrest_m),
                CLASS_LINING_SCALE[0],
            ),
            scale_at(
                super::LINING_SHOULDER_READ_ABOVE_FLOOR_M,
                widths.map(|w| w.shoulder_m),
                CLASS_LINING_SCALE[1],
            ),
        ];
        // The baseline hold follows the floor rigidly unless a polygon replaces
        // it below.
        self.lower_deck.floor_frac += shift;
        self.lower_deck.ceil_frac += shift;

        let hold = match drawn.and_then(|section| section.hold) {
            Some(config) => Some(self.drawn_hold(&config, floor_depth, height, crown, x_ref)?),
            None => self.class_hold(floor_depth, height, width, crown, x_ref),
        };
        // Whether the body has an underfloor hold at all follows from the
        // section alone: a drawn polygon, or a class hold that fits.
        self.underfloor_hold = hold.is_some();
        if let Some((hold, ceiling_depth)) = hold {
            self.lower_deck.floor_frac = frac_of(ceiling_depth + hold.height_m);
            self.lower_deck.ceil_frac = frac_of(ceiling_depth);
            self.lower_deck.width_factor = 1.0;
            self.hold = Some(hold);
        }
        Ok(())
    }

    /// A drawn hold, checked against the outer skin.
    fn drawn_hold(
        &self,
        config: &HoldSectionConfig,
        floor_depth: f64,
        height: f64,
        crown: f64,
        x_ref: f64,
    ) -> Result<(HoldSection, f64), CabinGeometryError> {
        let ceiling_depth = config
            .ceiling_below_crown_m
            .unwrap_or(floor_depth + FLOOR_BEAM_RATIO * height);
        let hold = HoldSection {
            floor_half_m: 0.5 * config.floor_width_m,
            side_half_m: 0.5 * config.side_width_m,
            wall_top_half_m: 0.5 * config.side_width_m,
            chamfer_height_m: config.chamfer_height_m,
            height_m: config.height_m,
            lining_m: 0.0,
        };
        // A printed ceiling depth is a datum of the drawing and stays where it
        // is; an estimated one (the class floor-beam depth) moves with the hold.
        let printed_ceiling = config.ceiling_below_crown_m.is_some();
        self.place_in_lining(hold, ceiling_depth, floor_depth, crown, x_ref, printed_ceiling)
            .ok_or_else(|| {
                CabinGeometryError::SectionOutsideSkin(format!(
                    "the hold ({:.3} m wide at its floor, {:.3} m high) is wider than the lined section {ceiling_depth:.3} m below the crown",
                    config.floor_width_m, config.height_m
                ))
            })
    }

    /// The class hold of a body without a drawing, `None` when the section
    /// leaves no room for one.
    fn class_hold(
        &self,
        floor_depth: f64,
        height: f64,
        width: f64,
        crown: f64,
        x_ref: f64,
    ) -> Option<(HoldSection, f64)> {
        let hold_height = if height > WIDE_BODY_HEIGHT_M {
            WIDE_HOLD_HEIGHT_RATIO * height
        } else {
            SINGLE_AISLE_HOLD_HEIGHT_M
        };
        let ceiling_depth = floor_depth + FLOOR_BEAM_RATIO * height;
        let hold_floor_depth = ceiling_depth + hold_height;
        if hold_height < MIN_HOLD_HEIGHT_M || hold_floor_depth >= SECTION_DEPTH_SHARE.1 * height {
            return None;
        }
        let chamfer = CHAMFER_HEIGHT_RATIO * hold_height;
        let skin = |depth: f64| self.outer_half_width(x_ref, crown - depth);
        // Never wider than the skin leaves room for, which is what holds a
        // narrow body's hold to its own section.
        let floor_half =
            (0.5 * FLOOR_WIDTH_RATIO * width).min(skin(hold_floor_depth) - MIN_HOLD_LINING_M);
        let side_half = (0.5 * SIDE_WIDTH_RATIO * width)
            .min(skin(hold_floor_depth - chamfer) - MIN_HOLD_LINING_M)
            .max(floor_half);
        if floor_half <= 0.0 {
            return None;
        }
        let hold = HoldSection {
            floor_half_m: floor_half,
            side_half_m: side_half,
            wall_top_half_m: (0.5 * SIDE_WIDTH_RATIO * width).max(side_half),
            chamfer_height_m: chamfer,
            height_m: hold_height,
            lining_m: 0.0,
        };
        self.place_in_lining(hold, ceiling_depth, floor_depth, crown, x_ref, false)
    }

    /// `hold` standing inside the lined interior: the polygon's lower corners,
    /// the floor edge and the foot of the side wall, must lie within the
    /// lining, not only within the outer skin, which the wall and the frames
    /// make a few centimetres thicker. A hold whose corners stand outside is
    /// raised by the least amount that brings them in, keeping its height
    /// (a printed or class dimension) and moving its ceiling up with it; the
    /// ceiling stops a floor-beam depth
    /// ([`MIN_DECK_SEPARATION_FRAC`](super::MIN_DECK_SEPARATION_FRAC) of the
    /// inner half-height) below the cabin floor, and the hold gets shallower
    /// by what is left. A hold whose ceiling depth the drawing prints
    /// (`printed_ceiling`) is a complete printed polygon and datum and is left
    /// as drawn: where it stands a few centimetres outside the modelled
    /// lining, the lining is the approximation. `None` when the hold cannot
    /// stand inside the lining.
    fn place_in_lining(
        &self,
        hold: HoldSection,
        ceiling_depth: f64,
        floor_depth: f64,
        crown: f64,
        x_ref: f64,
        printed_ceiling: bool,
    ) -> Option<(HoldSection, f64)> {
        if printed_ceiling {
            return self
                .seat_in_skin(hold, ceiling_depth, crown, x_ref)
                .map(|hold| (hold, ceiling_depth));
        }
        let floor_z = crown - ceiling_depth - hold.height_m;
        let inside = |raise: f64| {
            let z = floor_z + raise;
            self.lining_half_width_at(x_ref, z) >= hold.floor_half_m + LINING_MARGIN_M
                && self.lining_half_width_at(x_ref, z + hold.chamfer_height_m)
                    >= hold.side_half_m + LINING_MARGIN_M
        };
        let raise = if inside(0.0) {
            0.0
        } else {
            let (mut low, mut high) = (0.0, MAX_RAISE_TO_LINING_M);
            if !inside(high) {
                return None;
            }
            for _ in 0..BISECTIONS {
                let mid = 0.5 * (low + high);
                if inside(mid) {
                    high = mid;
                } else {
                    low = mid;
                }
            }
            high
        };
        let (hold, ceiling_depth) = if raise > 0.0 {
            let floor_below_crown = ceiling_depth + hold.height_m - raise;
            let lowest_ceiling =
                floor_depth + super::MIN_DECK_SEPARATION_FRAC * self.internal_half_height(x_ref);
            let ceiling = (ceiling_depth - raise).max(lowest_ceiling);
            let height = floor_below_crown - ceiling;
            if height <= 0.0 {
                return None;
            }
            (
                HoldSection {
                    height_m: height,
                    ..hold
                },
                ceiling,
            )
        } else {
            (hold, ceiling_depth)
        };
        self.seat_in_skin(hold, ceiling_depth, crown, x_ref)
            .map(|hold| (hold, ceiling_depth))
    }

    /// `hold` with its lining measured against the skin at the hold floor, or
    /// `None` when its polygon pokes through the skin there.
    fn seat_in_skin(
        &self,
        hold: HoldSection,
        ceiling_depth: f64,
        crown: f64,
        x_ref: f64,
    ) -> Option<HoldSection> {
        let floor_z = crown - ceiling_depth - hold.height_m;
        let lining = self.outer_half_width(x_ref, floor_z) - hold.floor_half_m;
        let side_z = floor_z + hold.chamfer_height_m;
        let side_lining = self.outer_half_width(x_ref, side_z) - hold.side_half_m;
        (lining >= 0.0 && side_lining >= 0.0).then_some(HoldSection {
            lining_m: lining.min(side_lining),
            ..hold
        })
    }

    /// The cross-section area of `deck` at `x`, m2: the drawn polygon under the
    /// local clear height for a hold that has one, the usable width times the
    /// height otherwise.
    pub fn section_area(&self, deck: &super::DeckSpec, x: f64) -> f64 {
        let height = self.deck_height(deck, x);
        if self.hold.is_none() || deck.name != crate::layout::LOWER {
            return self.usable_width(deck, x) * height;
        }
        let floor = self.floor_z(deck, x);
        let steps = 16;
        let dz = height / steps as f64;
        (0..steps)
            .map(|i| {
                let z = floor + (i as f64 + 0.5) * dz;
                2.0 * self.hold_half_width(x, z).unwrap_or(0.0) * dz
            })
            .sum()
    }

    /// Half the hold's width at station `x` and height `z`, or `None` above the
    /// passenger floor and on a body without a hold polygon. The polygon is
    /// followed down the belly with the hold floor and is never wider than the
    /// outer skin less the lining measured where the polygon was fitted.
    pub(super) fn hold_half_width(&self, x: f64, z: f64) -> Option<f64> {
        let hold = self.hold.as_ref()?;
        let main = self.passenger_decks.first()?;
        if z >= self.floor_z(main, x) {
            return None;
        }
        let above_floor = z - self.floor_z(&self.lower_deck, x);
        let skin = (self.outer_half_width(x, z) - hold.lining_m).max(0.0);
        Some(hold.half_width_above_floor(above_floor).min(skin))
    }
}

#[cfg(test)]
mod floor_depth_tests {
    use super::{default_floor_depth, FLOOR_KEEL_CLEARANCE_M, MIN_FLOOR_DEPTH_M};

    const WALL_M: f64 = 0.15;

    /// The ratio branch: 0.57 of a 5.0 m section (2.85 m) clears the head-room bound.
    #[test]
    fn a_large_section_takes_the_ratio() {
        let depth = default_floor_depth(5.0, WALL_M);
        assert!((depth - 0.57 * 5.0).abs() < 1e-12, "{depth}");
    }

    /// The bound branch: a 3.0 m section gives 1.71 m from the ratio, below the bound.
    #[test]
    fn a_mid_section_takes_the_head_room_bound() {
        let depth = default_floor_depth(3.0, WALL_M);
        assert_eq!(depth, MIN_FLOOR_DEPTH_M);
    }

    /// The cap branch. A 2.0 m section cannot hold the 2.115 m bound; the floor
    /// is capped at the inner lining height less the keel clearance, measured
    /// from the outer crown. Before this cap the limit was 0.995 of the height
    /// (1.99 m), which put the floor on the lining's keel; the Learjet 60
    /// (1.904 m, wall 0.09 m) is the registered case.
    #[test]
    fn a_small_section_is_capped_at_the_lining_less_the_keel_clearance() {
        let depth = default_floor_depth(2.0, WALL_M);
        let expected = 2.0 - WALL_M - FLOOR_KEEL_CLEARANCE_M;
        assert!((depth - expected).abs() < 1e-12, "{depth}");
        assert!(depth < MIN_FLOOR_DEPTH_M);
        let lj60 = default_floor_depth(1.904, 0.09);
        assert!(
            (lj60 - (1.904 - 0.09 - FLOOR_KEEL_CLEARANCE_M)).abs() < 1e-12,
            "{lj60}"
        );
    }

    /// The floor always leaves the clearance above the inner keel, for any wall.
    #[test]
    fn the_floor_never_sinks_below_the_keel_clearance() {
        for (height, wall) in [(1.9, 0.09), (2.0, 0.15), (2.4, 0.15), (2.8, 0.06)] {
            let inner_keel_depth = height - wall;
            let depth = default_floor_depth(height, wall);
            assert!(
                depth <= inner_keel_depth - FLOOR_KEEL_CLEARANCE_M + 1e-12,
                "{height} {wall}"
            );
        }
    }
}
