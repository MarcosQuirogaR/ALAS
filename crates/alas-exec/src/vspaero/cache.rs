// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in result cache (`ALAS_TOOL_CACHE_DIR`) for VSPAERO solves.
//!
//! The solver inputs are the `.vspgeom` panels and the `.vspaero` setup. The
//! `.vkey` tag file is also read, but OpenVSP writes run-specific text into it:
//! the absolute path of the geometry file, a random ten-letter id per
//! component, and the order in which it numbers components. None of that
//! reaches the aerodynamic totals the pipeline reads (serial and parallel runs
//! of the same design produced bit-identical polars with different ids), so the
//! key uses a canonical form of the tag file that keeps what identifies a
//! component (name, part, surface, copy, thickness, plate) and drops the run
//! specific fields. A tag file in an unexpected format is keyed verbatim with
//! only the path masked, which can miss but never hit wrongly.

use std::path::Path;

use crate::process::parent_directory;
use crate::tool_cache::DirCache;

/// Cache handle for one solve, or `None` when the cache is off or an input
/// cannot be read.
pub(super) fn begin(executable: &Path, case_path: &Path, thread_count: usize) -> Option<DirCache> {
    let vkey = std::fs::read_to_string(case_path.with_extension("vkey")).ok()?;
    let canonical = canonical_vkey(&vkey);
    DirCache::begin(
        "vspaero",
        executable,
        parent_directory(case_path),
        &["vspgeom", "vspaero"].map(|extension| case_path.with_extension(extension)),
        &[
            thread_count.max(1).to_string().as_bytes(),
            canonical.as_bytes(),
        ],
    )
}

/// Tag-file text with the run-specific fields removed (see the module note).
fn canonical_vkey(text: &str) -> String {
    let mut out = String::new();
    let mut in_part_table = false;
    for (index, line) in text.lines().enumerate() {
        if index == 1 {
            // The geometry file's absolute path.
            out.push_str("<geometry path>\n");
            continue;
        }
        if line.starts_with('#') {
            in_part_table = line.starts_with("# part#,geom#,surf#,gname,gid");
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if in_part_table && fields.len() == 9 {
            // part#, surf#, gname, thick, plate, copy#; dropped: geom#, gid, geomcopy#.
            for index in [0, 2, 3, 5, 6, 7] {
                out.push_str(fields[index]);
                out.push(',');
            }
            out.push('\n');
        } else {
            if line.trim().is_empty() {
                in_part_table = false;
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::canonical_vkey;

    const RUN_A: &str = "# VSPGEOM v3 Tag Key File\nC:/a/run1/x.vspgeom\n5\n\n# part#,geom#,surf#,gname,gid,thick,plate,copy#,geomcopy#\n1,0,0,Main Wing_C,TMFAEFFAUG,0,3,1,1\n3,1,0,Horizontal Stabilizer_C,MHCAOPIWHS,0,3,1,2\n\n# tag#,part#,ssname1\n1,1\n";
    const RUN_B: &str = "# VSPGEOM v3 Tag Key File\nC:/b/run2/x.vspgeom\n5\n\n# part#,geom#,surf#,gname,gid,thick,plate,copy#,geomcopy#\n1,2,0,Main Wing_C,EUFGCSZXPH,0,3,1,3\n3,1,0,Horizontal Stabilizer_C,FZISGDKKYV,0,3,1,2\n\n# tag#,part#,ssname1\n1,1\n";

    #[test]
    fn run_specific_paths_ids_and_geometry_order_do_not_change_the_key_text() {
        assert_eq!(canonical_vkey(RUN_A), canonical_vkey(RUN_B));
    }

    #[test]
    fn a_different_component_name_or_thickness_does_change_it() {
        let renamed = RUN_A.replace("Main Wing_C", "Canard_C");
        assert_ne!(canonical_vkey(RUN_A), canonical_vkey(&renamed));
        let thick = RUN_A.replace("TMFAEFFAUG,0,3", "TMFAEFFAUG,1,3");
        assert_ne!(canonical_vkey(RUN_A), canonical_vkey(&thick));
    }
}
