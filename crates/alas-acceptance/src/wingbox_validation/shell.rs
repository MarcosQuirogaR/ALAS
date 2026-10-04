// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Straight, symmetric-mid-plane shell check with free cross-section deformation.
//!
//! Coordinates are SI: x is chordwise, y is span, and z is up. The transformed
//! area centroid is the straight reference line. Covers/webs are CQUAD4, and
//! separate rectangular CBAR caps lie at their physical thickness centroids.
//! RBE3 interpolation transfers vertical station resultants through the spar
//! endpoints without adding section stiffness. This check includes shear,
//! Poisson, shear-lag and taper effects;
//! the reference line need not coincide with the section shear centre.

use std::io;

use alas_struct::nastran95::{Card, ContinuationTags, Field};

use super::model::Model;

const FIRST_PHYSICAL_GRID: i64 = 1001;
const MAX_GRIDS: usize = 5000;
const DEFAULT_DIVISIONS: usize = 4;

struct Station {
    upper: Vec<i64>,
    lower: Vec<i64>,
    webs: Vec<Vec<i64>>,
    physical: Vec<i64>,
}

/// Export a shell check with four subdivisions per cover bay and web depth.
pub(super) fn deck(model: &Model) -> io::Result<String> {
    deck_with_refinement(model, DEFAULT_DIVISIONS)
}

/// Export a cross-section mesh within the installed student solver grid limit.
pub(super) fn deck_with_refinement(model: &Model, divisions: usize) -> io::Result<String> {
    validate(model, divisions)?;
    let mut text = format!(
        "SOL 101\nCEND\nTITLE = {} STRAIGHT SHELL WING BOX\nECHO = NONE\nSPC = 1\nLOAD = 1\nDISPLACEMENT(PRINT) = ALL\nSTRESS(PRINT) = ALL\nFORCE(PRINT) = ALL\nSPCFORCES(PRINT) = ALL\nBEGIN BULK\n",
        model.name
    );
    let mut tags = ContinuationTags::new();
    card(
        &mut text,
        &mut tags,
        "PARAM",
        vec![Field::Text("POST"), Field::Int(-1)],
    );
    for (index, material) in [model.skin, model.web, model.cap].into_iter().enumerate() {
        card(
            &mut text,
            &mut tags,
            "MAT1",
            vec![
                Field::Int(index as i64 + 1),
                Field::Real(material.e_pa),
                Field::Real(material.g_pa()),
                Field::Real(material.nu),
            ],
        );
    }
    card(
        &mut text,
        &mut tags,
        "PSHELL",
        vec![
            Field::Int(1),
            Field::Int(1),
            Field::Real(model.sizing.t_skin),
            Field::Int(1),
            Field::Blank,
            Field::Int(1),
        ],
    );
    for (index, spar) in model.sizing.spars.iter().enumerate() {
        card(
            &mut text,
            &mut tags,
            "PSHELL",
            vec![
                Field::Int(index as i64 + 2),
                Field::Int(2),
                Field::Real(spar.t_web),
                Field::Int(2),
                Field::Blank,
                Field::Int(2),
            ],
        );
    }
    let mut next_grid = FIRST_PHYSICAL_GRID;
    let stations: Vec<_> = (0..model.case.y.len())
        .map(|index| {
            station(
                model,
                index,
                divisions,
                &mut next_grid,
                &mut text,
                &mut tags,
            )
        })
        .collect();
    let mut root = vec![Field::Int(1), Field::Text("123456")];
    root.extend(stations[0].physical.iter().map(|&node| Field::Int(node)));
    card(&mut text, &mut tags, "SPC1", root);

    let mut eid = 1_i64;
    for (index, nodes) in stations.iter().enumerate() {
        let monitor = index as i64 + 1;
        grid(
            &mut text,
            &mut tags,
            monitor,
            [0.0, model.case.y[index], 0.0],
        );
        card(
            &mut text,
            &mut tags,
            "SPC1",
            vec![Field::Int(1), Field::Text("456"), Field::Int(monitor)],
        );
        // Spar endpoints introduce a beam resultant, rather than pressure on
        // the un-ribbed covers. Including interior cover nodes would mix local
        // panel bending into both the load transfer and the monitored motion.
        // Only reference translations are dependent; section warping is free.
        let mut fields = vec![
            Field::Int(eid),
            Field::Blank,
            Field::Int(monitor),
            Field::Text("123"),
            Field::Real(1.0),
            Field::Text("123"),
        ];
        fields.extend(spar_nodes(nodes, divisions).into_iter().map(Field::Int));
        card(&mut text, &mut tags, "RBE3", fields);
        eid += 1;
    }
    let mut pid = model.sizing.spars.len() as i64 + 2;
    for (segment, pair) in stations.windows(2).enumerate() {
        for panel in 0..pair[0].upper.len() - 1 {
            quad(
                &mut text,
                &mut tags,
                &mut eid,
                1,
                [
                    pair[0].upper[panel + 1],
                    pair[1].upper[panel + 1],
                    pair[1].upper[panel],
                    pair[0].upper[panel],
                ],
            );
            quad(
                &mut text,
                &mut tags,
                &mut eid,
                1,
                [
                    pair[0].lower[panel],
                    pair[1].lower[panel],
                    pair[1].lower[panel + 1],
                    pair[0].lower[panel + 1],
                ],
            );
        }
        for (spar_index, spar) in model.sizing.spars.iter().enumerate() {
            for level in 0..divisions {
                quad(
                    &mut text,
                    &mut tags,
                    &mut eid,
                    spar_index as i64 + 2,
                    [
                        pair[0].webs[spar_index][level],
                        pair[1].webs[spar_index][level],
                        pair[1].webs[spar_index][level + 1],
                        pair[0].webs[spar_index][level + 1],
                    ],
                );
            }
            let width = 0.5 * (spar.w_cap[segment] + spar.w_cap[segment + 1]);
            let thickness = 0.5 * (spar.t_cap[segment] + spar.t_cap[segment + 1]);
            // BAR DIM1 is width; DIM2 follows the orientation vector, +z.
            card(
                &mut text,
                &mut tags,
                "PBARL",
                vec![
                    Field::Int(pid),
                    Field::Int(3),
                    Field::Blank,
                    Field::Text("BAR"),
                    Field::Blank,
                    Field::Blank,
                    Field::Blank,
                    Field::Blank,
                    Field::Real(width),
                    Field::Real(thickness),
                ],
            );
            let chord_node = spar_index * divisions;
            for (ga, gb, sign) in [
                (pair[0].upper[chord_node], pair[1].upper[chord_node], -1.0),
                (pair[0].lower[chord_node], pair[1].lower[chord_node], 1.0),
            ] {
                card(
                    &mut text,
                    &mut tags,
                    "CBAR",
                    vec![
                        Field::Int(eid),
                        Field::Int(pid),
                        Field::Int(ga),
                        Field::Int(gb),
                        Field::Real(0.0),
                        Field::Real(0.0),
                        Field::Real(1.0),
                        Field::Text("GGG"),
                        Field::Blank,
                        Field::Blank,
                        Field::Real(0.0),
                        Field::Real(0.0),
                        Field::Real(sign * spar.t_cap[segment] / 2.0),
                        Field::Real(0.0),
                        Field::Real(0.0),
                        Field::Real(sign * spar.t_cap[segment + 1] / 2.0),
                    ],
                );
                eid += 1;
            }
            pid += 1;
        }
    }
    let forces = station_forces(&model.case.y, &model.case.q_net, &model.point_forces)?;
    for (index, force) in forces.into_iter().enumerate() {
        if force != 0.0 {
            card(
                &mut text,
                &mut tags,
                "FORCE",
                vec![
                    Field::Int(1),
                    Field::Int(index as i64 + 1),
                    Field::Int(0),
                    Field::Real(force),
                    Field::Real(0.0),
                    Field::Real(0.0),
                    Field::Real(1.0),
                ],
            );
        }
    }
    text.push_str("ENDDATA\n");
    Ok(text)
}

fn validate(model: &Model, divisions: usize) -> io::Result<()> {
    let n = model.case.y.len();
    let s = model.sizing.spars.len();
    if divisions == 0
        || divisions > MAX_GRIDS
        || n < 2
        || n >= FIRST_PHYSICAL_GRID as usize
        || !(2..=MAX_GRIDS).contains(&s)
    {
        return Err(io::Error::other(
            "invalid shell station or refinement layout",
        ));
    }
    let physical = 2 * ((s - 1) * divisions + 1) + s * (divisions - 1);
    if n.checked_mul(physical + 1)
        .is_none_or(|count| count > MAX_GRIDS)
    {
        return Err(io::Error::other("shell refinement exceeds 5000 grids"));
    }
    if model.sizing.chord.len() != n
        || model.case.q_net.len() != n
        || !model.sizing.t_skin.is_finite()
        || model.sizing.t_skin <= 0.0
        || !model
            .case
            .y
            .windows(2)
            .all(|p| p[0].is_finite() && p[1].is_finite() && p[1] > p[0])
        || !model.sizing.chord.iter().all(|x| x.is_finite() && *x > 0.0)
        || !model
            .sizing
            .spars
            .windows(2)
            .all(|p| p[1].chord_fraction > p[0].chord_fraction)
    {
        return Err(io::Error::other("invalid shell dimensions or station grid"));
    }
    for spar in &model.sizing.spars {
        if !spar.chord_fraction.is_finite()
            || !spar.t_web.is_finite()
            || spar.t_web <= 0.0
            || [
                spar.h.len(),
                spar.w_cap.len(),
                spar.t_cap.len(),
                spar.a_cap.len(),
            ]
            .iter()
            .any(|&len| len != n)
        {
            return Err(io::Error::other("invalid shell spar geometry"));
        }
        for i in 0..n {
            if !spar.h[i].is_finite() || spar.h[i] <= 0.0 {
                return Err(io::Error::other(
                    "straight shell check requires full-span spars",
                ));
            }
            if !spar.w_cap[i].is_finite()
                || spar.w_cap[i] <= 0.0
                || !spar.t_cap[i].is_finite()
                || spar.t_cap[i] <= 0.0
                || !spar.a_cap[i].is_finite()
                || spar.a_cap[i] <= 0.0
                || 2.0 * spar.t_cap[i] >= spar.h[i]
            {
                return Err(io::Error::other("invalid shell cap dimensions"));
            }
        }
    }
    for material in [model.skin, model.web, model.cap] {
        if !material.e_pa.is_finite()
            || material.e_pa <= 0.0
            || !material.nu.is_finite()
            || material.nu <= -1.0
            || material.nu >= 0.5
        {
            return Err(io::Error::other("invalid isotropic shell material"));
        }
    }
    Ok(())
}

fn station(
    model: &Model,
    index: usize,
    divisions: usize,
    next_grid: &mut i64,
    text: &mut String,
    tags: &mut ContinuationTags,
) -> Station {
    let mut nodes = Station {
        upper: Vec::new(),
        lower: Vec::new(),
        webs: Vec::new(),
        physical: Vec::new(),
    };
    let chord = model.sizing.chord[index];
    let center = centroid(model, index);
    let y = model.case.y[index];
    for (bay, pair) in model.sizing.spars.windows(2).enumerate() {
        let end = if bay + 2 == model.sizing.spars.len() {
            divisions + 1
        } else {
            divisions
        };
        for panel in 0..end {
            let fraction = panel as f64 / divisions as f64;
            let x = chord
                * (pair[0].chord_fraction * (1.0 - fraction) + pair[1].chord_fraction * fraction)
                - center;
            let z = 0.5 * (pair[0].h[index] * (1.0 - fraction) + pair[1].h[index] * fraction);
            for (side, list) in [(1.0, &mut nodes.upper), (-1.0, &mut nodes.lower)] {
                grid(text, tags, *next_grid, [x, y, side * z]);
                list.push(*next_grid);
                nodes.physical.push(*next_grid);
                *next_grid += 1;
            }
        }
    }
    for (spar_index, spar) in model.sizing.spars.iter().enumerate() {
        let chord_node = spar_index * divisions;
        let mut web = vec![nodes.lower[chord_node]];
        for level in 1..divisions {
            let z = spar.h[index] * (level as f64 / divisions as f64 - 0.5);
            grid(
                text,
                tags,
                *next_grid,
                [spar.chord_fraction * chord - center, y, z],
            );
            web.push(*next_grid);
            nodes.physical.push(*next_grid);
            *next_grid += 1;
        }
        web.push(nodes.upper[chord_node]);
        nodes.webs.push(web);
    }
    nodes
}

fn centroid(model: &Model, station: usize) -> f64 {
    let mut area = 0.0;
    let mut first = 0.0;
    for spar in &model.sizing.spars {
        let a = 2.0 * spar.a_cap[station]
            + model.web.e_pa / model.cap.e_pa * spar.t_web * spar.h[station];
        area += a;
        first += a * spar.chord_fraction * model.sizing.chord[station];
    }
    for pair in model.sizing.spars.windows(2) {
        let x1 = pair[0].chord_fraction * model.sizing.chord[station];
        let x2 = pair[1].chord_fraction * model.sizing.chord[station];
        let a = 2.0 * model.skin.e_pa / model.cap.e_pa * model.sizing.t_skin * (x2 - x1);
        area += a;
        first += a * (x1 + x2) / 2.0;
    }
    first / area
}

fn spar_nodes(nodes: &Station, divisions: usize) -> Vec<i64> {
    nodes
        .upper
        .iter()
        .zip(&nodes.lower)
        .step_by(divisions)
        .flat_map(|(&upper, &lower)| [upper, lower])
        .collect()
}

fn station_forces(y: &[f64], q: &[f64], points: &[(f64, f64)]) -> io::Result<Vec<f64>> {
    if y.len() < 2
        || y.len() != q.len()
        || !q.iter().all(|x| x.is_finite())
        || !y
            .windows(2)
            .all(|p| p[0].is_finite() && p[1].is_finite() && p[1] > p[0])
    {
        return Err(io::Error::other("invalid shell loading"));
    }
    let mut forces = vec![0.0; y.len()];
    for (index, pair) in y.windows(2).enumerate() {
        // Equivalent nodal forces preserve the segment average's integrated
        // force and first moment. No nodal beam moments are imposed on shells.
        let force = (q[index] + q[index + 1]) * (pair[1] - pair[0]) / 4.0;
        forces[index] += force;
        forces[index + 1] += force;
    }
    for &(position, force) in points {
        if !position.is_finite() || !force.is_finite() {
            return Err(io::Error::other("nonfinite shell point load"));
        }
        let index = y
            .windows(2)
            .position(|p| position >= p[0] && position <= p[1])
            .ok_or_else(|| io::Error::other("shell point load outside the modeled span"))?;
        let fraction = (position - y[index]) / (y[index + 1] - y[index]);
        forces[index] += force * (1.0 - fraction);
        forces[index + 1] += force * fraction;
    }
    Ok(forces)
}

fn grid(text: &mut String, tags: &mut ContinuationTags, id: i64, xyz: [f64; 3]) {
    card(
        text,
        tags,
        "GRID",
        vec![
            Field::Int(id),
            Field::Blank,
            Field::Real(xyz[0]),
            Field::Real(xyz[1]),
            Field::Real(xyz[2]),
        ],
    );
}

fn quad(text: &mut String, tags: &mut ContinuationTags, eid: &mut i64, pid: i64, nodes: [i64; 4]) {
    let mut fields = vec![Field::Int(*eid), Field::Int(pid)];
    fields.extend(nodes.map(Field::Int));
    card(text, tags, "CQUAD4", fields);
    *eid += 1;
}

fn card(text: &mut String, tags: &mut ContinuationTags, name: &str, fields: Vec<Field>) {
    Card::new(name, fields).render(text, tags);
}

#[cfg(test)]
mod tests {
    use super::{spar_nodes, station_forces, Station};

    #[test]
    fn station_resultant_uses_spar_endpoints_and_excludes_cover_interiors() {
        let nodes = Station {
            upper: (1..=7).collect(),
            lower: (11..=17).collect(),
            webs: Vec::new(),
            physical: (1..=7).chain(11..=17).collect(),
        };
        assert_eq!(spar_nodes(&nodes, 3), [1, 11, 4, 14, 7, 17]);
    }

    #[test]
    fn constant_distributed_and_signed_point_loads_preserve_force_and_root_moment() {
        let y = [0.0, 0.8, 2.1, 4.0];
        let q = [12.0; 4];
        let point = (1.3, -7.0);
        let force = station_forces(&y, &q, &[point]).expect("valid span and loads");
        let resultant: f64 = force.iter().sum();
        let root_moment: f64 = force.iter().zip(y).map(|(f, y)| f * y).sum();
        assert!((resultant - (12.0 * 4.0 - 7.0)).abs() < 1.0e-13);
        assert!((root_moment - (12.0 * 4.0_f64.powi(2) / 2.0 - 7.0 * 1.3)).abs() < 1.0e-13);
    }

    #[test]
    fn point_load_outside_the_span_is_rejected() {
        assert!(station_forces(&[0.0, 1.0], &[0.0, 0.0], &[(1.1, -5.0)]).is_err());
    }
}
