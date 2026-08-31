// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared physical profiles for cabin cross-sections and solid previews.

use alas_payload::cargo::uld_by_code;
use alas_payload::layout::{ContainerMeta, DeckItem, ItemKind, ItemMeta, OverheadBinType};

/// An item's transverse outline extruded over its longitudinal footprint.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CabinAsset {
    pub(crate) x0: f64,
    pub(crate) x1: f64,
    /// Absolute aircraft-frame `[y, z]` vertices, ordered around the outline.
    pub(crate) profile_yz: Vec<[f64; 2]>,
}

impl CabinAsset {
    pub(crate) fn intersects(&self, station: f64) -> bool {
        station >= self.x0 - 1e-9 && station <= self.x1 + 1e-9
    }

    /// Closed extrusion faces: two end caps and one quad per profile edge.
    pub(crate) fn faces(&self) -> Vec<Vec<[f64; 3]>> {
        let count = self.profile_yz.len();
        if count < 3 {
            return Vec::new();
        }
        let mut faces = Vec::with_capacity(count + 2);
        faces.push(
            self.profile_yz
                .iter()
                .rev()
                .map(|point| [self.x0, point[0], point[1]])
                .collect(),
        );
        faces.push(
            self.profile_yz
                .iter()
                .map(|point| [self.x1, point[0], point[1]])
                .collect(),
        );
        for index in 0..count {
            let next = (index + 1) % count;
            let p0 = self.profile_yz[index];
            let p1 = self.profile_yz[next];
            faces.push(vec![
                [self.x0, p0[0], p0[1]],
                [self.x1, p0[0], p0[1]],
                [self.x1, p1[0], p1[1]],
                [self.x0, p1[0], p1[1]],
            ]);
        }
        faces
    }
}

fn rectangle(item: &DeckItem) -> Vec<[f64; 2]> {
    let half_width = item.width * 0.5;
    let half_height = item.height * 0.5;
    vec![
        [item.y - half_width, item.z - half_height],
        [item.y + half_width, item.z - half_height],
        [item.y + half_width, item.z + half_height],
        [item.y - half_width, item.z + half_height],
    ]
}

fn overhead_bin(item: &DeckItem, kind: OverheadBinType) -> Vec<[f64; 2]> {
    let half = item.width * 0.5;
    let bottom = item.z - item.height * 0.5;
    let top = item.z + item.height * 0.5;
    let height = top - bottom;
    match kind {
        OverheadBinType::Sidewall => vec![
            [item.y - half, bottom + height * 0.22],
            [item.y - half * 0.75, top],
            [item.y + half * 0.75, top],
            [item.y + half, bottom + height * 0.22],
            [item.y + half * 0.55, bottom],
            [item.y - half * 0.55, bottom],
        ],
        OverheadBinType::Center => vec![
            [item.y - half, top],
            [item.y + half, top],
            [item.y + half * 0.80, bottom + height * 0.18],
            [item.y + half * 0.34, bottom],
            [item.y - half * 0.34, bottom],
            [item.y - half * 0.80, bottom + height * 0.18],
        ],
    }
}

fn container_profile(item: &DeckItem, meta: &ContainerMeta) -> Vec<[f64; 2]> {
    let Some(uld) = uld_by_code(meta.uld) else {
        return rectangle(item);
    };
    let z_bottom = item.z - item.height * 0.5;
    uld.contour
        .vertices
        .iter()
        .map(|&[normalized_y, normalized_z]| {
            let normalized_y = if item.y < 0.0 && uld.contour.mirrorable {
                -normalized_y
            } else {
                normalized_y
            };
            [
                item.y + normalized_y * item.width * 0.5,
                z_bottom + normalized_z * item.height,
            ]
        })
        .collect()
}

/// Convert one semantic layout item into a shared render asset.
///
/// Container contour lookup is deliberately isolated here. Until the payload
/// database exposes its contour, its authoritative bounding box is retained.
pub(crate) fn asset_for_item(item: &DeckItem) -> Option<CabinAsset> {
    if !item.length.is_finite()
        || !item.width.is_finite()
        || !item.height.is_finite()
        || item.length <= 0.0
        || item.width <= 0.0
        || item.height <= 0.0
    {
        return None;
    }
    let profile_yz = match (&item.kind, &item.meta) {
        (ItemKind::OverheadBin, ItemMeta::OverheadBin(meta)) => overhead_bin(item, meta.bin_type),
        (ItemKind::Uld | ItemKind::Bag, ItemMeta::Container(meta)) => container_profile(item, meta),
        _ => return None,
    };
    Some(CabinAsset {
        x0: item.x - item.length * 0.5,
        x1: item.x + item.length * 0.5,
        profile_yz,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_payload::layout::{ContainerMeta, OverheadBinMeta};

    fn item(kind: ItemKind, meta: ItemMeta) -> DeckItem {
        DeckItem {
            kind,
            deck: "lower",
            x: 4.0,
            y: 0.3,
            z: -0.8,
            length: 1.56,
            width: 1.53,
            mass: 900.0,
            height: 1.63,
            label: String::new(),
            meta,
        }
    }

    #[test]
    fn each_uld_remains_one_six_face_asset() {
        let asset = asset_for_item(&item(
            ItemKind::Uld,
            ItemMeta::Container(ContainerMeta {
                uld: "AKE",
                fill: 0.6,
                color: "#e74c3c",
                net: Some(800.0),
            }),
        ))
        .expect("ULD has an asset");
        assert_eq!(asset.profile_yz.len(), 4);
        assert_eq!(asset.faces().len(), 6);
        assert!(asset.intersects(4.0));
        assert!(!asset.intersects(5.0));
    }

    #[test]
    fn bins_share_their_six_vertex_section_with_eight_face_extrusion() {
        let asset = asset_for_item(&item(
            ItemKind::OverheadBin,
            ItemMeta::OverheadBin(OverheadBinMeta {
                bin_type: OverheadBinType::Sidewall,
            }),
        ))
        .expect("bin has an asset");
        assert_eq!(asset.profile_yz.len(), 6);
        assert_eq!(asset.faces().len(), 8);
    }

    #[test]
    fn invalid_dimensions_do_not_become_invented_solids() {
        let mut invalid = item(
            ItemKind::Uld,
            ItemMeta::Container(ContainerMeta {
                uld: "AKE",
                fill: 0.6,
                color: "#e74c3c",
                net: Some(800.0),
            }),
        );
        invalid.length = 0.0;
        assert!(asset_for_item(&invalid).is_none());
    }
}
