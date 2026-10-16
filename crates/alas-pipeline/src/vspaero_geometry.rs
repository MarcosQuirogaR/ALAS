// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Pre-flight panel check of an OpenVSP `.vspgeom` lifting-surface mesh.
//!
//! VSPAERO's panel solver takes triangles and quadrilaterals. Where two
//! lifting surfaces of the exported aircraft intersect (the B747-400 tail: the
//! fin root is seated at the tailplane height), OpenVSP trims both surfaces
//! along the intersection and writes the cut cells as polygons of two, five,
//! six or eight vertices. The solver accepts that file and returns force
//! coefficients that are not physical: the 2026-10-07 B747-400 runs had
//! induced drag of -0.06 to -0.48 at every wake-iteration count and a side
//! force of 0.012 to 0.29 at zero sideslip on a symmetric aircraft, while the
//! eleven other presets (all-quadrilateral meshes) did not. The wake gate then
//! rejects the result after a multi-minute solve. This check finds the cause
//! first and names it.
//!
//! File layout (`# vspgeom v3`): comment line, a flag line, `vertices faces
//! components`, one line per vertex, the face count again, then one line per
//! face starting with its vertex count.

use std::fs;
use std::path::Path;

/// Faces listed in a diagnostic.
const LISTED_FACES: usize = 3;

/// Faces that are neither triangles nor quadrilaterals, as `(1-based face
/// number, vertex count, centroid)`, plus the total panel count.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NonPanelFaces {
    pub(crate) panel_count: usize,
    pub(crate) faces: Vec<(usize, usize, [f64; 3])>,
}

/// Parse `text` and list its faces that are not triangles or quadrilaterals.
/// `None` when the text is not a recognizable `vspgeom` (the check then does
/// not apply and must not block the solve).
pub(crate) fn non_panel_faces(text: &str) -> Option<NonPanelFaces> {
    let mut lines = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'));
    lines.next()?; // flag line
    let mut header = lines.next()?.split_whitespace();
    let vertex_count: usize = header.next()?.parse().ok()?;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        let mut fields = lines.next()?.split_whitespace();
        let mut xyz = [0.0; 3];
        for slot in &mut xyz {
            *slot = fields.next()?.parse().ok()?;
        }
        vertices.push(xyz);
    }
    let panel_count: usize = lines.next()?.trim().parse().ok()?;
    let mut faces = Vec::new();
    for number in 1..=panel_count {
        let mut fields = lines.next()?.split_whitespace();
        let count: usize = fields.next()?.parse().ok()?;
        let ids = fields
            .take(count)
            .map(|field| field.parse::<usize>().ok())
            .collect::<Option<Vec<_>>>()?;
        if count == 3 || count == 4 {
            continue;
        }
        let points = ids
            .iter()
            .filter_map(|id| vertices.get(id.checked_sub(1)?))
            .collect::<Vec<_>>();
        let n = points.len().max(1) as f64;
        let centroid = [0, 1, 2].map(|axis| points.iter().map(|p| p[axis]).sum::<f64>() / n);
        faces.push((number, count, centroid));
    }
    Some(NonPanelFaces { panel_count, faces })
}

/// A diagnostic when the `.vspgeom` at `path` contains faces VSPAERO cannot
/// treat as panels, `None` when it is clean or not checkable.
pub(crate) fn panel_defect(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let defect = non_panel_faces(&text)?;
    if defect.faces.is_empty() {
        return None;
    }
    let examples = defect
        .faces
        .iter()
        .take(LISTED_FACES)
        .map(|(number, count, c)| {
            format!(
                "face {number} has {count} vertices near x={:.1} y={:.1} z={:.1} m",
                c[0], c[1], c[2]
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "OpenVSP wrote {} of {} panels that are neither triangles nor quadrilaterals ({examples}). \
         This is where two lifting surfaces of the exported aircraft intersect; VSPAERO returns \
         non-physical forces on such a mesh (negative induced drag), so it was not run. Separate \
         the intersecting surfaces in the aircraft geometry (for example the tailplane height \
         against the fin root)",
        defect.faces.len(),
        defect.panel_count
    ))
}

// Tests assert on values they construct directly, so a failed unwrap is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(face_lines: &[&str]) -> String {
        let mut text = String::from("# vspgeom v3\n1\n6 0 1\n");
        for (x, y) in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)] {
            text.push_str(&format!("{x} {y} 0\n"));
        }
        text.push_str(&format!("{}\n", face_lines.len()));
        for line in face_lines {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str("1 1 0 0 1 0\n");
        text
    }

    #[test]
    fn an_all_quadrilateral_mesh_is_clean() {
        let text = geometry(&["4 1 2 5 4", "4 2 3 6 5"]);
        let found = non_panel_faces(&text).unwrap();
        assert_eq!(found.panel_count, 2);
        assert!(found.faces.is_empty());
    }

    #[test]
    fn intersection_cells_are_listed_with_their_location() {
        let text = geometry(&["4 1 2 5 4", "5 2 3 6 5 4", "2 1 2", "3 1 2 3"]);
        let found = non_panel_faces(&text).unwrap();
        assert_eq!(
            found.faces.len(),
            2,
            "triangles are panels, a 5-gon and a segment are not"
        );
        assert_eq!((found.faces[0].0, found.faces[0].1), (2, 5));
        assert_eq!((found.faces[1].0, found.faces[1].1), (3, 2));
        assert!((found.faces[1].2[0] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn an_unrecognized_file_does_not_block_the_solve() {
        assert!(non_panel_faces("not a geometry").is_none());
        assert!(non_panel_faces("").is_none());
        assert!(panel_defect(Path::new("no/such/file.vspgeom")).is_none());
    }
}
