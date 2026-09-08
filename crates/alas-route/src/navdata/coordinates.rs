// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Coordinate-bearing X-Plane 600/640 airway records, including radio navaids.
//!
//! Format: https://developer.x-plane.com/article/airway-data-awy-dat-file-format-specification/

use super::*;

impl NavdataGraph {
    /// Parse legacy coordinate-bearing airway endpoints without dropping navaids.
    ///
    /// X-Plane's AwyXP640 specification embeds each endpoint's latitude and
    /// longitude. The fix file is not a complete endpoint catalog: VOR/NDB
    /// endpoints need not appear there. Other formats retain the parity parser.
    pub fn parse_with_airway_coordinates(fix_data: &str, airway_data: &str) -> Self {
        let coordinate_format = airway_data.lines().any(|line| {
            let mut columns = line.split_whitespace();
            matches!(columns.next(), Some("600" | "640"))
                && columns
                    .next()
                    .is_some_and(|value| value.eq_ignore_ascii_case("version"))
        });
        if !coordinate_format {
            return Self::parse(fix_data, airway_data);
        }
        let mut fixes = Vec::new();
        let mut indices = HashMap::new();
        let mut edges: BTreeMap<usize, Vec<(usize, f64)>> = BTreeMap::new();
        for line in airway_data.lines() {
            let columns: Vec<_> = line.split_whitespace().collect();
            if columns.len() < 10 {
                continue;
            }
            let (Ok(lat_a), Ok(lon_a), Ok(lat_b), Ok(lon_b)) = (
                columns[1].parse::<f64>(),
                columns[2].parse::<f64>(),
                columns[4].parse::<f64>(),
                columns[5].parse::<f64>(),
            ) else {
                continue;
            };
            if ![lat_a, lat_b]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 90.0)
                || ![lon_a, lon_b]
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 180.0)
            {
                continue;
            }
            let mut endpoint = |ident: &str, lat: f64, lon: f64| {
                *indices
                    .entry((ident.to_owned(), lat.to_bits(), lon.to_bits()))
                    .or_insert_with(|| {
                        let index = fixes.len();
                        fixes.push(Fix {
                            ident: ident.to_owned(),
                            lat,
                            lon,
                        });
                        index
                    })
            };
            let a = endpoint(columns[0], lat_a, lon_a);
            let b = endpoint(columns[3], lat_b, lon_b);
            let distance = haversine_m(lat_a, lon_a, lat_b, lon_b);
            edges.entry(a).or_default().push((b, distance));
            edges.entry(b).or_default().push((a, distance));
        }
        let connected: Vec<_> = edges.keys().copied().collect();
        let coords_rad = connected
            .iter()
            .map(|&i| (fixes[i].lat.to_radians(), fixes[i].lon.to_radians()))
            .collect();
        Self {
            fixes,
            edges,
            connected,
            coords_rad,
        }
    }
}
