// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

const SEED_CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/rc_innovations_seed.json"
));
const REVIEWED_CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/rc_innovations_reviewed.json"
));
const MULTI_SOURCE_CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/multi_source_catalog.json"
));

/// A normalized component catalogue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    /// Schema revision for forward-compatible importers.
    pub schema_version: u32,
    /// Physical component records in stable source order.
    pub records: Vec<ComponentRecord>,
}

/// A catalogue import or validation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct CatalogError {
    message: String,
}

impl CatalogError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Catalog {
    /// Parse and validate a normalized JSON catalogue.
    ///
    /// This is the stable import seam for a scraper or a hand-reviewed data
    /// update. Network access stays outside the physics crate, so a shipped
    /// aircraft analysis never depends on a retailer being reachable.
    pub fn from_json(json: &str) -> Result<Self, CatalogError> {
        let catalog: Self = serde_json::from_str(json)
            .map_err(|error| CatalogError::new(format!("catalog JSON is invalid: {error}")))?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Parse and validate normalized catalogue JSON from a reader.
    pub fn from_reader(mut reader: impl Read) -> Result<Self, CatalogError> {
        let mut json = String::new();
        reader
            .read_to_string(&mut json)
            .map_err(|error| CatalogError::new(format!("catalog could not be read: {error}")))?;
        Self::from_json(&json)
    }

    /// Check schema, provenance, identifiers, and physically possible values.
    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.schema_version != 1 {
            return Err(CatalogError::new(format!(
                "unsupported catalogue schema version {}",
                self.schema_version
            )));
        }
        let mut ids = std::collections::BTreeSet::new();
        for record in &self.records {
            record.validate()?;
            if !ids.insert(record.id.as_str()) {
                return Err(CatalogError::new(format!(
                    "duplicate component id '{}'",
                    record.id
                )));
            }
        }
        Ok(())
    }

    /// Find one record by its stable catalogue identifier.
    pub fn get(&self, id: &str) -> Option<&ComponentRecord> {
        self.records.iter().find(|record| record.id == id)
    }
}

/// The reviewed RC Innovations seed catalogue embedded in the binary.
pub fn seed_catalog() -> Result<&'static Catalog, &'static CatalogError> {
    static CATALOG: OnceLock<Result<Catalog, CatalogError>> = OnceLock::new();
    CATALOG
        .get_or_init(|| Catalog::from_json(SEED_CATALOG_JSON))
        .as_ref()
}

/// The expanded, human-reviewed RC Innovations catalogue embedded in the binary.
pub fn reviewed_catalog() -> Result<&'static Catalog, &'static CatalogError> {
    static CATALOG: OnceLock<Result<Catalog, CatalogError>> = OnceLock::new();
    CATALOG
        .get_or_init(|| Catalog::from_json(REVIEWED_CATALOG_JSON))
        .as_ref()
}

/// The source-expanded catalogue assembled from manufacturer and specialist
/// supplier pages outside the RC Innovations seed.
pub fn multi_source_catalog() -> Result<&'static Catalog, &'static CatalogError> {
    static CATALOG: OnceLock<Result<Catalog, CatalogError>> = OnceLock::new();
    CATALOG
        .get_or_init(|| Catalog::from_json(MULTI_SOURCE_CATALOG_JSON))
        .as_ref()
}

/// Propeller records derived from the tables in the user's local data directory.
///
/// These records establish geometry (from the table title) and the existence of
/// a bounded Ct/Cp table.  They intentionally leave purchased mass, hub
/// geometry, and non-electric compatibility unknown.  The catalogue is empty
/// when no tables are installed (see `propulsion_electric::prop_data`) and is
/// a snapshot of the directory as read on first use.
pub fn apc_performance_catalog() -> Result<&'static Catalog, &'static CatalogError> {
    let loaded = crate::propulsion_electric::prop_data::loaded_prop_tables();
    loaded
        .catalog
        .get_or_init(|| {
            let known = multi_source_catalog().map_err(|error| (*error).clone())?;
            let covered = |id: &str| {
                let stripped = id.strip_suffix('e').unwrap_or(id);
                [id, stripped]
                    .into_iter()
                    .any(|key| known.get(key).is_some() || known.get(&format!("{key}e")).is_some())
            };
            let records = loaded
                .tables
                .iter()
                .filter(|table| !covered(table.propeller_id()))
                .map(derived_propeller_record)
                .collect();
            let catalog = Catalog {
                schema_version: 1,
                records,
            };
            catalog.validate()?;
            Ok(catalog)
        })
        .as_ref()
}

fn derived_propeller_record(
    table: &crate::propulsion_electric::PropellerPerformanceMap,
) -> ComponentRecord {
    ComponentRecord {
        id: table.propeller_id().to_owned(),
        manufacturer: "APC Propellers".to_owned(),
        model: table.model().to_owned(),
        kind: ComponentKind::Propeller(PropellerSpec {
            diameter_m: Some(table.diameter_m()),
            pitch_m: Some(table.pitch_m()),
            blade_count: None,
            mass_kg: None,
            bore_diameter_m: None,
            electric_compatible: table.model().to_ascii_uppercase().contains('E').then_some(true),
        }),
        provenance: Provenance {
            publisher: "APC Propellers".to_owned(),
            source_url: "https://www.apcprop.com/technical-information/performance-data/".to_owned(),
            source_title: format!("APC performance data: {}", table.source_file()),
            transformations: vec![
                "diameter and pitch multiplied by 0.0254 from the performance-file title in inches to m".to_owned(),
                "Ct/Cp tables are read from the user's local propeller data directory without extrapolation".to_owned(),
                "E-series electric compatibility is retained only when the model designation contains E; other compatibility fields remain absent".to_owned(),
            ],
        },
    }
}

/// Merge the reviewed RC Innovations records with the independently sourced
/// manufacturer records while rejecting duplicate identifiers.
pub fn full_catalog() -> Result<Catalog, CatalogError> {
    let reviewed = reviewed_catalog().map_err(|error| (*error).clone())?;
    let multi_source = multi_source_catalog().map_err(|error| (*error).clone())?;
    let apc_performance = apc_performance_catalog().map_err(|error| (*error).clone())?;
    let mut records = reviewed.records.clone();
    records.extend(multi_source.records.iter().cloned());
    records.extend(apc_performance.records.iter().cloned());
    let catalog = Catalog {
        schema_version: reviewed.schema_version,
        records,
    };
    catalog.validate()?;
    Ok(catalog)
}

/// Borrow the complete embedded catalogue for GUI and optimizer selection.
pub fn optimization_catalog() -> Result<&'static Catalog, &'static CatalogError> {
    static CATALOG: OnceLock<Result<Catalog, CatalogError>> = OnceLock::new();
    CATALOG
        .get_or_init(|| full_catalog().map(|source| crate::selectable_catalog(&source, 6.0)))
        .as_ref()
}

/// One retailer or manufacturer page and the normalization applied to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Publisher whose page states the values.
    pub publisher: String,
    /// Stable product page URL.
    pub source_url: String,
    /// Human-readable page title used during review.
    pub source_title: String,
    /// Explicit unit conversions or interpretation limits.
    #[serde(default)]
    pub transformations: Vec<String>,
}

/// A component with physical data and a traceable source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentRecord {
    /// Stable, lowercase catalogue key.
    pub id: String,
    /// Manufacturer printed on the product page.
    pub manufacturer: String,
    /// Product or selected variant name.
    pub model: String,
    /// Typed physical specification.
    #[serde(flatten)]
    pub kind: ComponentKind,
    /// Source and normalization record.
    pub provenance: Provenance,
}

impl ComponentRecord {
    fn validate(&self) -> Result<(), CatalogError> {
        if self.id.is_empty()
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(CatalogError::new(format!(
                "component id '{}' is not a lowercase slug",
                self.id
            )));
        }
        if self.manufacturer.trim().is_empty() || self.model.trim().is_empty() {
            return Err(CatalogError::new(format!(
                "component '{}' has no manufacturer or model",
                self.id
            )));
        }
        if self.provenance.publisher.trim().is_empty()
            || self.provenance.source_title.trim().is_empty()
            || !self.provenance.source_url.starts_with("https://")
        {
            return Err(CatalogError::new(format!(
                "component '{}' has incomplete HTTPS provenance",
                self.id
            )));
        }
        self.kind.validate(&self.id)
    }
}

/// Physical component families understood by the UAV feasibility layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec", rename_all = "snake_case")]
pub enum ComponentKind {
    /// Rechargeable propulsion battery.
    Battery(BatterySpec),
    /// Brushless electric motor.
    Motor(MotorSpec),
    /// Electronic speed controller and optional BEC.
    Esc(EscSpec),
    /// Position actuator for a control surface or mechanism.
    Servo(ServoSpec),
    /// Fixed or folding propeller.
    Propeller(PropellerSpec),
    /// Purchased sheet or stock material.
    MaterialStock(MaterialStockSpec),
    /// Radio receiver with optional integrated telemetry.
    Receiver(ReceiverSpec),
    /// Telemetry sensor or other avionics item.
    Electronics(ElectronicsSpec),
    /// Purchased landing-gear assembly or mechanism.
    LandingGear(LandingGearSpec),
}

impl ComponentKind {
    fn validate(&self, id: &str) -> Result<(), CatalogError> {
        match self {
            Self::Battery(spec) => spec.validate(id),
            Self::Motor(spec) => spec.validate(id),
            Self::Esc(spec) => spec.validate(id),
            Self::Servo(spec) => spec.validate(id),
            Self::Propeller(spec) => spec.validate(id),
            Self::MaterialStock(spec) => spec.validate(id),
            Self::Receiver(spec) => spec.validate(id),
            Self::Electronics(spec) => spec.validate(id),
            Self::LandingGear(spec) => spec.validate(id),
        }
    }
}

/// Axis-aligned component dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dimensions {
    /// Length in metres.
    pub length_m: f64,
    /// Width in metres.
    pub width_m: f64,
    /// Height or thickness in metres.
    pub height_m: f64,
}

impl Dimensions {
    pub(crate) fn validate(self, id: &str) -> Result<(), CatalogError> {
        if positive(self.length_m) && positive(self.width_m) && positive(self.height_m) {
            Ok(())
        } else {
            Err(CatalogError::new(format!(
                "component '{id}' has invalid dimensions"
            )))
        }
    }
}

/// Battery nameplate values, normalized to SI and ampere-hours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatterySpec {
    /// Cell chemistry, such as `LiPo`.
    pub chemistry: String,
    /// Series cell count.
    pub series_cells: Option<u16>,
    /// Pack nominal voltage.
    pub nominal_voltage_v: Option<f64>,
    /// Rated charge capacity.
    pub capacity_ah: Option<f64>,
    /// Published discharge C rating, without reinterpreting it as pulse data.
    pub discharge_rating_c: Option<f64>,
    /// Pack mass.
    pub mass_kg: Option<f64>,
    /// Pack outer dimensions.
    pub dimensions: Option<Dimensions>,
    /// Main power connector.
    pub connector: Option<String>,
}

impl BatterySpec {
    /// Nominal stored energy from the nameplate voltage and capacity.
    pub fn nominal_energy_wh(&self) -> Option<f64> {
        Some(self.nominal_voltage_v? * self.capacity_ah?)
    }

    /// Rated discharge current from capacity times the published C rating.
    pub fn rated_discharge_current_a(&self) -> Option<f64> {
        Some(self.capacity_ah? * self.discharge_rating_c?)
    }

    fn validate(&self, id: &str) -> Result<(), CatalogError> {
        validate_optional_positive(id, "nominal voltage", self.nominal_voltage_v)?;
        validate_optional_positive(id, "capacity", self.capacity_ah)?;
        validate_optional_positive(id, "discharge rating", self.discharge_rating_c)?;
        validate_optional_positive(id, "mass", self.mass_kg)?;
        if self.series_cells == Some(0) || self.chemistry.trim().is_empty() {
            return Err(CatalogError::new(format!(
                "battery '{id}' has invalid cell count or chemistry"
            )));
        }
        if let Some(dimensions) = self.dimensions {
            dimensions.validate(id)?;
        }
        Ok(())
    }
}

/// Motor limits published for one selected winding variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorSpec {
    /// No-load speed constant in revolutions per minute per volt.
    pub kv_rpm_per_v: Option<f64>,
    /// DC winding resistance at the manufacturer-stated test condition.
    pub winding_resistance_ohm: Option<f64>,
    /// No-load current at the manufacturer-stated test voltage.
    pub no_load_current_a: Option<f64>,
    /// Voltage associated with the published no-load current.
    pub no_load_test_voltage_v: Option<f64>,
    /// Minimum supported LiPo series count.
    pub min_series_cells: Option<u16>,
    /// Maximum supported LiPo series count.
    pub max_series_cells: Option<u16>,
    /// Published maximum current.
    pub max_current_a: Option<f64>,
    /// Published maximum electrical power.
    pub max_power_w: Option<f64>,
    /// Published maximum static thrust, normalized to newtons.
    pub max_static_thrust_n: Option<f64>,
    /// Motor mass including cables when the source says so.
    pub mass_kg: Option<f64>,
    /// Motor outer dimensions when the source supplies a bounding cylinder.
    pub dimensions: Option<Dimensions>,
    /// Recommended propeller diameter.
    pub recommended_propeller_diameter_m: Option<f64>,
    /// Recommended propeller pitch.
    pub recommended_propeller_pitch_m: Option<f64>,
}

impl MotorSpec {
    fn validate(&self, id: &str) -> Result<(), CatalogError> {
        for (field, value) in [
            ("Kv", self.kv_rpm_per_v),
            ("winding resistance", self.winding_resistance_ohm),
            ("no-load current", self.no_load_current_a),
            ("no-load test voltage", self.no_load_test_voltage_v),
            ("maximum current", self.max_current_a),
            ("maximum power", self.max_power_w),
            ("maximum static thrust", self.max_static_thrust_n),
            ("mass", self.mass_kg),
            (
                "recommended propeller diameter",
                self.recommended_propeller_diameter_m,
            ),
            (
                "recommended propeller pitch",
                self.recommended_propeller_pitch_m,
            ),
        ] {
            validate_optional_positive(id, field, value)?;
        }
        validate_cell_range(id, self.min_series_cells, self.max_series_cells).and_then(|()| {
            match self.dimensions {
                Some(dimensions) => dimensions.validate(id),
                None => Ok(()),
            }
        })
    }
}

/// ESC electrical limits and optional regulated receiver supply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EscSpec {
    /// Minimum supported LiPo series count.
    pub min_series_cells: Option<u16>,
    /// Maximum supported LiPo series count.
    pub max_series_cells: Option<u16>,
    /// Continuous motor current rating.
    pub continuous_current_a: Option<f64>,
    /// Minimum adjustable BEC output voltage.
    pub bec_min_voltage_v: Option<f64>,
    /// Maximum adjustable BEC output voltage.
    pub bec_max_voltage_v: Option<f64>,
    /// Continuous BEC current when explicitly published.
    pub bec_continuous_current_a: Option<f64>,
    /// Peak BEC current when explicitly published.
    pub bec_peak_current_a: Option<f64>,
    /// Controller mass including cables when the source says so.
    pub mass_kg: Option<f64>,
    /// Outer dimensions.
    pub dimensions: Option<Dimensions>,
}
