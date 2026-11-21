// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drawn interior cross-section of a body: where its passenger floor and
//! its underfloor hold sit in the outer section, read from the manufacturer's
//! public section drawings.
//!
//! The same numbers place the cabin floor for the payload layout and bound the
//! hold; the loft's outer skin must contain them, which the payload geometry
//! checks when it builds. Depths are measured down from the outer crown and
//! widths across the section, in metres.

use serde::{Deserialize, Serialize};

/// Share of the outer section height within which a drawn depth must fall:
/// shallower puts it in the crown, deeper below the keel.
pub const SECTION_DEPTH_SHARE: (f64, f64) = (0.3, 0.995);

/// The drawn cabin section of one body.
///
/// A body without a drawing leaves the whole struct unset and takes the class
/// section of `alas-payload`; a body whose drawing gives the floor but not the
/// hold leaves `hold` unset.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CabinSectionConfig {
    /// Depth of the main passenger floor below the outer crown, m.
    pub floor_below_crown_m: Option<f64>,
    /// The underfloor hold's drawn polygon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<HoldSectionConfig>,
    /// The drawn width of the cabin lining at the heights a seat row is held to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cabin_widths: Option<CabinWidthsConfig>,
}

/// The width of the cabin lining, measured on the manufacturer's passenger
/// cross-section at the middle of the constant section, at two heights above
/// the passenger floor: the armrests (25 in, 0.635 m, where the manufacturers
/// print the cabin width) and the shoulders of a seated occupant (the median
/// adult woman's, 1.034 m above the floor).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CabinWidthsConfig {
    /// Lining-to-lining width 0.635 m above the floor, m.
    pub armrest_m: f64,
    /// Lining-to-lining width 1.034 m above the floor, m.
    pub shoulder_m: f64,
}

/// An underfloor hold's drawn polygon: a flat floor, a chamfer to the side
/// walls, vertical side walls and a flat ceiling under the floor beams.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldSectionConfig {
    /// Depth of the hold ceiling (underside of the floor beams) below the
    /// outer crown, m; unset puts it a class floor-beam depth under the cabin
    /// floor (a drawing of the hold alone does not tie it to the crown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceiling_below_crown_m: Option<f64>,
    /// Height from the hold floor to its ceiling, m.
    pub height_m: f64,
    /// Width of the flat hold floor, m.
    pub floor_width_m: f64,
    /// Width between the vertical side walls, m.
    pub side_width_m: f64,
    /// Height of the chamfer from the floor corner up to the side wall, m.
    pub chamfer_height_m: f64,
}

impl CabinSectionConfig {
    /// Why the drawn values are not a physical section of an outer section
    /// `height_m` tall (`None` when the height is not yet known), or `None`.
    pub fn issue(&self, height_m: Option<f64>) -> Option<String> {
        let (low, high) = SECTION_DEPTH_SHARE;
        let in_range = |name: &str, depth: f64| -> Option<String> {
            let bounded = height_m.is_none_or(|h| depth > low * h && depth < high * h);
            if depth.is_finite() && depth > 0.0 && bounded {
                return None;
            }
            Some(format!(
                "{name} {depth:?} m must be finite and between {low} and {high} of the section height {height_m:?} m."
            ))
        };
        if let Some(depth) = self.floor_below_crown_m {
            if let Some(message) = in_range("The floor depth below the crown", depth) {
                return Some(message);
            }
        }
        if let Some(widths) = self.cabin_widths {
            let sane = [widths.armrest_m, widths.shoulder_m]
                .iter()
                .all(|w| w.is_finite() && *w > 0.0)
                && widths.shoulder_m <= widths.armrest_m
                && widths.armrest_m < 1.2 * height_m.unwrap_or(f64::INFINITY);
            if !sane {
                return Some(
                    "The cabin lining widths must be finite and positive, the shoulder width no wider than the armrest width, and narrower than the section."
                        .to_owned(),
                );
            }
        }
        let hold = self.hold?;
        let ceiling = hold.ceiling_below_crown_m;
        let ceiling_issue = ceiling.and_then(|depth| in_range("The hold ceiling depth", depth));
        ceiling_issue.or_else(|| {
            let ordered = match (self.floor_below_crown_m, ceiling) {
                (Some(floor), Some(ceiling)) => floor < ceiling,
                _ => true,
            };
            let sizes = [
                hold.height_m,
                hold.floor_width_m,
                hold.side_width_m,
                hold.chamfer_height_m,
            ]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
                && hold.floor_width_m <= hold.side_width_m
                && hold.chamfer_height_m < hold.height_m;
            let deep_enough = height_m.is_none_or(|h| {
                ceiling.unwrap_or(0.0) + hold.height_m < SECTION_DEPTH_SHARE.1 * h
            });
            (!(ordered && sizes && deep_enough)).then(|| {
                "The hold must lie below the cabin floor and inside the section, with a floor no wider than its side walls and a chamfer lower than the hold."
                    .to_owned()
            })
        })
    }
}
