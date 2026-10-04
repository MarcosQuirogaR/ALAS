// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Decoding the DATA record of each datablock into its table.

use super::*;

/// Route one (IDENT, DATA) pair into the right table.
pub(super) fn store(
    op2: &mut Op2,
    name: &str,
    table3: &[u8],
    table4: &[u8],
) -> Result<(), Op2Error> {
    let analysis = word_i32(table3, 0)? / 10; // approach code = analysis*10 + device
    let subcase = i64::from(word_i32(table3, 3)?);
    if name.starts_with("OES") {
        // MSC groups several element layouts in OES1X1. Only element type 144
        // is CQUAD4 corner stress (87 words); CBAR type 34 and CTRIA3 type 74
        // must be skipped rather than decoded as quads.
        let element_type = word_i32(table3, 2)?;
        let num_wide = word_i32(table3, 9)?;
        if element_type != 144 || num_wide != 87 {
            op2.unread_result_tables
                .push(UnreadResultTable::ElementStress {
                    name: name.to_owned(),
                    subcase,
                    element_type,
                    num_wide,
                });
            return Ok(());
        }
        let table = op2
            .cquad4_stress
            .entry(subcase)
            .or_insert_with(|| StressTable {
                element_ids: Vec::new(),
                node_ids: Vec::new(),
                data: Vec::new(),
            });
        read_cquad4_corner(table4, table)?;
        return Ok(());
    }
    // OQG carries SPC forces and OPG carries applied loads in the same vector
    // shape. Treating either as displacement would overwrite OUG in a real MSC
    // file because OPG is written after OUG.
    if !name.starts_with("OUG") && name != "OPHIG" {
        op2.unread_result_tables.push(UnreadResultTable::Vector {
            name: name.to_owned(),
            subcase,
            num_wide: i64::from(word_i32(table3, 9)?),
        });
        return Ok(());
    }
    let num_wide = i64::from(word_i32(table3, 9)?);
    let mut nodes = read_vector(table4, num_wide)?;
    match analysis {
        2 => {
            // Eigenvectors: one IDENT/DATA pair per mode, all one subcase.
            let mode = i64::from(word_i32(table3, 4)?);
            let eigenvalue = f64::from(word_f32(table3, 5)?);
            let written_cycle = f64::from(word_f32(table3, 6)?);
            // pyNastran-authored fixtures carry mode_cycle in word 7. MSC
            // 2026.1 leaves that word zero while writing the eigenvalue, so use
            // the defining relation lambda = (2*pi*f)^2 only for the absent
            // product field. A negative eigenvalue has no real cyclic
            // frequency and remains NaN rather than being silently absoluted.
            let cycle = if written_cycle.is_finite() && written_cycle > 0.0 {
                written_cycle
            } else if eigenvalue > 0.0 {
                eigenvalue.sqrt() / std::f64::consts::TAU
            } else {
                f64::NAN
            };
            let table = op2
                .eigenvectors
                .entry(subcase)
                .or_insert_with(|| EigenvectorTable {
                    modes: Vec::new(),
                    eigenvalues: Vec::new(),
                    mode_cycles: Vec::new(),
                    node_ids: nodes.node_ids.clone(),
                    data: Vec::new(),
                });
            table.modes.push(mode);
            table.eigenvalues.push(eigenvalue);
            table.mode_cycles.push(cycle);
            table.data.push(nodes.real);
        }
        5 => {
            // Complex frequency response: one pair per frequency, one subcase.
            // FORMAT=3 stores magnitudes followed by phase angles in degrees;
            // MSC writes this when case control requests PHASE. The synthetic
            // pyNastran fixture uses FORMAT=2 real/imaginary pairs instead.
            if word_i32(table3, 8)? == 3 {
                magnitude_phase_to_rectangular(&mut nodes);
            }
            let freq = f64::from(word_f32(table3, 4)?);
            let table =
                op2.complex_displacements
                    .entry(subcase)
                    .or_insert_with(|| ComplexVectorTable {
                        freqs: Vec::new(),
                        node_ids: nodes.node_ids.clone(),
                        real: Vec::new(),
                        imag: Vec::new(),
                    });
            table.freqs.push(freq);
            table.real.push(nodes.real);
            table.imag.push(nodes.imag);
        }
        _ => {
            if op2.displacements.contains_key(&subcase) {
                op2.duplicate_static_subcases.push(subcase);
            }
            op2.displacements.insert(
                subcase,
                VectorTable {
                    node_ids: nodes.node_ids,
                    data: nodes.real,
                },
            );
        }
    }
    Ok(())
}

/// Convert a FORMAT=3 complex vector from magnitude/degrees to Cartesian form.
fn magnitude_phase_to_rectangular(rows: &mut VectorRows) {
    for (magnitudes, phases) in rows.real.iter_mut().zip(&mut rows.imag) {
        for (magnitude, phase_degrees) in magnitudes.iter_mut().zip(phases.iter_mut()) {
            let phase_radians = phase_degrees.to_radians();
            let real = *magnitude * phase_radians.cos();
            let imaginary = *magnitude * phase_radians.sin();
            *magnitude = real;
            *phase_degrees = imaginary;
        }
    }
}

/// The node line and per-node components read out of one DATA record.
pub(super) struct VectorRows {
    node_ids: Vec<i64>,
    real: Vec<[f64; 6]>,
    imag: Vec<[f64; 6]>,
}

/// Read a displacement/eigenvector/complex DATA record: `num_wide` words per
/// node: `[nid*10+device, gridtype, six real (+ six imaginary)]`.
pub(super) fn read_vector(table4: &[u8], num_wide: i64) -> Result<VectorRows, Op2Error> {
    let per = match num_wide {
        8 | 14 => num_wide as usize,
        other => return Err(Op2Error::UnsupportedVectorWidth { num_wide: other }),
    };
    let total = table4.len() / 4;
    if total % per != 0 {
        return Err(Op2Error::RecordTooShort);
    }
    let count = total / per;
    let mut rows = VectorRows {
        node_ids: Vec::with_capacity(count),
        real: Vec::with_capacity(count),
        imag: Vec::with_capacity(count),
    };
    for k in 0..count {
        let base = k * per;
        rows.node_ids.push(i64::from(word_i32(table4, base)? / 10));
        rows.real.push(read_six(table4, base + 2)?);
        if per == 14 {
            rows.imag.push(read_six(table4, base + 8)?);
        }
    }
    Ok(rows)
}

/// Read a CQUAD4-144 corner-stress DATA record: 87 words per element:
/// `[eid*10+device, 'CEN/', then five nodes of (gid, two fibers x eight)]`.
fn read_cquad4_corner(table4: &[u8], table: &mut StressTable) -> Result<(), Op2Error> {
    let total = table4.len() / 4;
    if total % CQUAD4_CORNER_WORDS != 0 {
        return Err(Op2Error::RecordTooShort);
    }
    let mut p = 0usize;
    while p < total {
        let eid = i64::from(word_i32(table4, p)? / 10);
        p += 2; // element id (with device code) and the 'CEN/' word
        for node_index in 0..CORNER_NODES {
            let raw_gid = i64::from(word_i32(table4, p)?);
            // MSC writes the CQUAD4 corner count (`4`) in the centroid slot;
            // pyNastran normalizes that logical location to grid id zero. Its
            // synthesized OP2 writes a literal zero, so this correction is
            // two-sided and preserves the frozen fixture.
            let gid = if node_index == 0 { 0 } else { raw_gid };
            p += 1;
            for _fiber in 0..2 {
                let mut row = [0.0f64; 8];
                for (component, slot) in row.iter_mut().enumerate() {
                    *slot = f64::from(word_f32(table4, p + component)?);
                }
                p += 8;
                table.element_ids.push(eid);
                table.node_ids.push(gid);
                table.data.push(row);
            }
        }
    }
    Ok(())
}

/// The nodes a CQUAD4 corner record reports: the centroid then four corners.
const CORNER_NODES: usize = 5;

/// CQUAD4-144 corner stress words per element.
const CQUAD4_CORNER_WORDS: usize = 87;
