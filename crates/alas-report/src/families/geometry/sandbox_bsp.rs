// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact painter's ordering of the sandbox faces through a binary space
//! partition.
//!
//! Sorting faces by their mean depth fails for the aircraft: a lofted wing
//! quad runs from the root section to the next planform station, so its
//! mean depth says nothing about the root corner that pierces the fuselage,
//! and from many angles the wing was painted over the nearer fuselage skin.
//! The tree built here splits every face on the planes of the faces that
//! cross it, after which a back-to-front walk of the tree for any view
//! direction paints every fragment under everything nearer to the viewer,
//! without depending on the order in which components were added.
//!
//! The tree is built once per geometry in model axes; only the walk depends
//! on the camera, so an orbit frame costs one traversal.

use crate::scene::Point3D;

/// One face fragment stored in the tree, in model axes.
#[derive(Debug, Clone, PartialEq)]
pub struct BspPolygon {
    /// Vertices in model axes, in the winding of the source face.
    pub points: Vec<Point3D>,
    /// Index of the face this fragment was cut from.
    pub source: usize,
    /// Per edge (from vertex `i` to `i + 1`), whether it was made by a cut
    /// rather than being part of the source face's outline. Empty means
    /// every edge is original.
    pub cut: Vec<bool>,
}

impl BspPolygon {
    /// A fragment that is a whole source face.
    pub fn face(points: Vec<Point3D>, source: usize) -> Self {
        Self {
            points,
            source,
            cut: Vec::new(),
        }
    }

    /// Whether the edge leaving vertex `i` was made by a cut.
    pub fn edge_is_cut(&self, i: usize) -> bool {
        self.cut.get(i).copied().unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Plane {
    normal: Point3D,
    offset: f64,
}

impl Plane {
    /// The plane through a polygon by the Newell method, robust for
    /// slightly non-planar lofted quads; `None` for a degenerate polygon.
    fn of(points: &[Point3D]) -> Option<Self> {
        if points.len() < 3 {
            return None;
        }
        let mut normal = [0.0; 3];
        let mut centroid = [0.0; 3];
        for (i, a) in points.iter().enumerate() {
            let b = points[(i + 1) % points.len()];
            normal[0] += (a[1] - b[1]) * (a[2] + b[2]);
            normal[1] += (a[2] - b[2]) * (a[0] + b[0]);
            normal[2] += (a[0] - b[0]) * (a[1] + b[1]);
            for k in 0..3 {
                centroid[k] += a[k];
            }
        }
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if !len.is_finite() || len < 1e-12 {
            return None;
        }
        let normal = [normal[0] / len, normal[1] / len, normal[2] / len];
        let n = points.len() as f64;
        let centroid = [centroid[0] / n, centroid[1] / n, centroid[2] / n];
        Some(Self {
            normal,
            offset: -(normal[0] * centroid[0] + normal[1] * centroid[1] + normal[2] * centroid[2]),
        })
    }

    fn distance(&self, p: Point3D) -> f64 {
        self.normal[0] * p[0] + self.normal[1] * p[1] + self.normal[2] * p[2] + self.offset
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Coplanar,
    Front,
    Back,
    Spanning,
}

fn classify(plane: &Plane, points: &[Point3D], eps: f64) -> Side {
    let mut front = false;
    let mut back = false;
    for &p in points {
        let d = plane.distance(p);
        if d > eps {
            front = true;
        } else if d < -eps {
            back = true;
        }
    }
    match (front, back) {
        (false, false) => Side::Coplanar,
        (true, false) => Side::Front,
        (false, true) => Side::Back,
        (true, true) => Side::Spanning,
    }
}

/// A piece of a split polygon: each vertex with the flag of the edge that
/// leaves it (`true` when that edge runs along the cutting plane).
type Piece = Vec<(Point3D, bool)>;

/// Cut a polygon by a plane into its front and back pieces. Vertices on the
/// plane belong to both pieces, so no intersection point is invented for
/// an edge that merely touches the plane. Edges along the plane are
/// flagged as cuts; portions of the original edges keep their flag.
fn split(plane: &Plane, polygon: &BspPolygon, eps: f64) -> (Piece, Piece) {
    let points = &polygon.points;
    let mut front = Vec::with_capacity(points.len() + 2);
    let mut back = Vec::with_capacity(points.len() + 2);
    let side = |d: f64| {
        if d > eps {
            1
        } else if d < -eps {
            -1
        } else {
            0
        }
    };
    for (i, &a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        let flag = polygon.edge_is_cut(i);
        let sa = side(plane.distance(a));
        let sb = side(plane.distance(b));
        if sa >= 0 {
            // The front piece leaves `a` along the plane only when `a` is
            // on it and the edge dives behind; otherwise it follows the
            // original edge (to `b` or to the crossing point).
            front.push((a, if sa == 0 && sb < 0 { true } else { flag }));
        }
        if sa <= 0 {
            back.push((a, if sa == 0 && sb > 0 { true } else { flag }));
        }
        if sa * sb < 0 {
            let da = plane.distance(a);
            let db = plane.distance(b);
            let t = da / (da - db);
            let cut = [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ];
            if sa > 0 {
                front.push((cut, true));
                back.push((cut, flag));
            } else {
                front.push((cut, flag));
                back.push((cut, true));
            }
        }
    }
    (front, back)
}

fn area_squared(points: &[Point3D]) -> f64 {
    let mut n = [0.0; 3];
    for (i, a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    0.25 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2])
}

#[derive(Debug, Clone)]
struct Node {
    plane: Plane,
    /// Fragments lying in the node plane, painted between the two halves.
    polygons: Vec<usize>,
    front: Option<usize>,
    back: Option<usize>,
}

/// The partition tree over a set of face fragments.
#[derive(Debug, Clone, Default)]
pub struct BspTree {
    polygons: Vec<BspPolygon>,
    nodes: Vec<Node>,
    root: Option<usize>,
    /// Fragments without a usable plane, painted first.
    degenerate: Vec<usize>,
    /// Cutting stops once this many fragments exist.
    max_fragments: usize,
}

/// How many candidate splitters are scored per node.
const SPLITTER_CANDIDATES: usize = 8;

type Pending = Vec<(Vec<usize>, Option<(usize, bool)>)>;

impl BspTree {
    /// Build the tree over `faces`, cutting faces where they cross each
    /// other. `eps` is the model distance under which a vertex counts as
    /// lying on a plane. Cutting stops once `max_fragments` exist; the
    /// remaining sets are then kept whole under their node in insertion
    /// order.
    pub fn build(faces: Vec<BspPolygon>, eps: f64, max_fragments: usize) -> Self {
        let eps = eps.max(1e-9);
        let mut tree = Self {
            max_fragments,
            ..Self::default()
        };
        let mut pending: Pending = Vec::new();
        let mut initial = Vec::with_capacity(faces.len());
        for face in faces {
            let index = tree.polygons.len();
            tree.polygons.push(face);
            initial.push(index);
        }
        pending.push((initial, None));
        while let Some((set, parent)) = pending.pop() {
            let Some(node) = tree.build_node(set, eps, &mut pending) else {
                continue;
            };
            match parent {
                None => tree.root = Some(node),
                Some((p, true)) => tree.nodes[p].front = Some(node),
                Some((p, false)) => tree.nodes[p].back = Some(node),
            }
        }
        tree
    }

    /// Partition one set, queueing its halves; `None` when the set holds no
    /// usable plane (its members are then painted first, in any order).
    fn build_node(&mut self, set: Vec<usize>, eps: f64, pending: &mut Pending) -> Option<usize> {
        let planes: Vec<(usize, Plane)> = set
            .iter()
            .filter_map(|&i| Plane::of(&self.polygons[i].points).map(|p| (i, p)))
            .collect();
        if planes.is_empty() || self.polygons.len() >= self.max_fragments {
            self.degenerate.extend(set);
            return None;
        }
        let score = |plane: &Plane| {
            let (mut front, mut back, mut spanning) = (0usize, 0usize, 0usize);
            for &i in &set {
                match classify(plane, &self.polygons[i].points, eps) {
                    Side::Front => front += 1,
                    Side::Back => back += 1,
                    Side::Spanning => spanning += 1,
                    Side::Coplanar => {}
                }
            }
            spanning * 3 + front.abs_diff(back)
        };
        let (splitter, plane) = planes
            .iter()
            .take(SPLITTER_CANDIDATES)
            .min_by_key(|(_, plane)| score(plane))
            .copied()
            .unwrap_or(planes[0]);
        let mut coplanar = Vec::new();
        let mut front = Vec::new();
        let mut back = Vec::new();
        for i in set {
            // The splitter defines the plane, so it stays with this node
            // whatever its own (slightly non-planar) vertices say; this is
            // also what makes every child set strictly smaller.
            if i == splitter {
                coplanar.push(i);
                continue;
            }
            match classify(&plane, &self.polygons[i].points, eps) {
                Side::Coplanar => coplanar.push(i),
                Side::Front => front.push(i),
                Side::Back => back.push(i),
                Side::Spanning => {
                    let (f, b) = split(&plane, &self.polygons[i], eps);
                    let source = self.polygons[i].source;
                    let mut replaced = false;
                    for (piece, side) in [(f, &mut front), (b, &mut back)] {
                        let points: Vec<Point3D> = piece.iter().map(|&(p, _)| p).collect();
                        if points.len() < 3 || area_squared(&points) < 1e-18 {
                            continue;
                        }
                        let cut: Vec<bool> = piece.iter().map(|&(_, c)| c).collect();
                        if replaced {
                            side.push(self.polygons.len());
                            self.polygons.push(BspPolygon {
                                points,
                                source,
                                cut,
                            });
                        } else {
                            self.polygons[i].points = points;
                            self.polygons[i].cut = cut;
                            side.push(i);
                            replaced = true;
                        }
                    }
                    if !replaced {
                        // Both pieces vanished: the face was a sliver on
                        // the plane; keep it with the plane.
                        coplanar.push(i);
                    }
                }
            }
        }
        let node = self.nodes.len();
        self.nodes.push(Node {
            plane,
            polygons: coplanar,
            front: None,
            back: None,
        });
        if !front.is_empty() {
            pending.push((front, Some((node, true))));
        }
        if !back.is_empty() {
            pending.push((back, Some((node, false))));
        }
        Some(node)
    }

    /// Every fragment in the tree.
    pub fn polygons(&self) -> &[BspPolygon] {
        &self.polygons
    }

    /// Fragment indices back to front for an orthographic viewer looking
    /// from direction `view` (unit vector from the model toward the eye).
    pub fn painter_order(&self, view: Point3D) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.polygons.len());
        order.extend_from_slice(&self.degenerate);
        let mut stack: Vec<(usize, bool)> = Vec::new();
        if let Some(root) = self.root {
            stack.push((root, false));
        }
        // Each node is visited twice: first to queue its far half, then,
        // after that half is emitted, to emit itself and queue the near half.
        while let Some((index, expanded)) = stack.pop() {
            let node = &self.nodes[index];
            let n = node.plane.normal;
            let toward_viewer = n[0] * view[0] + n[1] * view[1] + n[2] * view[2] >= 0.0;
            let (far, near) = if toward_viewer {
                (node.back, node.front)
            } else {
                (node.front, node.back)
            };
            if expanded {
                order.extend_from_slice(&node.polygons);
                if let Some(near) = near {
                    stack.push((near, false));
                }
            } else {
                stack.push((index, true));
                if let Some(far) = far {
                    stack.push((far, false));
                }
            }
        }
        order
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn quad(points: [Point3D; 4], source: usize) -> BspPolygon {
        BspPolygon::face(points.to_vec(), source)
    }

    fn floor() -> BspPolygon {
        quad(
            [
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            1,
        )
    }

    fn position(tree: &BspTree, order: &[usize], pred: impl Fn(&BspPolygon) -> bool) -> usize {
        order
            .iter()
            .position(|&i| pred(&tree.polygons()[i]))
            .expect("fragment present")
    }

    #[test]
    fn crossing_faces_are_split_and_ordered_for_both_view_directions() {
        // A vertical wall at x = 0 and a floor at z = 0 crossing it.
        let wall = quad(
            [
                [0.0, -1.0, -1.0],
                [0.0, 1.0, -1.0],
                [0.0, 1.0, 1.0],
                [0.0, -1.0, 1.0],
            ],
            0,
        );
        let tree = BspTree::build(vec![wall, floor()], 1e-9, usize::MAX);
        assert!(tree.polygons().len() >= 3, "one of the faces was cut");
        let is_front_floor =
            |p: &BspPolygon| p.source == 1 && p.points.iter().all(|q| q[0] >= -1e-9);
        let is_back_floor = |p: &BspPolygon| p.source == 1 && p.points.iter().all(|q| q[0] <= 1e-9);

        // Seen from x > 0: the floor half at x > 0 lies in front of the
        // wall, the half at x < 0 behind it.
        let order = tree.painter_order([1.0, 0.0, 1.0]);
        assert_eq!(order.len(), tree.polygons().len());
        let wall_at = position(&tree, &order, |p| p.source == 0);
        assert!(position(&tree, &order, is_back_floor) < wall_at);
        assert!(wall_at < position(&tree, &order, is_front_floor));

        let order = tree.painter_order([-1.0, 0.0, 1.0]);
        let wall_at = position(&tree, &order, |p| p.source == 0);
        assert!(position(&tree, &order, is_front_floor) < wall_at);
        assert!(wall_at < position(&tree, &order, is_back_floor));
    }

    #[test]
    fn cut_edges_are_flagged_and_original_edges_are_not() {
        let wall = quad(
            [
                [0.0, -1.0, -1.0],
                [0.0, 1.0, -1.0],
                [0.0, 1.0, 1.0],
                [0.0, -1.0, 1.0],
            ],
            0,
        );
        let tree = BspTree::build(vec![wall, floor()], 1e-9, usize::MAX);
        let pieces: Vec<&BspPolygon> = tree.polygons().iter().filter(|p| p.source == 1).collect();
        let cut_edges = pieces.iter().filter(|p| p.cut.iter().any(|&c| c)).count();
        assert!(
            cut_edges >= 1,
            "the floor was cut along the wall: {pieces:?}"
        );
        for piece in pieces {
            for (i, &p1) in piece.points.iter().enumerate() {
                let p2 = piece.points[(i + 1) % piece.points.len()];
                let on_wall = p1[0].abs() < 1e-9 && p2[0].abs() < 1e-9;
                assert_eq!(piece.edge_is_cut(i), on_wall, "edge {p1:?} -> {p2:?}");
            }
        }
        // The uncut wall keeps every edge original.
        let wall = tree
            .polygons()
            .iter()
            .find(|p| p.source == 0)
            .expect("wall");
        assert!((0..wall.points.len()).all(|i| !wall.edge_is_cut(i)));
    }

    #[test]
    fn degenerate_faces_survive_and_paint_first() {
        let needle = BspPolygon::face(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]], 7);
        let tree = BspTree::build(vec![needle, floor()], 1e-9, usize::MAX);
        let order = tree.painter_order([0.0, 0.0, 1.0]);
        assert_eq!(order.len(), 2);
        assert_eq!(tree.polygons()[order[0]].source, 7);
    }
}
