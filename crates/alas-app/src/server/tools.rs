// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tool catalogue and the configuration, preset and airport tools. The
//! run tools are in [`super::runs`].

use alas_config::airport_io::{AirportProvenance, AirportProvenanceKind, CustomAirport};
use alas_config::{MissionPlan, RaymerProfile, Severity, TemplateInputs};
use serde_json::{json, Value};

use super::workspace::{self, Workspace};
use super::Server;

/// Metres per foot (exact, international foot).
const M_PER_FT: f64 = 0.3048;

/// Most airports `list_airports` returns.
const AIRPORT_LIST_LIMIT: usize = 50;

/// What a tool call produced.
pub enum Outcome {
    /// No tool has that name.
    Unknown,
    /// The tool's result.
    Done(Value),
    /// The tool ran and refused or failed; the message says why.
    Failed(String),
}

impl From<Result<Value, String>> for Outcome {
    fn from(result: Result<Value, String>) -> Self {
        match result {
            Ok(value) => Self::Done(value),
            Err(message) => Self::Failed(message),
        }
    }
}

/// Dispatch one `tools/call`.
pub fn call(server: &mut Server, name: &str, args: &Value) -> Outcome {
    let ws = &server.workspace;
    let result = match name {
        "list_presets" => Ok(list_presets()),
        "list_mission_templates" => Ok(list_mission_templates()),
        "create_config" => create_config(ws, args),
        "read_config" => read_config(ws, args),
        "get_effective_config" => get_effective_config(ws, args),
        "edit_config" => edit_config(ws, args),
        "validate_config" => validate_config(ws, args),
        "list_airports" => Ok(list_airports(args)),
        "add_airport" => {
            if server.runs.is_running() {
                Err("a run is in progress; add the airport after it ends".to_owned())
            } else {
                add_airport(ws, args)
            }
        }
        _ => return super::runs::call(server, name, args),
    };
    result.into()
}

/// A required string argument.
pub fn str_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("argument {name} (string) is required"))
}

/// An optional boolean argument with its default.
pub fn bool_arg(args: &Value, name: &str, default: bool) -> Result<bool, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("argument {name} must be true or false")),
    }
}

fn f64_arg(args: &Value, name: &str) -> Result<Option<f64>, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| format!("argument {name} must be a number")),
    }
}

fn list_presets() -> Value {
    let presets: Vec<Value> = alas_config::presets::registry()
        .iter()
        .map(|preset| {
            json!({
                "name": preset.name,
                "display_name": preset.display_name,
                "description": preset.description,
            })
        })
        .collect();
    json!({ "presets": presets })
}

fn profile_id(profile: RaymerProfile) -> String {
    serde_json::to_value(profile)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn list_mission_templates() -> Value {
    let templates: Vec<Value> = RaymerProfile::ALL
        .iter()
        .map(|profile| json!({ "id": profile_id(*profile), "title": profile.title() }))
        .collect();
    json!({
        "source": "Raymer, Aircraft Design: A Conceptual Approach, Fig. 3.2",
        "templates": templates,
    })
}

/// The validation result of a loaded configuration.
fn validation(document: &Value) -> Result<Value, String> {
    let (config, notes) = workspace::load(document)?;
    workspace::check_locations(&config)?;
    let issues = alas_config::validate(&config);
    let pick = |severity: Severity| -> Vec<Value> {
        issues
            .iter()
            .filter(|issue| issue.severity == severity)
            .map(|issue| json!({ "field": issue.field_path, "message": issue.message }))
            .collect()
    };
    let errors = pick(Severity::Error);
    Ok(json!({
        "valid": errors.is_empty(),
        "errors": errors,
        "warnings": pick(Severity::Warning),
        "load_notes": notes,
    }))
}

fn create_config(ws: &Workspace, args: &Value) -> Result<Value, String> {
    let path = ws.resolve(str_arg(args, "path")?)?;
    let preset = str_arg(args, "preset")?;
    alas_config::presets::get(preset).map_err(|_| {
        format!(
            "unknown preset {preset:?}; list_presets gives the names: {}",
            alas_config::presets::available().join(", ")
        )
    })?;
    if path.exists() && !bool_arg(args, "overwrite", false)? {
        return Err(format!(
            "{} exists; pass overwrite: true to replace it",
            ws.relative(&path)
        ));
    }
    let mut document = json!({ "preset": preset });
    if let Some(template) = args.get("mission_template").filter(|v| !v.is_null()) {
        let profile: RaymerProfile = serde_json::from_value(template.clone())
            .map_err(|_| "mission_template must be an id from list_mission_templates".to_owned())?;
        let (config, _) = workspace::load(&document)?;
        let altitude_ft = match f64_arg(args, "cruise_altitude_ft")? {
            Some(feet) => feet,
            None => config.requirements.cruise_altitude_m / M_PER_FT,
        };
        let plan = MissionPlan::raymer(
            profile,
            &TemplateInputs::from_policy(&config.fuel_policy, altitude_ft)
                .with_profile(&config.mission.profile),
        );
        document["mission"] =
            json!({ "plan": serde_json::to_value(plan).map_err(|e| e.to_string())? });
    }
    let validation = validation(&document)?;
    workspace::write_document(&path, &document)?;
    Ok(json!({ "path": ws.relative(&path), "document": document, "validation": validation }))
}

fn read_config(ws: &Workspace, args: &Value) -> Result<Value, String> {
    let path = ws.resolve(str_arg(args, "path")?)?;
    Ok(json!({ "path": ws.relative(&path), "document": workspace::read_document(&path)? }))
}

fn get_effective_config(ws: &Workspace, args: &Value) -> Result<Value, String> {
    let path = ws.resolve(str_arg(args, "path")?)?;
    let (config, _) = workspace::load(&workspace::read_document(&path)?)?;
    let mut value = serde_json::to_value(&config).map_err(|e| e.to_string())?;
    let field = args.get("field").and_then(Value::as_str).unwrap_or("");
    for key in field.split('.').filter(|key| !key.is_empty()) {
        value = match value {
            Value::Object(mut map) => map
                .remove(key)
                .ok_or_else(|| format!("field {field:?}: no {key:?} here"))?,
            _ => return Err(format!("field {field:?}: {key:?} is below a value")),
        };
    }
    Ok(json!({ "path": ws.relative(&path), "field": field, "value": value }))
}

fn edit_config(ws: &Workspace, args: &Value) -> Result<Value, String> {
    let path = ws.resolve(str_arg(args, "path")?)?;
    let patch = args
        .get("patch")
        .filter(|patch| patch.is_object())
        .ok_or("argument patch (object, a JSON merge patch) is required")?;
    let mut document = workspace::read_document(&path)?;
    workspace::merge_patch(&mut document, patch);
    // The schema decides before anything is written: a refused edit leaves
    // the file as it was.
    let validation = validation(&document)?;
    workspace::write_document(&path, &document)?;
    Ok(json!({ "path": ws.relative(&path), "document": document, "validation": validation }))
}

fn validate_config(ws: &Workspace, args: &Value) -> Result<Value, String> {
    let path = ws.resolve(str_arg(args, "path")?)?;
    let mut result = validation(&workspace::read_document(&path)?)?;
    result["path"] = json!(ws.relative(&path));
    Ok(result)
}

fn list_airports(args: &Value) -> Value {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let matches: Vec<Value> = alas_config::airports::database_with_custom()
        .into_iter()
        .filter(|airport| {
            query.is_empty()
                || airport.icao.to_ascii_lowercase().contains(&query)
                || airport.name.to_ascii_lowercase().contains(&query)
        })
        .take(AIRPORT_LIST_LIMIT)
        .map(|airport| {
            json!({
                "icao": airport.icao,
                "name": airport.name,
                "elevation_m": airport.elevation_m,
                "toda_m": airport.toda_m,
                "lda_m": airport.lda_m,
                "isa_deviation_c": airport.isa_deviation_c,
                "latitude_deg": airport.latitude_deg,
                "longitude_deg": airport.longitude_deg,
            })
        })
        .collect();
    json!({ "limit": AIRPORT_LIST_LIMIT, "airports": matches })
}

fn add_airport(ws: &Workspace, args: &Value) -> Result<Value, String> {
    let number = |name: &str| f64_arg(args, name)?.ok_or(format!("argument {name} is required"));
    let runways: Vec<f64> = serde_json::from_value(
        args.get("runway_lengths_m")
            .cloned()
            .ok_or("argument runway_lengths_m (array of metres) is required")?,
    )
    .map_err(|_| "runway_lengths_m must be an array of numbers".to_owned())?;
    let airport = CustomAirport {
        icao: str_arg(args, "icao")?.to_owned(),
        name: str_arg(args, "name")?.to_owned(),
        latitude_deg: number("latitude_deg")?,
        longitude_deg: number("longitude_deg")?,
        isa_delta_c: f64_arg(args, "isa_deviation_c")?.unwrap_or(0.0),
        altitude_m: number("elevation_m")?,
        runway_lengths_m: runways,
        declared_toda_m: f64_arg(args, "toda_m")?,
        declared_lda_m: f64_arg(args, "lda_m")?,
        provenance: AirportProvenance {
            kind: AirportProvenanceKind::UserEntered,
            source: Some(str_arg(args, "source")?.to_owned()),
            note: None,
        },
        departure_surveys: Vec::new(),
    };
    let count = crate::custom_airports::add_to_store(&ws.airport_store(), airport)?;
    Ok(json!({ "stored": count, "store": ws.airport_store().display().to_string() }))
}

/// One catalogue entry: a tool and the JSON Schema of its arguments.
pub fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

/// Every tool with its input schema, as `tools/list` returns them.
pub fn catalogue() -> Vec<Value> {
    let path = json!({ "type": "string", "description": "Path relative to the workspace" });
    let config_path = json!({ "type": "string", "description": "Configuration file (.json/.yaml) relative to the workspace" });
    let mut tools = vec![
        tool("list_presets", "Registered aircraft presets: name (the configuration key), display name, description.", json!({}), &[]),
        tool("list_mission_templates", "Mission plan templates (Raymer's example mission profiles) for create_config.", json!({}), &[]),
        tool(
            "create_config",
            "Write a new configuration file selecting a preset, optionally with a template mission plan; returns the document and its validation.",
            json!({
                "path": config_path,
                "preset": { "type": "string" },
                "mission_template": { "type": "string" },
                "cruise_altitude_ft": { "type": "number", "description": "Template cruise altitude, ft; default the configuration's cruise altitude" },
                "overwrite": { "type": "boolean" },
            }),
            &["path", "preset"],
        ),
        tool("read_config", "The configuration document as written in the file.", json!({ "path": config_path }), &["path"]),
        tool(
            "get_effective_config",
            "The configuration after loading (defaults and preset applied), or one dotted field of it such as mission.plan.",
            json!({ "path": config_path, "field": { "type": "string" } }),
            &["path"],
        ),
        tool(
            "edit_config",
            "Apply a JSON merge patch (RFC 7386: objects merge, null deletes, arrays replace) to a configuration file. Refused, and the file left unchanged, when the result does not load.",
            json!({ "path": config_path, "patch": { "type": "object" } }),
            &["path", "patch"],
        ),
        tool("validate_config", "Load a configuration and list its validation errors (block a run) and warnings.", json!({ "path": config_path }), &["path"]),
        tool(
            "list_airports",
            "Airports the route can use (curated table plus the user's custom airports) matching a query on ICAO code or name.",
            json!({ "query": { "type": "string" } }),
            &[],
        ),
        tool(
            "add_airport",
            "Add or replace a custom airport in the user's store (shared with the desktop application). State where the data come from in source.",
            json!({
                "icao": { "type": "string" },
                "name": { "type": "string" },
                "latitude_deg": { "type": "number" },
                "longitude_deg": { "type": "number" },
                "elevation_m": { "type": "number" },
                "runway_lengths_m": { "type": "array", "items": { "type": "number" } },
                "toda_m": { "type": "number" },
                "lda_m": { "type": "number" },
                "isa_deviation_c": { "type": "number" },
                "source": { "type": "string" },
            }),
            &["icao", "name", "latitude_deg", "longitude_deg", "elevation_m", "runway_lengths_m", "source"],
        ),
    ];
    tools.extend(super::runs::catalogue(&path, &config_path));
    tools
}
