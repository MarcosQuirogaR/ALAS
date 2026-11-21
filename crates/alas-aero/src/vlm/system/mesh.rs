// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The quad panel mesh of an airplane and the vortex-lattice quantities derived from each panel.

use super::*;

/// Radius of the Rankine core around a horseshoe's legs, as a fraction of the
/// smaller of the panel's bound-leg length and chord, seen by collocation
/// points of the other half of its surface or of another surface of its
/// lattice component (see [`CoreComponent`]).
///
/// Within one half of one surface the lattice places every collocation point
/// at least half a panel from the legs that matter, so no core is needed and
/// none is applied. The two mirrored halves of a symmetric surface are two
/// meshes: each section twists about its own dihedral-inclined spanwise axis,
/// so a twisted root section moves its trailing edge sideways by
/// `c sin(twist) sin(dihedral)` and the halves cross the plane of symmetry
/// (B747-400 stabilizer at 24 spanwise panels and -6 deg incidence: root
/// collocation points 47 mm from the other half's trailing legs, 0.27 m
/// strips, a 0.06 jump in Cm that stops the trimmed drag nodes converging).
/// Untwisted, the halves meet on the plane of symmetry and every root
/// collocation point stays half a strip from the shared legs, outside the
/// core. Joined surfaces (a wingtip device on its wing, a T-tail's
/// stabilizer on its fin) meet along a shared edge and keep this core too.
const INTER_SURFACE_CORE_FRACTION: f64 = 0.25;

/// Radius of the Rankine core around a horseshoe's legs seen by collocation
/// points of another lattice component, as a multiple of the panel's spanwise
/// vortex spacing (its bound leg projected on the y-z plane).
///
/// AVL 3.52 user guide (Drela and Youngren), vortex-spacing rule 1 and the
/// COMPONENT keyword: a trailing leg that passes close to a downstream control
/// point of another surface corrupts the solution unless the spanwise
/// spacings of the two surfaces line up; when that cannot be arranged AVL
/// gives every vortex a finite core for the control points of other
/// components, of radius `VRCOREW` times the strip width (default 2.0; the
/// chord-scaled `VRCOREC` defaults to 0), and none within a component. Two
/// independent meshes cannot line up: a fin crossing the plane of a wing
/// root puts collocation points a few millimetres from the wing's
/// shared-edge trailing legs (ATR72-600 at 1x24: 18 mm from legs of 0.13 m
/// panels, the bare `1 / distance` entry drives the pivot ratio past
/// [`MAX_PIVOT_RATIO`]), and a horizontal tail mounted in the plane of the
/// wing's trailing legs (C295: legs 24 mm from its collocation points at 24
/// spanwise panels) makes Cm against stabilizer incidence rough (second
/// differences 4.7e-2 per 0.2 deg with a core a quarter of the panel size,
/// 6e-4 with this one), which stops the trim converging. The core form is
/// the Rankine one of [`calculate_induced_velocity_horseshoe_cored`], exact
/// outside the radius; AVL uses the Scully form `r / sqrt(r^4 + rc^4)`,
/// which agrees with it well inside the core.
const OTHER_COMPONENT_CORE_SPACING_FACTOR: f64 = 2.0;

/// Relative distance, to the larger chord of the segment, within which the
/// end leading edge of one surface lies on another surface's planform: the
/// builder seats joined surfaces on their parent by construction, so only
/// round-off separates them.
const JUNCTION_TOLERANCE: f64 = 1e-6;

/// Where a panel sits for the leg cores: one half of one surface (`2
/// wing_index` for the meshed half, `2 wing_index + 1` for the mirror image of
/// a symmetric surface) and its lattice component (see
/// [`lattice_components`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vlm) struct CoreComponent {
    pub(in crate::vlm) half: usize,
    pub(in crate::vlm) component: usize,
}

/// The leg core radius a source at `source` with radii `within_component`
/// ([`INTER_SURFACE_CORE_FRACTION`]) and `between_components`
/// ([`OTHER_COMPONENT_CORE_SPACING_FACTOR`]) presents to a collocation point
/// at `field`: none within one half of one surface.
#[inline]
pub(in crate::vlm) fn core_seen(
    field: CoreComponent,
    source: CoreComponent,
    within_component: f64,
    between_components: f64,
) -> f64 {
    if field.half == source.half {
        0.0
    } else if field.component == source.component {
        within_component
    } else {
        between_components
    }
}

/// The leg core radius `source` presents to a collocation point at `field`,
/// see [`core_seen`].
pub(in crate::vlm) fn inter_surface_core(field: CoreComponent, source: &Panel) -> f64 {
    core_seen(
        field,
        source.core_component,
        source.leg_core_radius,
        source.other_component_core_radius,
    )
}

/// Lattice component of each surface, AVL's COMPONENT: surfaces whose root or
/// tip leading edge lies on another surface's planform (a wingtip device on
/// its wing tip, a T-tail's stabilizer on its fin) share the lowest of their
/// wing indices; every other surface is a component of its own. A
/// geometry-only surface carries no panels and joins nothing.
pub fn lattice_components(wings: &[Wing]) -> Vec<usize> {
    let mut component: Vec<usize> = (0..wings.len()).collect();
    for (j, surface) in wings.iter().enumerate() {
        for (i, other) in wings.iter().enumerate() {
            if i != j
                && !surface.is_geometry_only()
                && !other.is_geometry_only()
                && end_meets_planform(surface, other)
            {
                let keep = component[i].min(component[j]);
                let merged = component[i].max(component[j]);
                for c in &mut component {
                    if *c == merged {
                        *c = keep;
                    }
                }
            }
        }
    }
    component
}

/// Radius of the near-miss check of [`lattice_grouping_diagnostics`], as a
/// fraction of the surface's end chord. A diagnostic only: the grouping
/// itself stays the exact junction of [`JUNCTION_TOLERANCE`].
const NEAR_JUNCTION_CHORD_FRACTION: f64 = 0.1;

/// A surface end that misses another surface's planform by less than
/// [`NEAR_JUNCTION_CHORD_FRACTION`] of its chord yet is not joined to it by
/// [`lattice_components`]: a stabilizer a little off its fin, a wingtip
/// device a little off its tip. The two then see each other's legs through
/// the wide between-component core instead of the junction core.
#[derive(Debug, Clone, PartialEq)]
pub struct LatticeGroupingDiagnostic {
    /// Name of the surface whose end misses.
    pub surface: String,
    /// Name of the surface it misses.
    pub other: String,
    /// Distance from the end leading edge to the other planform, m.
    pub gap_m: f64,
    /// That end's chord, m.
    pub end_chord_m: f64,
}

/// Every ungrouped near-junction of `wings`, see [`LatticeGroupingDiagnostic`].
pub fn lattice_grouping_diagnostics(wings: &[Wing]) -> Vec<LatticeGroupingDiagnostic> {
    let component = lattice_components(wings);
    let mut diagnostics = Vec::new();
    for (j, surface) in wings.iter().enumerate() {
        for (i, other) in wings.iter().enumerate() {
            if i == j
                || component[i] == component[j]
                || surface.is_geometry_only()
                || other.is_geometry_only()
            {
                continue;
            }
            for end in [surface.xsecs.first(), surface.xsecs.last()]
                .into_iter()
                .flatten()
            {
                let p = end.xyz_le;
                let mut gap = planform_gap(p, other);
                if other.symmetric {
                    gap = gap.min(planform_gap([p[0], -p[1], p[2]], other));
                }
                if gap <= NEAR_JUNCTION_CHORD_FRACTION * end.chord {
                    diagnostics.push(LatticeGroupingDiagnostic {
                        surface: surface.name.clone(),
                        other: other.name.clone(),
                        gap_m: gap,
                        end_chord_m: end.chord,
                    });
                }
            }
        }
    }
    diagnostics
}

/// Distance from `point` to the ruled planform of `wing`, m.
fn planform_gap(point: [f64; 3], wing: &Wing) -> f64 {
    wing.xsecs
        .windows(2)
        .map(|pair| {
            let (a, b) = (&pair[0], &pair[1]);
            let span = sub3(b.xyz_le, a.xyz_le);
            let length_squared = span[1] * span[1] + span[2] * span[2];
            if length_squared <= 0.0 {
                return f64::INFINITY;
            }
            let t = (((point[1] - a.xyz_le[1]) * span[1] + (point[2] - a.xyz_le[2]) * span[2])
                / length_squared)
                .clamp(0.0, 1.0);
            let le = add3(a.xyz_le, scale3(span, t));
            let chord = a.chord + t * (b.chord - a.chord);
            let chordwise = if point[0] < le[0] {
                le[0] - point[0]
            } else {
                (point[0] - le[0] - chord).max(0.0)
            };
            (point[1] - le[1]).hypot(point[2] - le[2]).hypot(chordwise)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Set every panel's lattice component from [`lattice_components`].
pub(super) fn assign_lattice_components(wings: &[Wing], panels: &mut [Panel]) {
    let component = lattice_components(wings);
    for panel in panels {
        panel.core_component.component = component[panel.wing_index];
    }
}

/// The frozen reference lattice: every leg seen by a collocation point of
/// another lattice component keeps the [`INTER_SURFACE_CORE_FRACTION`] core
/// instead of the [`OTHER_COMPONENT_CORE_SPACING_FACTOR`] one. For parity
/// fixtures only.
pub(super) fn use_reference_component_core(panels: &mut [Panel]) {
    for panel in panels {
        panel.other_component_core_radius = panel.leg_core_radius;
    }
}

fn end_meets_planform(surface: &Wing, other: &Wing) -> bool {
    let (Some(root), Some(tip)) = (surface.xsecs.first(), surface.xsecs.last()) else {
        return false;
    };
    [root.xyz_le, tip.xyz_le].into_iter().any(|p| {
        on_planform(p, other) || (other.symmetric && on_planform([p[0], -p[1], p[2]], other))
    })
}

/// Whether `point` lies on the ruled planform between the leading edges and
/// chords of `wing`'s sections.
fn on_planform(point: [f64; 3], wing: &Wing) -> bool {
    wing.xsecs.windows(2).any(|pair| {
        let (a, b) = (&pair[0], &pair[1]);
        let span = sub3(b.xyz_le, a.xyz_le);
        let length_squared = span[1] * span[1] + span[2] * span[2];
        if length_squared <= 0.0 {
            return false;
        }
        let tolerance = JUNCTION_TOLERANCE * a.chord.max(b.chord);
        let t = ((point[1] - a.xyz_le[1]) * span[1] + (point[2] - a.xyz_le[2]) * span[2])
            / length_squared;
        let slack = tolerance / length_squared.sqrt();
        if t < -slack || t > 1.0 + slack {
            return false;
        }
        let le = add3(a.xyz_le, scale3(span, t));
        let chord = a.chord + t * (b.chord - a.chord);
        (point[1] - le[1]).hypot(point[2] - le[2]) <= tolerance
            && point[0] >= le[0] - tolerance
            && point[0] <= le[0] + chord + tolerance
    })
}

/// One panel's four quad-mesh corners and the vortex-lattice quantities
/// derived from them: the per-panel arrays `run` builds and consumes,
/// grouped so the assembly loop reads as one step per panel rather than
/// eight parallel index operations.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::vlm) struct Panel {
    pub(super) normal_direction: [f64; 3],
    pub(super) left_vortex_vertex: [f64; 3],
    pub(super) right_vortex_vertex: [f64; 3],
    pub(super) vortex_center: [f64; 3],
    pub(super) vortex_bound_leg: [f64; 3],
    pub(super) collocation_point: [f64; 3],
    /// Kept alongside the derived quantities above so [`VlmResult::panels`]
    /// can report the raw mesh, not just what the AIC assembly needs, see
    /// [`PanelSample`].
    pub(super) front_left: [f64; 3],
    pub(super) back_left: [f64; 3],
    pub(super) back_right: [f64; 3],
    pub(super) front_right: [f64; 3],
    pub(super) is_trailing_edge: bool,
    pub(super) wing_index: usize,
    /// Half and lattice component, see [`CoreComponent`]. A collocation point
    /// sees the legs of its own half without a core.
    pub(super) core_component: CoreComponent,
    /// Radius of the Rankine core applied to this panel's horseshoe legs when
    /// a collocation point of the other half or another surface of its
    /// lattice component passes near them, see
    /// [`INTER_SURFACE_CORE_FRACTION`].
    pub(super) leg_core_radius: f64,
    /// The same for a collocation point of another lattice component, see
    /// [`OTHER_COMPONENT_CORE_SPACING_FACTOR`].
    pub(super) other_component_core_radius: f64,
}

impl Panel {
    /// Derive one panel's vortex-lattice quantities from its four quad-mesh
    /// corners, in `run`'s own front-left/back-left/back-right/front-right
    /// order.
    pub(in crate::vlm) fn from_quad(
        front_left: [f64; 3],
        back_left: [f64; 3],
        back_right: [f64; 3],
        front_right: [f64; 3],
        is_trailing_edge: bool,
        wing_index: usize,
    ) -> Result<Self, VlmError> {
        let diag1 = sub3(front_right, back_left);
        let diag2 = sub3(front_left, back_right);
        let cross = cross3(diag1, diag2);
        let area_normal = norm3(cross);
        if !area_normal.is_finite() || area_normal <= f64::EPSILON {
            return Err(VlmError::DegeneratePanel { wing_index });
        }
        let normal_direction = scale3(cross, 1.0 / area_normal);

        let left_vortex_vertex = add3(scale3(front_left, 0.75), scale3(back_left, 0.25));
        let right_vortex_vertex = add3(scale3(front_right, 0.75), scale3(back_right, 0.25));
        let vortex_center = scale3(add3(left_vortex_vertex, right_vortex_vertex), 0.5);
        let vortex_bound_leg = sub3(right_vortex_vertex, left_vortex_vertex);

        let panel_chord =
            0.5 * (norm3(sub3(front_left, back_left)) + norm3(sub3(front_right, back_right)));
        let leg_core_radius =
            INTER_SURFACE_CORE_FRACTION * norm3(vortex_bound_leg).min(panel_chord);
        let other_component_core_radius =
            OTHER_COMPONENT_CORE_SPACING_FACTOR * vortex_bound_leg[1].hypot(vortex_bound_leg[2]);

        let collocation_left = add3(scale3(front_left, 0.25), scale3(back_left, 0.75));
        let collocation_right = add3(scale3(front_right, 0.25), scale3(back_right, 0.75));
        let collocation_point = scale3(add3(collocation_left, collocation_right), 0.5);

        Ok(Self {
            normal_direction,
            left_vortex_vertex,
            right_vortex_vertex,
            vortex_center,
            vortex_bound_leg,
            collocation_point,
            front_left,
            back_left,
            back_right,
            front_right,
            is_trailing_edge,
            wing_index,
            core_component: CoreComponent {
                half: 2 * wing_index,
                component: wing_index,
            },
            leg_core_radius,
            other_component_core_radius,
        })
    }
}

/// Mesh every wing on `airplane` into quad panels, exactly as `run`'s own
/// meshing step does: [`Wing::subdivide_sections`] with
/// [`SpacingFunction::Cosspace`] when `spanwise_resolution > 1`, then
/// [`Wing::mesh_thin_surface`] at `chordwise_resolution` with camber.
///
/// With `align` (the product lattice), each horizontal surface downstream of
/// another lattice component also takes a strip boundary at every trailing
/// leg of that component, see [`trailing_leg_stations`]; the frozen
/// reference lattice does not.
pub(super) fn mesh_panels(
    airplane: &Airplane,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
    align: bool,
) -> Result<Vec<Panel>, VlmError> {
    let components = lattice_components(&airplane.wings);
    let mut panels = Vec::new();
    for (wing_index, wing) in airplane.wings.iter().enumerate() {
        let stations = if align {
            trailing_leg_stations(&airplane.wings, &components, wing_index, &panels)
        } else {
            Vec::new()
        };
        mesh_wing_panels(
            wing,
            wing_index,
            spanwise_resolution,
            chordwise_resolution,
            &stations,
            align,
            &mut panels,
        )?;
    }
    assign_lattice_components(&airplane.wings, &mut panels);
    Ok(panels)
}

/// The spanwise subdivision of each lofted section of `wing`: the requested
/// `spanwise_resolution`, reduced where it would cut a section into strips
/// narrower than the camber height of its lattice.
///
/// The lattice lies on the mean camber surface, but each horseshoe's trailing
/// legs leave its bound vortex straight downstream, so downstream collocation
/// points sit off the plane of the legs that pass them by up to the camber
/// height `h` (the range of the camber line at the chordwise lattice stations,
/// times chord). A spanwise pattern of period twice the strip width `w` is
/// seen from a distance `h` attenuated roughly as `exp(-pi h / w)`: once strips
/// are much narrower than `h`, alternating circulations across them barely
/// change any normal wash, and the influence matrix turns singular. Measured:
/// the B747-400 tip edge (sections 0.05 m wide, `h` about 0.085 m at four
/// chordwise panels) gave a lift coefficient of 72 at 2x4 and an
/// ill-conditioned matrix at 8x8; the same lattice with a symmetric section
/// converges at every resolution, and the A320-200 and A340-300 lattices,
/// whose cambered main wings carry inboard sections narrower than their camber
/// height, were ill-conditioned at 8x8 and solve under this rule.
///
/// So a section is split only into strips no narrower than `h`: the largest
/// count up to the request whose narrowest cosine-spaced strip is at least
/// `h` wide, and at least one. A section wide against its camber takes the
/// full request; a one-panel chord (`h = 0`) and resolution 1 change nothing.
fn lattice_section_ratios(
    wing: &Wing,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Vec<usize> {
    let stations = alas_geom::aircraft::spacing::cosspace(0.0, 1.0, chordwise_resolution + 1);
    let camber_height = |xsec: &alas_geom::aircraft::wing::WingXSec| {
        let camber = xsec.airfoil.local_camber(&stations);
        let high = camber.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let low = camber.iter().copied().fold(f64::INFINITY, f64::min);
        (high - low).max(0.0) * xsec.chord.abs()
    };
    wing.xsecs
        .windows(2)
        .map(|pair| {
            let (a, b) = (pair[0].xyz_le, pair[1].xyz_le);
            let extent = (b[1] - a[1]).hypot(b[2] - a[2]);
            let height = camber_height(&pair[0]).max(camber_height(&pair[1]));
            let mut ratio = 1;
            for count in 2..=spanwise_resolution {
                let fractions = alas_geom::aircraft::spacing::cosspace(0.0, 1.0, count + 1);
                let narrowest = fractions
                    .windows(2)
                    .map(|step| step[1] - step[0])
                    .fold(f64::INFINITY, f64::min);
                if extent * narrowest >= height {
                    ratio = count;
                } else {
                    break;
                }
            }
            ratio
        })
        .collect()
}

/// Whether `wing` spans mostly along y, root to tip: a wing or a tailplane,
/// not a fin.
fn spans_along_y(wing: &Wing) -> bool {
    match (wing.xsecs.first(), wing.xsecs.last()) {
        (Some(root), Some(tip)) => {
            let (dy, dz) = (
                tip.xyz_le[1] - root.xyz_le[1],
                tip.xyz_le[2] - root.xyz_le[2],
            );
            dy > dz.abs()
        }
        _ => false,
    }
}

/// The spanwise stations, m, at which surface `index` must place a strip
/// boundary: AVL 3.52 user guide (Drela and Youngren), vortex-spacing rule 1,
/// "surfaces lined up along x should have the same spanwise vortex spacing",
/// the remedy it prefers to a finite core. A trailing leg of an upstream
/// surface that passes between two of a downstream surface's legs passes
/// close to its collocation point; on a strip boundary it sits where the
/// surface's own legs are.
///
/// The stations are the y of the trailing-edge row's bound-vortex vertices
/// of every symmetric surface of another lattice component, already meshed
/// in `panels` (so earlier in the wing list), spanning along y and ending
/// upstream of this surface's root leading edge, that fall strictly inside
/// this surface's span. Empty for a surface that does not span along y or is
/// not symmetric.
pub(super) fn trailing_leg_stations(
    wings: &[Wing],
    components: &[usize],
    index: usize,
    panels: &[Panel],
) -> Vec<f64> {
    let wing = &wings[index];
    let (Some(root), Some(tip)) = (wing.xsecs.first(), wing.xsecs.last()) else {
        return Vec::new();
    };
    if !wing.is_geometry_only() && !wing.symmetric && !spans_along_y(wing) {
        return junction_stations(wings, components, index);
    }
    if wing.is_geometry_only() || !wing.symmetric || !spans_along_y(wing) {
        return Vec::new();
    }
    let (inner, outer) = (root.xyz_le[1], tip.xyz_le[1]);
    let upstream = |other: usize| {
        let surface = &wings[other];
        other != index
            && components[other] != components[index]
            && surface.symmetric
            && spans_along_y(surface)
            && surface
                .xsecs
                .first()
                .is_some_and(|r| r.xyz_le[0] + r.chord < root.xyz_le[0])
    };
    let tolerance = JUNCTION_TOLERANCE * (outer - inner);
    let mut stations: Vec<f64> = panels
        .iter()
        .filter(|panel| {
            panel.is_trailing_edge
                && panel.core_component.half == 2 * panel.wing_index
                && upstream(panel.wing_index)
        })
        .flat_map(|panel| [panel.left_vortex_vertex[1], panel.right_vortex_vertex[1]])
        .filter(|&y| y > inner + tolerance && y < outer - tolerance)
        .collect();
    stations.sort_by(f64::total_cmp);
    stations.dedup_by(|a, b| (*a - *b).abs() <= tolerance);
    stations
}

/// The heights, m, at which a fin must place a strip boundary: the root of
/// every symmetric surface of its own lattice component that is seated on it
/// at the centre plane (a T-tail's stabilizer), strictly inside the fin's
/// span.
///
/// Same rule as [`trailing_leg_stations`], seen from the fin. The stabilizer's
/// root legs run aft along the line `y = 0` at the stabilizer's height, which
/// is the fin's own plane: a leg that lies between two of the fin's strip
/// boundaries passes at a fraction of a strip from the collocation points on
/// either side of it, and the fraction changes with every refinement of either
/// surface. The A400M's stabilizer sits 0.027 m below a boundary of its fin
/// and 0.14 m (0.4 strip) from the collocation points of the strip it cuts: its
/// yawing moment against sideslip then moves by 16 % between tail strip counts
/// of 48 and 96, against 1 % when the stabilizer is seated on a boundary
/// (measured, `examples/t_tail_wake_probe.rs`). On a boundary the legs
/// coincide with the fin's own and every collocation point is half a strip
/// away, AVL 3.52 user guide (Drela and Youngren), vortex-spacing rule 1.
fn junction_stations(wings: &[Wing], components: &[usize], index: usize) -> Vec<f64> {
    let fin = &wings[index];
    let (Some(first), Some(last)) = (fin.xsecs.first(), fin.xsecs.last()) else {
        return Vec::new();
    };
    let (bottom, top) = (
        first.xyz_le[2].min(last.xyz_le[2]),
        first.xyz_le[2].max(last.xyz_le[2]),
    );
    let tolerance = JUNCTION_TOLERANCE * (top - bottom);
    let mut stations: Vec<f64> = wings
        .iter()
        .enumerate()
        .filter(|&(other, surface)| {
            other != index
                && components[other] == components[index]
                && !surface.is_geometry_only()
                && surface.symmetric
                && spans_along_y(surface)
        })
        .filter_map(|(_, surface)| {
            let root = surface.xsecs.first()?;
            let seated = root.xyz_le[1].abs() <= JUNCTION_TOLERANCE * root.chord.max(1.0);
            let height = root.xyz_le[2];
            (seated && height > bottom + tolerance && height < top - tolerance).then_some(height)
        })
        .collect();
    stations.sort_by(f64::total_cmp);
    stations.dedup_by(|a, b| (*a - *b).abs() <= tolerance);
    stations
}

/// `wing` re-stationed so that every one of `stations` (m, ascending) is a
/// strip boundary. Each section where the loft changes (see
/// [`is_straight_loft_station`]) stays where it is and these sections and
/// `stations` cut the span into intervals; each interval takes the number of
/// strips the surface had over it (its share of strip indices, rounded, at
/// least one), placed at even steps of the strip index so the surface's own
/// spacing (cosine toward the tip) carries over. A tail strip is therefore
/// never cut into a sliver beside a trailing leg: between two legs one strip
/// width apart it has the wing's spacing, AVL's rule, and a leg within half
/// the surface's narrowest strip of a loft kink of a tailplane makes no
/// boundary (the kink keeps its station; a fin's junction replaces the
/// kink). New stations lie on the straight loft between the neighbouring sections. Unchanged when the
/// sections do not run strictly outward in y.
fn with_strip_boundaries(wing: &Wing, stations: &[f64]) -> Result<Wing, VlmError> {
    let xsecs = &wing.xsecs;
    // The span coordinate: y for a wing or tailplane, z for a fin.
    let axis = if spans_along_y(wing) { 1 } else { 2 };
    if stations.is_empty()
        || xsecs.len() < 2
        || xsecs
            .windows(2)
            .any(|pair| pair[1].xyz_le[axis] <= pair[0].xyz_le[axis])
    {
        return Ok(wing.clone());
    }
    let ys: Vec<f64> = xsecs.iter().map(|xsec| xsec.xyz_le[axis]).collect();
    let last = ys.len() - 1;
    let tolerance = JUNCTION_TOLERANCE * (ys[last] - ys[0]);
    // The strip-index coordinate: section k sits at k, linear in between.
    let index_at = |y: f64| -> f64 {
        let j = ys
            .windows(2)
            .position(|pair| y < pair[1])
            .unwrap_or(last - 1);
        j as f64 + (y - ys[j]) / (ys[j + 1] - ys[j])
    };
    let y_at = |index: f64| -> f64 {
        let j = (index.floor() as usize).min(last - 1);
        ys[j] + (index - j as f64) * (ys[j + 1] - ys[j])
    };
    // Kept sections (indices) and the leg stations, merged by strip index.
    let mut boundaries: Vec<f64> = (0..=last)
        .filter(|&k| !is_straight_loft_station(xsecs, k, axis))
        .map(|k| k as f64)
        .collect();
    // A fin's junction height that falls within the narrowest strip the fin's
    // own sections already resolve of a loft kink replaces that kink: a
    // boundary a few centimetres from another is a sliver whose long legs
    // lie almost on its neighbour's, and the matrix turns ill-conditioned
    // (ATR72-600 fin 0.010 m, A400M fin 0.027 m from a kink, at 24 chordwise
    // panels pivot ratios of 1e4 to 4e5). The loft moves by that distance at
    // most, along the straight segments either side.
    //
    // A tailplane has the same defect beside a trailing leg: a wing leg that
    // falls a millimetre from a loft kink of the tail (the start of its tip
    // fairing) cuts a strip of that width, whose circulation the lattice
    // cannot determine. Measured on a redesigned A220-300 (tail kink at
    // y = 6.0270 m, wing leg at 6.0275 m): the Trefftz-plane drag of the
    // Goethert-stretched lattice at zero lift rose from 24 to 109 counts and
    // the trimmed table reached the elliptic bound, for a design 0.03 m of
    // wing shift away from one without the strip. There a leg within half
    // the narrowest strip of the surface's own stations of a loft kink makes
    // no boundary: the kink keeps its station, so the tailplane's planform is
    // the one drawn (moving the kink to the leg reshaped it and put the Ames
    // tail-on neutral point of TN D-3797 out of its measured band), and no
    // strip narrower than half of what the surface already has is made.
    let narrowest = if axis == 2 {
        boundaries
            .windows(2)
            .map(|pair| y_at(pair[1]) - y_at(pair[0]))
            .fold(f64::INFINITY, f64::min)
    } else {
        0.5 * ys
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .fold(f64::INFINITY, f64::min)
    };
    // Only the surface's own sections are candidates: a station pushed by an
    // earlier leg is another surface's, and merging into it would drop legs.
    let own = boundaries.len();
    for &y in stations {
        let index = index_at(y);
        if boundaries.iter().all(|&b| (y_at(b) - y).abs() > tolerance) {
            let interior = |b: f64| b > 0.0 && b < last as f64 && (y_at(b) - y).abs() < narrowest;
            let merged = if axis == 2 {
                boundaries[..own].iter().position(|&b| interior(b))
            } else {
                // The nearest kink: the first within reach may be a whole
                // strip inboard of the one the leg stands beside.
                (0..own)
                    .filter(|&position| interior(boundaries[position]))
                    .min_by(|&left, &right| {
                        (y_at(boundaries[left]) - y)
                            .abs()
                            .total_cmp(&(y_at(boundaries[right]) - y).abs())
                    })
            };
            match merged {
                // The fin's junction is a boundary the T-tail root needs; the
                // kink it falls beside gives way.
                Some(position) if axis == 2 => boundaries[position] = index,
                // A tailplane keeps its own loft: the foreign leg beside the
                // kink cuts no strip and the kink's station stands.
                Some(_) => {}
                None => boundaries.push(index),
            }
        }
    }
    boundaries.sort_by(f64::total_cmp);
    let mut aligned = wing.clone();
    aligned.xsecs = Vec::with_capacity(xsecs.len() + stations.len());
    let section_at = |index: f64| -> Result<WingXSec, VlmError> {
        let k = index.round();
        if (index - k).abs() <= 1e-12 {
            return Ok(xsecs[k as usize].clone());
        }
        let j = (index.floor() as usize).min(last - 1);
        Ok(WingXSec::between(
            &xsecs[j],
            &xsecs[j + 1],
            index - j as f64,
        )?)
    };
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let strips = ((end - start).round() as usize).max(1);
        for step in 0..strips {
            aligned.xsecs.push(section_at(
                start + (end - start) * step as f64 / strips as f64,
            )?);
        }
    }
    aligned.xsecs.push(xsecs[last].clone());
    Ok(aligned)
}

/// Whether section `k` is an interior station of a straight loft between its
/// two neighbours, see [`with_strip_boundaries`].
fn is_straight_loft_station(xsecs: &[WingXSec], k: usize, axis: usize) -> bool {
    if k == 0 || k + 1 >= xsecs.len() {
        return false;
    }
    let (a, m, b) = (&xsecs[k - 1], &xsecs[k], &xsecs[k + 1]);
    if m.airfoil != a.airfoil || m.airfoil != b.airfoil {
        return false;
    }
    let s = (m.xyz_le[axis] - a.xyz_le[axis]) / (b.xyz_le[axis] - a.xyz_le[axis]);
    let scale = a.chord.max(b.chord).max(1.0);
    let near = |left: f64, mid: f64, right: f64| {
        (left + s * (right - left) - mid).abs() <= 1e-9 * scale.max(left.abs())
    };
    (0..3).all(|axis| near(a.xyz_le[axis], m.xyz_le[axis], b.xyz_le[axis]))
        && near(a.chord, m.chord, b.chord)
        && near(a.twist, m.twist, b.twist)
}

/// Incidence offset, rad (nose-up positive, see
/// [`incidence::rotated_normal`]), that turns the flow-tangency normal of the
/// panel between chordwise stations `x_nondim[j]` and `x_nondim[j + 1]` of the
/// strip between sections `a` and `b` from the panel-mean camber slope to the
/// camber slope at the panel's collocation point.
///
/// The lattice puts the bound vortex at the quarter and the collocation point
/// at the three-quarter chord of each panel (Katz and Plotkin, Low-Speed
/// Aerodynamics, 2nd ed., sec. 12.1 and the camber example of 12.4: the
/// boundary condition `w = V (dz/dx - alpha)` is applied at the collocation
/// point). The normal of a quad whose corners lie on the camber surface
/// carries the slope averaged over the whole panel, the slope at its
/// midpoint to second order: a quarter panel forward of the collocation
/// point. That lag is first order in the panel width, so thin-airfoil theory
/// (Anderson, Fundamentals of Aerodynamics, eq. 4.61) is approached only as
/// 1 / N: a one-panel parabolic arc gives `alpha_L0 = 0` instead of
/// `-2 h / c`, exact with the slope at the three-quarter point. Here the
/// slope at the collocation point is the secant over a window of one panel
/// width centred on it (clamped at the leading and trailing edges): the
/// panel-mean slope moved a quarter panel aft, second-order accurate for a
/// smooth section and still an average, so it does not pick up the pointwise
/// noise of a tabulated one. Both slopes are taken on the
/// section's own mean camber line in its chord frame, so twist, dihedral and
/// taper act as in the mesh. The result is the mean of the two sections,
/// the collocation point being at mid-strip.
fn collocation_slope_offset(a: &WingXSec, b: &WingXSec, x_nondim: &[f64], j: usize) -> f64 {
    let (front, back) = (x_nondim[j], x_nondim[j + 1]);
    let width = back - front;
    if width <= 0.0 {
        return 0.0;
    }
    let collocation = front + 0.75 * width;
    let window = [
        (collocation - 0.5 * width).max(0.0),
        (collocation + 0.5 * width).min(1.0),
    ];
    let section_lag = |xsec: &WingXSec| {
        let z = xsec
            .airfoil
            .local_camber(&[front, back, window[0], window[1]]);
        let mean = (z[1] - z[0]) / width;
        let at_collocation = (z[3] - z[2]) / (window[1] - window[0]);
        at_collocation.atan() - mean.atan()
    };
    -0.5 * (section_lag(a) + section_lag(b))
}

/// Append one surface's panels in the same order as the full mesh, with a
/// strip boundary at each of `stations` (see [`trailing_leg_stations`]).
/// With `collocation_slope` the flow-tangency normals carry the camber slope
/// at the collocation point, see [`collocation_slope_offset`]; the frozen
/// reference lattice keeps the panel-mean slope.
pub(super) fn mesh_wing_panels(
    wing: &Wing,
    wing_index: usize,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
    stations: &[f64],
    collocation_slope: bool,
    panels: &mut Vec<Panel>,
) -> Result<(), VlmError> {
    // Wingtip devices kept for geometry output only carry no aerodynamic model.
    if wing.is_geometry_only() {
        return Ok(());
    }
    let subdivided;
    let wing_ref: &Wing = if spanwise_resolution > 1 {
        let ratios = lattice_section_ratios(wing, spanwise_resolution, chordwise_resolution);
        subdivided = wing.subdivide_sections_by(&ratios, SpacingFunction::Cosspace)?;
        &subdivided
    } else {
        wing
    };
    let aligned;
    let wing_ref: &Wing = if stations.is_empty() {
        wing_ref
    } else {
        aligned = with_strip_boundaries(wing_ref, stations)?;
        &aligned
    };

    let (points, faces) = wing_ref.mesh_thin_surface(chordwise_resolution, true);
    let x_nondim = alas_geom::aircraft::spacing::cosspace(0.0, 1.0, chordwise_resolution + 1);
    // Upstream's `(arange(len(faces)) + 1) % chordwise_resolution == 0`,
    // evaluated per wing (including its mirrored half, already appended
    // to `faces` by `mesh_thin_surface` when the wing is symmetric)
    // before the per-wing arrays are concatenated.
    // `mesh_thin_surface` appends the mirror image of a symmetric surface as
    // the second half of `faces`.
    let half_count = if wing.symmetric {
        faces.len() / 2
    } else {
        faces.len()
    };
    for (i, face) in faces.iter().enumerate() {
        let is_trailing_edge = (i + 1) % chordwise_resolution == 0;
        let mut panel = Panel::from_quad(
            points[face[0]],
            points[face[1]],
            points[face[2]],
            points[face[3]],
            is_trailing_edge,
            wing_index,
        )?;
        panel.core_component.half = 2 * wing_index + usize::from(i >= half_count);
        if collocation_slope {
            let in_half = i % half_count.max(1);
            let offset = collocation_slope_offset(
                &wing_ref.xsecs[in_half / chordwise_resolution],
                &wing_ref.xsecs[in_half / chordwise_resolution + 1],
                &x_nondim,
                in_half % chordwise_resolution,
            );
            panel.normal_direction = incidence::rotated_normal(&panel, offset);
        }
        panels.push(panel);
    }
    Ok(())
}
