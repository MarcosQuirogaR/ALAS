// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The item-level mass ledger every mass property is computed from.
//!
//! [`crate::breakdown`] answers with ten lumped groups, one point each. That
//! is enough for a first centre of gravity and not for anything downstream
//! of it: an inertia tensor needs each item's own extent, a loading case
//! needs to know which items are fixed, loadable or consumable, and a
//! calibration needs to know where a number came from. The ledger is the
//! representation those questions are answered from. Each [`MassItem`]
//! carries a mass, a reference point, a centroidal inertia tensor, the role
//! that decides which mass states include it, and the method that produced
//! it. [`MassProperties`] combines any set of items into a total mass, a
//! centre of gravity and a tensor about that centre through the parallel-axis
//! theorem, and does nothing else.
//!
//! Axes are the geometry frame every other crate uses: `x` positive aft,
//! `y` positive starboard, `z` positive up: JSBSim's structural frame.
//! Products of inertia are stored as the integrals `P_xy = int x y dm`
//! (JSBSim with `negated_crossproduct_inertia="false"`) and appear negated
//! on the off-diagonal of [`InertiaTensor::matrix`]. AeroSandbox stores the
//! negated integral as its `Ixy` and negates nothing further, so its
//! `inertia_tensor` equals [`InertiaTensor::matrix`] while its `Ixy` is the
//! negative of `pxy` here.

mod error;

pub use error::LedgerError;

/// A symmetric inertia tensor about a stated point, in kg m^2.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InertiaTensor {
    /// Moment of inertia about the x axis.
    pub ixx: f64,
    /// Moment of inertia about the y axis.
    pub iyy: f64,
    /// Moment of inertia about the z axis.
    pub izz: f64,
    /// Product of inertia `int x y dm`.
    pub pxy: f64,
    /// Product of inertia `int x z dm`.
    pub pxz: f64,
    /// Product of inertia `int y z dm`.
    pub pyz: f64,
}

impl InertiaTensor {
    /// A tensor with no inertia: the tensor of a point mass about itself.
    pub const ZERO: Self = Self {
        ixx: 0.0,
        iyy: 0.0,
        izz: 0.0,
        pxy: 0.0,
        pxz: 0.0,
        pyz: 0.0,
    };

    /// A diagonal tensor with no products of inertia.
    pub const fn diagonal(ixx: f64, iyy: f64, izz: f64) -> Self {
        Self {
            ixx,
            iyy,
            izz,
            pxy: 0.0,
            pxz: 0.0,
            pyz: 0.0,
        }
    }

    /// The 3x3 matrix form, products of inertia negated off the diagonal.
    pub fn matrix(&self) -> [[f64; 3]; 3] {
        [
            [self.ixx, -self.pxy, -self.pxz],
            [-self.pxy, self.iyy, -self.pyz],
            [-self.pxz, -self.pyz, self.izz],
        ]
    }

    /// The same tensor about a point displaced by `d` from the centroid of
    /// the mass `mass_kg` it describes.
    ///
    /// `d` is the centroid position relative to the new reference point, so
    /// moving a centroidal tensor to the aircraft centre of gravity passes
    /// `centroid - cg`. Each moment gains `m (d.d - d_i^2)` and each product
    /// gains `m d_i d_j`, which is the parallel-axis theorem in components.
    pub fn translated(&self, mass_kg: f64, d: [f64; 3]) -> Self {
        let [dx, dy, dz] = d;
        Self {
            ixx: self.ixx + mass_kg * (dy * dy + dz * dz),
            iyy: self.iyy + mass_kg * (dx * dx + dz * dz),
            izz: self.izz + mass_kg * (dx * dx + dy * dy),
            pxy: self.pxy + mass_kg * dx * dy,
            pxz: self.pxz + mass_kg * dx * dz,
            pyz: self.pyz + mass_kg * dy * dz,
        }
    }

    /// Component-wise sum, valid only for tensors about the same point.
    pub fn plus(&self, other: &Self) -> Self {
        Self {
            ixx: self.ixx + other.ixx,
            iyy: self.iyy + other.iyy,
            izz: self.izz + other.izz,
            pxy: self.pxy + other.pxy,
            pxz: self.pxz + other.pxz,
            pyz: self.pyz + other.pyz,
        }
    }

    /// Whether every term is finite.
    pub fn is_finite(&self) -> bool {
        [self.ixx, self.iyy, self.izz, self.pxy, self.pxz, self.pyz]
            .iter()
            .all(|value| value.is_finite())
    }

    /// Whether the tensor could describe a real mass distribution.
    ///
    /// The moments must be nonnegative and satisfy the triangle inequalities
    /// `I_xx + I_yy >= I_zz` and permutations, which hold for any body
    /// because each moment is a sum of squared distances the other two
    /// share. A tolerance proportional to the largest moment absorbs
    /// rounding in the sums that produced it.
    pub fn is_physical(&self) -> bool {
        if !self.is_finite() {
            return false;
        }
        let scale = self.ixx.abs().max(self.iyy.abs()).max(self.izz.abs());
        let tolerance = 1.0e-9 * scale.max(1.0e-300);
        let nonnegative =
            self.ixx >= -tolerance && self.iyy >= -tolerance && self.izz >= -tolerance;
        let triangle = self.ixx + self.iyy + tolerance >= self.izz
            && self.iyy + self.izz + tolerance >= self.ixx
            && self.izz + self.ixx + tolerance >= self.iyy;
        nonnegative && triangle
    }
}

/// Total mass, centre of gravity and inertia tensor about that centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProperties {
    /// Total mass, kg.
    pub mass_kg: f64,
    /// Centre of gravity in the geometry frame, m.
    pub cg_m: [f64; 3],
    /// Inertia tensor about the centre of gravity, kg m^2.
    pub inertia_cg: InertiaTensor,
}

impl MassProperties {
    /// The properties of nothing at all.
    pub const EMPTY: Self = Self {
        mass_kg: 0.0,
        cg_m: [0.0; 3],
        inertia_cg: InertiaTensor::ZERO,
    };

    /// A point mass at `position_m`.
    pub const fn point(mass_kg: f64, position_m: [f64; 3]) -> Self {
        Self {
            mass_kg,
            cg_m: position_m,
            inertia_cg: InertiaTensor::ZERO,
        }
    }

    /// Combine several bodies into one.
    ///
    /// The centre of gravity is the mass-weighted mean of the parts and each
    /// part's centroidal tensor is carried to it through the parallel-axis
    /// theorem. An empty or massless set returns [`Self::EMPTY`] rather than
    /// dividing by zero.
    pub fn combine<'a>(parts: impl IntoIterator<Item = &'a MassProperties>) -> Self {
        let parts: Vec<&MassProperties> = parts.into_iter().collect();
        let mass_kg: f64 = parts.iter().map(|part| part.mass_kg).sum();
        if !mass_kg.is_finite() || mass_kg <= 0.0 {
            return Self::EMPTY;
        }
        let mut moment = [0.0; 3];
        for part in &parts {
            for (axis, total) in moment.iter_mut().enumerate() {
                *total += part.mass_kg * part.cg_m[axis];
            }
        }
        let cg_m = [
            moment[0] / mass_kg,
            moment[1] / mass_kg,
            moment[2] / mass_kg,
        ];
        let mut inertia_cg = InertiaTensor::ZERO;
        for part in &parts {
            let d = [
                part.cg_m[0] - cg_m[0],
                part.cg_m[1] - cg_m[1],
                part.cg_m[2] - cg_m[2],
            ];
            inertia_cg = inertia_cg.plus(&part.inertia_cg.translated(part.mass_kg, d));
        }
        Self {
            mass_kg,
            cg_m,
            inertia_cg,
        }
    }

    /// The tensor about an arbitrary reference point.
    pub fn inertia_about(&self, reference_m: [f64; 3]) -> InertiaTensor {
        let d = [
            self.cg_m[0] - reference_m[0],
            self.cg_m[1] - reference_m[1],
            self.cg_m[2] - reference_m[2],
        ];
        self.inertia_cg.translated(self.mass_kg, d)
    }

    /// Radii of gyration `(r_x, r_y, r_z)` about the centre of gravity, m.
    pub fn radii_of_gyration(&self) -> [f64; 3] {
        if self.mass_kg <= 0.0 {
            return [0.0; 3];
        }
        [
            (self.inertia_cg.ixx / self.mass_kg).max(0.0).sqrt(),
            (self.inertia_cg.iyy / self.mass_kg).max(0.0).sqrt(),
            (self.inertia_cg.izz / self.mass_kg).max(0.0).sqrt(),
        ]
    }
}

/// The functional group a ledger item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MassGroup {
    /// Main-wing structure, including high-lift devices and spoilers.
    WingStructure,
    /// Horizontal tail structure.
    HorizontalTail,
    /// Vertical tail structure.
    VerticalTail,
    /// Fuselage structure.
    Fuselage,
    /// Nose and main landing gear.
    LandingGear,
    /// Engines, nacelles, pylons and installation.
    Propulsion,
    /// Avionics, electrical, hydraulics, environmental control, APU and controls.
    Systems,
    /// Seats, monuments, insulation and cabin equipment.
    Furnishings,
    /// Crew, oil, catering, containers and other operating items.
    OperatingItems,
    /// Passengers, baggage and cargo.
    Payload,
    /// Fuel in the tanks.
    Fuel,
}

impl MassGroup {
    /// Stable report label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::WingStructure => "wing structure",
            Self::HorizontalTail => "horizontal tail",
            Self::VerticalTail => "vertical tail",
            Self::Fuselage => "fuselage",
            Self::LandingGear => "landing gear",
            Self::Propulsion => "propulsion",
            Self::Systems => "systems",
            Self::Furnishings => "furnishings",
            Self::OperatingItems => "operating items",
            Self::Payload => "payload",
            Self::Fuel => "fuel",
        }
    }
}

/// Which mass states an item is counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MassRole {
    /// Installed and never removed: part of the manufacturer's empty mass.
    Fixed,
    /// Carried on every flight but not installed: crew, fluids, catering.
    OperatingItem,
    /// Fuel that cannot be delivered to the engines; part of the empty mass.
    UnusableFuel,
    /// Passengers, baggage and cargo for one load case.
    Payload,
    /// Fuel that can be burned; its mass changes with the flight state.
    UsableFuel,
}

impl MassRole {
    /// Whether the role is part of the operating empty mass.
    pub const fn is_operating_empty(self) -> bool {
        matches!(self, Self::Fixed | Self::OperatingItem | Self::UnusableFuel)
    }
}

/// Where a ledger item's mass came from.
///
/// Every variant is a plain tag (the correlation name is a `&'static str`),
/// so this is `Copy`: a method label can be handed to several ledger rows
/// without cloning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MassMethod {
    /// A published empirical correlation, named by source.
    Correlation(&'static str),
    /// A configured fraction of the takeoff mass.
    TakeoffMassFraction,
    /// Integrated from geometry and material densities.
    Geometric,
    /// A manufacturer-published or declared value.
    Declared,
    /// Placed by the payload layout engine.
    LayoutPlacement,
    /// Volume and density of a tank.
    TankFill,
    /// The takeoff-mass closure remainder, which is an allowance and not a
    /// physical estimate.
    ClosureRemainder,
}

/// One row of the ledger.
#[derive(Debug, Clone, PartialEq)]
pub struct MassItem {
    /// Stable identifier, unique within a ledger.
    pub id: String,
    /// Functional group.
    pub group: MassGroup,
    /// Which mass states count it.
    pub role: MassRole,
    /// Mass, kg.
    pub mass_kg: f64,
    /// Reference point in the geometry frame, m.
    pub position_m: [f64; 3],
    /// Centroidal inertia tensor, kg m^2.
    pub local_inertia: InertiaTensor,
    /// How the mass was obtained.
    pub method: MassMethod,
}

impl MassItem {
    /// The item as a body of its own.
    pub fn properties(&self) -> MassProperties {
        MassProperties {
            mass_kg: self.mass_kg,
            cg_m: self.position_m,
            inertia_cg: self.local_inertia,
        }
    }
}

/// The complete list of items.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MassLedger {
    items: Vec<MassItem>,
}

impl MassLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an item.
    pub fn push(&mut self, item: MassItem) {
        self.items.push(item);
    }

    /// Every item, in insertion order.
    pub fn items(&self) -> &[MassItem] {
        &self.items
    }

    /// Mutable access to every item, for state substitution.
    pub fn items_mut(&mut self) -> &mut [MassItem] {
        &mut self.items
    }

    /// Reject a ledger that cannot describe a physical aircraft.
    ///
    /// A negative or non-finite mass is refused rather than clamped: an
    /// empirical correlation that went negative on a degenerate candidate is
    /// a finding about the candidate, and clamping it would hide the finding
    /// behind a plausible centre of gravity.
    pub fn validate(&self) -> Result<(), LedgerError> {
        for (index, item) in self.items.iter().enumerate() {
            if !item.mass_kg.is_finite() || item.mass_kg < 0.0 {
                return Err(LedgerError::InvalidMass {
                    id: item.id.clone(),
                    mass_kg: item.mass_kg,
                });
            }
            if item.position_m.iter().any(|value| !value.is_finite()) {
                return Err(LedgerError::InvalidPosition {
                    id: item.id.clone(),
                });
            }
            if !item.local_inertia.is_physical() {
                return Err(LedgerError::InvalidInertia {
                    id: item.id.clone(),
                });
            }
            if self.items[..index].iter().any(|other| other.id == item.id) {
                return Err(LedgerError::DuplicateId {
                    id: item.id.clone(),
                });
            }
        }
        Ok(())
    }

    /// Combined properties of the items `include` selects.
    pub fn properties_where(&self, include: impl Fn(&MassItem) -> bool) -> MassProperties {
        let parts: Vec<MassProperties> = self
            .items
            .iter()
            .filter(|item| include(item))
            .map(MassItem::properties)
            .collect();
        MassProperties::combine(parts.iter())
    }

    /// Combined properties of every item with one of `roles`.
    pub fn properties_of(&self, roles: &[MassRole]) -> MassProperties {
        self.properties_where(|item| roles.contains(&item.role))
    }

    /// Total mass of the items `include` selects, kg.
    pub fn mass_where(&self, include: impl Fn(&MassItem) -> bool) -> f64 {
        self.items
            .iter()
            .filter(|item| include(item))
            .map(|item| item.mass_kg)
            .sum()
    }

    /// Mass of each group present, in first-appearance order.
    pub fn group_totals(&self) -> Vec<(MassGroup, f64)> {
        let mut totals: Vec<(MassGroup, f64)> = Vec::new();
        for item in &self.items {
            match totals.iter_mut().find(|(group, _)| *group == item.group) {
                Some((_, total)) => *total += item.mass_kg,
                None => totals.push((item.group, item.mass_kg)),
            }
        }
        totals
    }

    /// The operating empty mass: fixed items, operating items and unusable fuel.
    pub fn operating_empty(&self) -> MassProperties {
        self.properties_where(|item| item.role.is_operating_empty())
    }
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
