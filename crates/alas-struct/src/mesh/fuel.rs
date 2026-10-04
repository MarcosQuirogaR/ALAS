// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuel inertia for the same design state as the strength and beam models.

use super::{Conm2, Deck, MeshNodeIndex};

/// Invalid fuel array or attachment mesh; the deck is left unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("fuel inertia needs finite nonnegative running masses, increasing stations and complete spar attachment grids")]
pub struct FuelMassError;

/// Add the modelled semi-wing's fuel inertia to a finite-element deck.
///
/// SI: stations m, running mass kg/m. Trapezoidal nodal weights match the
/// sizing grid's integral exactly. Each station's mass is shared equally
/// between the upper/lower nodes of the front/rear full-span spars. CONM2
/// offsets preserve the requested spanwise station on the mesh's different
/// rib grid, conserving mass and spanwise first moment. Chordwise/vertical
/// fuel location is the box midpoint approximation, not a resolved cell mesh.
/// Existing signed GRAV cards then generate n*g relief for every load case;
/// the same fuel mass participates in modal inertia.
///
/// Call once, after building the mesh and before writing/running solver decks.
pub fn add_distributed_fuel_mass(
    deck: &mut Deck,
    index: &MeshNodeIndex,
    stations_m: &[f64],
    running_mass_kg_m: &[f64],
) -> Result<(), FuelMassError> {
    if stations_m.len() < 2
        || stations_m.len() != running_mass_kg_m.len()
        || stations_m.iter().any(|y| !y.is_finite() || *y < 0.0)
        || stations_m.windows(2).any(|pair| pair[1] <= pair[0])
        || running_mass_kg_m
            .iter()
            .any(|mass| !mass.is_finite() || *mass < 0.0)
        || index.spar_upper_nids.len() < 2
        || index.spar_upper_nids.len() != index.spar_lower_nids.len()
    {
        return Err(FuelMassError);
    }
    // The outermost spars enclose the box; intermediate spars can terminate
    // at a kink and must not absorb outboard fuel by accidental index choice.
    let last = index.spar_upper_nids.len() - 1;
    let lines = [
        &index.spar_upper_nids[0],
        &index.spar_lower_nids[0],
        &index.spar_upper_nids[last],
        &index.spar_lower_nids[last],
    ];
    if lines.iter().any(|line| {
        line.is_empty()
            || line.iter().any(|nid| {
                deck.grid_xyz(*nid)
                    .is_none_or(|xyz| xyz.iter().any(|v| !v.is_finite()))
            })
    }) {
        return Err(FuelMassError);
    }
    let last_eid = deck
        .quads
        .iter()
        .chain(&deck.trias)
        .map(|element| element.eid)
        .chain(deck.bars.iter().map(|element| element.eid))
        .chain(deck.masses.iter().map(|element| element.eid))
        .chain(deck.rigid_elements.iter().map(|element| element.eid))
        .max()
        .unwrap_or(0);
    let mut added = Vec::new();
    for (j, (&station, &running)) in stations_m.iter().zip(running_mass_kg_m).enumerate() {
        let left = if j == 0 {
            0.0
        } else {
            station - stations_m[j - 1]
        };
        let right = if j + 1 == stations_m.len() {
            0.0
        } else {
            stations_m[j + 1] - station
        };
        let mass_each = running * (left + right) / 8.0;
        if !mass_each.is_finite() {
            return Err(FuelMassError);
        }
        if mass_each == 0.0 {
            continue;
        }
        for line in &lines {
            let nid = *line
                .iter()
                .min_by(|a, b| {
                    (deck.node_y(**a) - station)
                        .abs()
                        .total_cmp(&(deck.node_y(**b) - station).abs())
                })
                .ok_or(FuelMassError)?;
            added.push(Conm2 {
                eid: last_eid + added.len() as i64 + 1,
                nid,
                cid: 0,
                mass: mass_each,
                offset: [0.0, station - deck.node_y(nid), 0.0],
            });
        }
    }
    deck.masses.extend(added);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuel_lumping_conserves_mass_and_spanwise_first_moment_and_ids() {
        let mut deck = Deck::new();
        for (i, xyz) in [
            [0.0, 0.0, 1.0],
            [0.0, 10.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 10.0, -1.0],
            [2.0, 0.0, 1.0],
            [2.0, 10.0, 1.0],
            [2.0, 0.0, -1.0],
            [2.0, 10.0, -1.0],
        ]
        .into_iter()
        .enumerate()
        {
            deck.add_grid(i as i64 + 1, xyz);
        }
        let index = MeshNodeIndex {
            root_nid: 1,
            tip_nid: 2,
            kink_nid: 1,
            spar_upper_nids: vec![vec![1, 2], vec![5, 6]],
            spar_lower_nids: vec![vec![3, 4], vec![7, 8]],
            engine_nids: vec![],
        };
        let y = [0.0, 3.0, 10.0];
        let mass = [10.0, 20.0, 0.0];
        add_distributed_fuel_mass(&mut deck, &index, &y, &mass).unwrap();
        assert!((deck.masses.iter().map(|m| m.mass).sum::<f64>() - 115.0).abs() < 1.0e-12);
        assert!(
            (deck
                .masses
                .iter()
                .map(|m| m.mass * (deck.node_y(m.nid) + m.offset[1]))
                .sum::<f64>()
                - 300.0)
                .abs()
                < 1.0e-12
        );
        assert_eq!(
            deck.masses.iter().map(|m| m.eid).collect::<Vec<_>>(),
            (1..=8).collect::<Vec<_>>()
        );
        let before = deck.clone();
        assert!(add_distributed_fuel_mass(&mut deck, &index, &y, &[10.0, f64::NAN, 0.0]).is_err());
        assert_eq!(deck, before);
    }
}
