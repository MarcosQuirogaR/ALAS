// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use alas_exec::ToolLocator;
use serde_json::{json, Value};

use super::{serve_streams, Server, Workspace, PROTOCOL_VERSIONS};

/// The custom-airport registry is process-wide: server tests take turns.
static REGISTRY: Mutex<()> = Mutex::new(());

struct Fixture {
    server: Server,
    dir: PathBuf,
    next_id: u64,
    _turn: MutexGuard<'static, ()>,
}

impl Fixture {
    /// A server on a fresh folder that is both its workspace and its user
    /// data folder, so no test touches the real store.
    fn new(name: &str) -> Self {
        let turn = REGISTRY
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("alas-server-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let locator = ToolLocator::new(&dir, &dir);
        let workspace = Workspace::for_locator(&locator, Some(dir.join("ws")));
        Self {
            server: Server::new(workspace),
            dir,
            next_id: 0,
            _turn: turn,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let line =
            json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params});
        let reply = self.server.handle_line(&line.to_string()).unwrap();
        let reply: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["id"], json!(self.next_id));
        reply
    }

    /// Call a tool; the structured result and whether it is an error.
    fn tool(&mut self, name: &str, arguments: Value) -> (Value, bool) {
        let reply = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &reply["result"];
        let text = result["content"][0]["text"].as_str().unwrap();
        // The text block carries the same value as the structured content.
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap(),
            result["structuredContent"]
        );
        (
            result["structuredContent"].clone(),
            result["isError"] == json!(true),
        )
    }

    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let (value, error) = self.tool(name, arguments);
        assert!(!error, "{name}: {value}");
        value
    }

    fn refused(&mut self, name: &str, arguments: Value) -> String {
        let (value, error) = self.tool(name, arguments);
        assert!(error, "{name} should refuse: {value}");
        value["error"].as_str().unwrap().to_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.shutdown();
        let _ = alas_config::airport_io::replace_custom_airports(Vec::new());
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn initialize_negotiates_the_protocol_revision() {
    let mut fx = Fixture::new("init");
    let reply = fx.request(
        "initialize",
        json!({"protocolVersion": "2025-03-26", "capabilities": {}}),
    );
    assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(reply["result"]["serverInfo"]["name"], "ALAS");
    assert!(reply["result"]["capabilities"]["tools"].is_object());
    let reply = fx.request("initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(reply["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    assert_eq!(fx.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn every_tool_is_listed_with_an_object_schema_and_dispatches() {
    let mut fx = Fixture::new("list");
    let reply = fx.request("tools/list", json!({}));
    let tools = reply["result"]["tools"].as_array().unwrap();
    assert!(tools.len() >= 14);
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(tool["inputSchema"]["type"], "object", "{name}");
        assert!(!tool["description"].as_str().unwrap().is_empty());
        // Every listed tool is known to the dispatcher: a missing argument
        // is a tool error, never "unknown tool".
        let reply = fx.request("tools/call", json!({"name": name, "arguments": {}}));
        assert!(reply.get("error").is_none(), "{name}: {reply}");
    }
}

#[test]
fn protocol_errors_use_the_json_rpc_codes() {
    let mut fx = Fixture::new("errors");
    let reply = |fx: &mut Fixture, line: &str| -> Value {
        serde_json::from_str(&fx.server.handle_line(line).unwrap()).unwrap()
    };
    assert_eq!(reply(&mut fx, "{not json")["error"]["code"], -32700);
    assert_eq!(reply(&mut fx, "[]")["error"]["code"], -32600);
    assert_eq!(
        reply(&mut fx, r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#)["error"]["code"],
        -32600
    );
    assert_eq!(fx.request("no/such", json!({}))["error"]["code"], -32601);
    assert_eq!(
        fx.request("tools/call", json!({"name": "no_such_tool"}))["error"]["code"],
        -32602
    );
    assert_eq!(
        reply(&mut fx, r#"{"jsonrpc":"2.0","id":2,"method":7}"#)["error"]["code"],
        -32600
    );
    assert_eq!(
        reply(&mut fx, r#"{"jsonrpc":"2.0","id":3}"#)["error"]["code"],
        -32600
    );
    // Notifications are never answered, well formed or not; nor are
    // responses (this server sends no requests).
    for line in [
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"method":"notifications/cancelled","params":{"requestId":1}}"#,
        r#"{"jsonrpc":"2.0","method":5}"#,
        r#"{"jsonrpc":"2.0","id":4,"result":{}}"#,
    ] {
        assert!(fx.server.handle_line(line).is_none(), "{line}");
    }
}

#[test]
fn an_over_long_message_is_refused_and_the_next_one_is_read() {
    let mut fx = Fixture::new("long");
    let padding = "x".repeat(usize::try_from(super::MAX_MESSAGE_BYTES).unwrap() + 10);
    let input = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{{\"p\":\"{padding}\"}}}}\n\
         {{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}}\n"
    );
    let mut output = Vec::new();
    assert_eq!(
        serve_streams(&mut fx.server, input.as_bytes(), &mut output),
        0
    );
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32600);
    assert_eq!(lines[1]["id"], 2);
    assert_eq!(lines[1]["result"], json!({}));
}

#[test]
fn the_stream_loop_answers_line_by_line_and_ends_at_end_of_input() {
    let mut fx = Fixture::new("stream");
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n\
                 {\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n\
                 {\"jsonrpc\":\"2.0\",\"id\":\"b\",\"method\":\"ping\"}\n";
    let mut output = Vec::new();
    assert_eq!(
        serve_streams(&mut fx.server, input.as_bytes(), &mut output),
        0
    );
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["id"], "b");
}

#[test]
fn paths_cannot_leave_the_workspace() {
    let mut fx = Fixture::new("sandbox");
    for path in ["../escape.json", "a/../../b.json", "", "./a.json"] {
        let error = fx.refused("create_config", json!({"path": path, "preset": "A320-200"}));
        assert!(
            error.contains("relative to the workspace"),
            "{path}: {error}"
        );
    }
    let absolute = fx.dir.join("abs.json").display().to_string();
    fx.refused("read_config", json!({"path": absolute}));
    fx.refused(
        "start_run",
        json!({"config_path": "x.json", "run_name": "../up", "optimize": false, "baseline": false}),
    );
}

#[test]
fn a_link_inside_the_workspace_cannot_lead_out_of_it() {
    let mut fx = Fixture::new("link");
    let outside = fx.dir.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.json"), "{}").unwrap();
    let workspace = fx.dir.join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    let link = workspace.join("link");
    // A directory junction needs no privilege on Windows; a symbolic link
    // does, so the junction is the link a user can actually make.
    #[cfg(windows)]
    let made = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&outside, &link).is_ok();
    assert!(made, "could not make the link");
    let error = fx.refused("read_config", json!({"path": "link/secret.json"}));
    assert!(error.contains("outside the workspace"), "{error}");
    let error = fx.refused(
        "create_config",
        json!({"path": "link/new.json", "preset": "ATR72-600"}),
    );
    assert!(error.contains("outside the workspace"), "{error}");
    assert!(!outside.join("new.json").exists());
}

#[test]
fn a_configuration_cannot_name_a_program_or_folder() {
    let mut fx = Fixture::new("locations");
    fx.ok(
        "create_config",
        json!({"path": "k.json", "preset": "ATR72-600"}),
    );
    let before = fx.ok("read_config", json!({"path": "k.json"}));
    // An edit that points a solver at another program is refused, and the
    // file stays as it was.
    let error = fx.refused(
        "edit_config",
        json!({"path": "k.json", "patch": {"structures": {
            "run_nastran": true, "nastran_exe_path": "C:/Windows/System32/cmd.exe"}}}),
    );
    assert!(error.contains("structures.nastran_exe_path"), "{error}");
    assert_eq!(fx.ok("read_config", json!({"path": "k.json"})), before);
    for (group, field) in [
        ("mses", "mses_dir"),
        ("mses", "osmap_path"),
        ("structures", "nastran_solver_path"),
        ("structures", "nastran95_dir_path"),
        ("structures", "nastran95_runtime_path"),
        ("structures", "nastran95_rf_stage_path"),
        ("structures", "patran_exe_path"),
        ("mission", "navdata_dir"),
        ("mission", "routes_dir"),
        ("mission", "texture_path"),
    ] {
        let error = fx.refused(
            "edit_config",
            json!({"path": "k.json", "patch": {group: {field: "C:/elsewhere"}}}),
        );
        assert!(error.contains(&format!("{group}.{field}")), "{error}");
    }

    // A document written by other means is refused by the tools that read
    // it, start_run first: no program is started.
    let mut document = before["document"].clone();
    document["structures"] = json!({"nastran_exe_path": "C:/Windows/System32/cmd.exe"});
    document["mission"] = json!({"navdata_dir": "C:/elsewhere"});
    let path = fx.dir.join("ws/k.json");
    super::workspace::write_document(&path, &document).unwrap();
    let error = fx.refused(
        "start_run",
        json!({"config_path": "k.json", "run_name": "k", "optimize": false, "baseline": false}),
    );
    assert!(
        error.contains("structures.nastran_exe_path") && error.contains("mission.navdata_dir"),
        "{error}"
    );
    assert!(!fx.dir.join("ws/runs/k").exists());
    fx.refused("validate_config", json!({"path": "k.json"}));
    // Removing the fields (null in a merge patch) puts the defaults back.
    let edited = fx.ok(
        "edit_config",
        json!({"path": "k.json", "patch": {"structures": {"nastran_exe_path": null},
                                           "mission": {"navdata_dir": null}}}),
    );
    assert!(edited["validation"]["valid"].as_bool().unwrap(), "{edited}");

    // Every preset leaves the locations at their defaults.
    for preset in alas_config::presets::available() {
        let (config, _) = super::workspace::load(&json!({ "preset": preset })).unwrap();
        super::workspace::check_locations(&config)
            .unwrap_or_else(|error| panic!("{preset}: {error}"));
    }
}

#[test]
fn a_configuration_naming_a_simbrief_user_is_not_run() {
    let mut fx = Fixture::new("simbrief");
    fx.ok(
        "create_config",
        json!({"path": "s.json", "preset": "ATR72-600"}),
    );
    fx.ok(
        "edit_config",
        json!({"path": "s.json", "patch": {"mission": {"simbrief_username": "someone"}}}),
    );
    let error = fx.refused(
        "start_run",
        json!({"config_path": "s.json", "run_name": "s", "optimize": false, "baseline": false}),
    );
    assert!(error.contains("network"), "{error}");
    assert!(!fx.dir.join("ws/runs/s").exists());
}

/// Poll `run_id` until it is no longer running.
fn wait_for(fx: &mut Fixture, id: &str) -> Value {
    let clock = Instant::now();
    loop {
        let status = fx.ok("run_status", json!({"run_id": id}));
        if status["status"] != "running" {
            return status;
        }
        assert!(
            clock.elapsed() < Duration::from_secs(900),
            "run did not end: {status}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn a_cancelled_run_stops_and_says_so() {
    let mut fx = Fixture::new("cancel");
    fx.ok(
        "create_config",
        json!({"path": "k.json", "preset": "ATR72-600"}),
    );
    let started = fx.ok(
        "start_run",
        json!({"config_path": "k.json", "run_name": "k", "optimize": false, "baseline": false,
               "parallel": false}),
    );
    let id = started["run_id"].as_str().unwrap().to_owned();
    let cancelled = fx.ok("cancel_run", json!({"run_id": id}));
    assert!(cancelled["note"]
        .as_str()
        .unwrap()
        .contains("stage boundary"));
    let status = wait_for(&mut fx, &id);
    assert_eq!(status["status"], "cancelled", "{status}");
    // A stopped run leaves no results to read as if it had finished.
    assert!(!fx.dir.join("ws/runs/k/results.json").exists());
    fx.refused("cancel_run", json!({"run_id": "run-99"}));
}

#[test]
fn a_configuration_round_trips_through_create_edit_and_validate() {
    let mut fx = Fixture::new("config");
    let presets = fx.ok("list_presets", json!({}));
    assert!(presets["presets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "A320-200"));
    let unknown = fx.refused(
        "create_config",
        json!({"path": "c.yaml", "preset": "NO-SUCH"}),
    );
    assert!(unknown.contains("A320-200"), "{unknown}");

    let created = fx.ok(
        "create_config",
        json!({"path": "cases/a320.yaml", "preset": "A320-200", "mission_template": "commercial_transport"}),
    );
    assert_eq!(created["path"], "cases/a320.yaml");
    assert!(
        created["validation"]["valid"].as_bool().unwrap(),
        "{created}"
    );
    assert!(!created["document"]["mission"]["plan"]["stages"]
        .as_array()
        .unwrap()
        .is_empty());
    fx.refused(
        "create_config",
        json!({"path": "cases/a320.yaml", "preset": "A320-200"}),
    );

    let edited = fx.ok(
        "edit_config",
        json!({"path": "cases/a320.yaml", "patch": {"departure_airport": "LEMD", "mission": {"plan": null}}}),
    );
    assert_eq!(edited["document"]["departure_airport"], "LEMD");
    assert!(edited["document"]["mission"].get("plan").is_none());
    let field = fx.ok(
        "get_effective_config",
        json!({"path": "cases/a320.yaml", "field": "departure_airport"}),
    );
    assert_eq!(field["value"], "LEMD");

    // A patch the schema refuses leaves the file as it was.
    let before = fx.ok("read_config", json!({"path": "cases/a320.yaml"}));
    fx.refused(
        "edit_config",
        json!({"path": "cases/a320.yaml", "patch": {"no_such_group": {"x": 1}}}),
    );
    assert_eq!(
        fx.ok("read_config", json!({"path": "cases/a320.yaml"})),
        before
    );
    assert!(
        fx.ok("validate_config", json!({"path": "cases/a320.yaml"}))["valid"]
            .as_bool()
            .unwrap()
    );
}

/// Case C1 (competition mission LPA-VDE-LPA with a diversion to FUE) entered
/// through the server tools alone, as the automation guide walks it: the
/// three aerodromes as custom airports, the ATR 72-600 preset, the 18-stage
/// plan. Aerodrome data: AENA Spanish AIP, rounded (case inputs).
fn build_c1(fx: &mut Fixture) {
    for (icao, name, lat, lon, elevation_m, runway_m) in [
        ("GCLP", "Gran Canaria", 27.932, -15.387, 24.0, 3_100.0),
        ("GCHI", "El Hierro", 27.815, -17.887, 32.0, 1_250.0),
        ("GCFV", "Fuerteventura", 28.453, -13.864, 26.0, 3_400.0),
    ] {
        fx.ok(
            "add_airport",
            json!({"icao": icao, "name": name, "latitude_deg": lat, "longitude_deg": lon,
                   "elevation_m": elevation_m, "runway_lengths_m": [runway_m], "toda_m": runway_m,
                   "lda_m": runway_m, "source": "AENA, Spanish AIP, rounded (case input)"}),
        );
    }
    let found = fx.ok("list_airports", json!({"query": "GCHI"}));
    assert_eq!(found["airports"][0]["toda_m"], 1_250.0);
    fx.ok(
        "create_config",
        json!({"path": "c1/c1.json", "preset": "ATR72-600"}),
    );
    // Cruise legs: the great-circle LPA-VDE distance less about 40 km of
    // climb and descent, as the acceptance case states it.
    let leg_km = {
        let lpa = alas_config::airports::get("GCLP").unwrap();
        let vde = alas_config::airports::get("GCHI").unwrap();
        alas_route::route::Route::great_circle(lpa, vde, 2).total_distance_m() / 1000.0
    };
    let cruise = json!({"type": "cruise", "extent": {"distance_km": leg_km - 40.0}});
    // The missed approach climbs to the preset profile's last descent level,
    // read back from the server as the acceptance case reads it.
    let missed_ft = fx.ok(
        "get_effective_config",
        json!({"path": "c1/c1.json", "field": "mission.profile.descent_4_altitude_ft"}),
    )["value"]
        .as_f64()
        .unwrap();
    let ft = |m: f64| m / 0.3048;
    let stage = |id: &str, kind: Value| json!({"id": id, "kind": kind});
    let reserve = |id: &str, kind: Value| json!({"id": id, "kind": kind, "reserve": true});
    let plan = json!({
        "name": "C1 competition mission",
        "stages": [
            stage("taxi-lpa", json!({"type": "taxi", "time_min": 8.0})),
            stage("takeoff-lpa", json!({"type": "takeoff"})),
            stage("climb-lpa", json!({"type": "climb", "altitude_ft": 10_000.0})),
            stage("cruise-lpa-vde", cruise.clone()),
            stage("descent-vde", json!({"type": "descent", "altitude_ft": 1_500.0, "agl": true})),
            stage("landing-vde", json!({"type": "landing", "field_elevation_ft": ft(32.0)})),
            stage("taxi-vde", json!({"type": "taxi", "time_min": 5.0})),
            stage("critical-takeoff-vde", json!({"type": "takeoff", "field": {
                "name": "El Hierro", "elevation_ft": ft(32.0), "toda_m": 1_250.0, "isa_deviation_k": 0.0}})),
            stage("climb-vde", json!({"type": "climb", "altitude_ft": 10_000.0})),
            stage("cruise-vde-lpa", cruise),
            stage("atc-hold", json!({"type": "loiter", "time_min": 15.0, "altitude_ft": 5_000.0})),
            stage("descent-lpa", json!({"type": "descent", "altitude_ft": 3_000.0})),
            stage("approach-lpa", json!({"type": "approach", "decision_height_ft": 200.0})),
            reserve("missed-approach", json!({"type": "go_around", "altitude_ft": missed_ft, "agl": true})),
            reserve("divert-fue", json!({"type": "diversion", "distance_km": 86.0 * 1.852, "cruise_altitude_ft": 10_000.0})),
            reserve("final-hold", json!({"type": "loiter", "time_min": 30.0, "altitude_ft": 1_500.0})),
            reserve("landing-fue", json!({"type": "landing"})),
            reserve("taxi-fue", json!({"type": "taxi", "time_min": 5.0})),
        ],
    });
    let edited = fx.ok(
        "edit_config",
        json!({"path": "c1/c1.json", "patch": {
            "departure_airport": "GCLP", "arrival_airport": "GCHI",
            "structures": {"run_nastran": false, "run_patran_export": false},
            "mission": {"plan": plan}}}),
    );
    assert!(edited["validation"]["valid"].as_bool().unwrap(), "{edited}");
    let stages = fx.ok(
        "get_effective_config",
        json!({"path": "c1/c1.json", "field": "mission.plan.stages"}),
    );
    assert_eq!(stages["value"].as_array().unwrap().len(), 18);
}

#[test]
fn case_c1_is_built_through_the_server_tools() {
    let mut fx = Fixture::new("c1-build");
    build_c1(&mut fx);
    // The airports went to the user's store, beside the tool preferences.
    let store = std::fs::read_to_string(fx.dir.join("custom-airports.json")).unwrap();
    assert!(store.contains("GCHI") && store.contains("AENA"));
}

#[test]
fn case_c1_runs_through_the_server_and_reports_every_stage() {
    let mut fx = Fixture::new("c1-run");
    build_c1(&mut fx);
    let started = fx.ok(
        "start_run",
        json!({"config_path": "c1/c1.json", "run_name": "c1", "optimize": false, "baseline": false,
               "parallel": false}),
    );
    let id = started["run_id"].as_str().unwrap().to_owned();
    assert_eq!(started["status"], "running");
    // A second run is refused while this one goes; so is a new airport,
    // which would change the registry the run reads.
    fx.refused("start_run", json!({"config_path": "c1/c1.json", "run_name": "c1b", "optimize": false, "baseline": false}));
    let error = fx.refused(
        "add_airport",
        json!({"icao": "GCXO", "name": "Tenerife Norte", "latitude_deg": 28.48,
               "longitude_deg": -16.34, "elevation_m": 632.0, "runway_lengths_m": [3_400.0],
               "source": "test"}),
    );
    assert!(error.contains("run"), "{error}");
    let status = wait_for(&mut fx, &id);
    assert_eq!(status["status"], "completed", "{status}");
    assert!(status["event_count"].as_u64().unwrap() > 0);

    let results = fx.ok("get_results", json!({"run_name": "c1"}));
    let body = &results["results"];
    assert_eq!(body["schema"], super::runs::RESULTS_SCHEMA);
    let stages = body["mission_stages"].as_array().unwrap();
    assert_eq!(stages.len(), 18, "{body}");
    let vde = stages
        .iter()
        .find(|s| s["id"] == "critical-takeoff-vde")
        .unwrap();
    // The verdict of the acceptance case: a full cabin does not take off
    // from 1,250 m, and the run says so.
    assert_eq!(vde["field_check"]["available_m"], 1_250.0);
    assert_eq!(vde["field_check"]["feasible"], false, "{vde}");
    assert!(
        body["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == "FieldTakeoffDistanceViolation"),
        "{}",
        body["findings"]
    );
    // The numbers are the pipeline's own: a direct run of the same document
    // flies every stage with the same fuel and masses, to the bit.
    let direct = direct_run(&fx, "c1/c1.json");
    let history = direct
        .feasibility
        .mass_balance
        .as_ref()
        .and_then(|balance| balance.history.as_ref())
        .unwrap();
    assert_eq!(history.stages.len(), stages.len());
    for (served, flown) in stages.iter().zip(&history.stages) {
        assert_eq!(served["id"], flown.id.as_str());
        for (key, value) in [
            ("fuel_kg", flown.fuel_kg),
            ("mass_start_kg", flown.mass_start_kg),
            ("mass_end_kg", flown.mass_end_kg),
            ("time_s", flown.time_s),
        ] {
            assert_eq!(served[key].as_f64(), Some(value), "{} {key}", flown.id);
        }
    }
    assert!(results["run_manifest"].is_object());
    let files = results["files"].as_array().unwrap();
    // The files docs/HEADLESS.md says a run writes.
    for written in [
        "results.json",
        "run_manifest.json",
        "design_database.json",
        "payload_layout.json",
        "plots/plot_manifest.json",
    ] {
        let path = format!("runs/c1/{written}");
        assert!(
            files.iter().any(|f| f["path"] == path),
            "{path} not written: {files:?}"
        );
    }
    assert!(files
        .iter()
        .any(|f| f["path"].as_str().unwrap().ends_with(".csv")));
    let svg = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with(".svg"))
        .expect("figures written");
    let figure = fx.ok("read_output_file", json!({"path": svg["path"]}));
    assert!(figure["text"].as_str().unwrap().contains("<svg"));
    fx.refused("start_run", json!({"config_path": "c1/c1.json", "run_name": "c1", "optimize": false, "baseline": false}));
}

/// The workspace document at `path` run straight through the pipeline, as a
/// program linking it would: no server, no command line.
fn direct_run(fx: &Fixture, path: &str) -> alas_pipeline::PipelineResult {
    let document = super::workspace::read_document(&fx.dir.join("ws").join(path)).unwrap();
    let (config, _) = super::workspace::load(&document).unwrap();
    let locator = ToolLocator::new(&fx.dir, &fx.dir);
    let environment = locator.resolve_environment(
        std::path::Path::new(&config.mses.mses_dir),
        std::path::Path::new(&config.structures.nastran_exe_path),
        std::path::Path::new(&config.structures.patran_exe_path),
        std::path::Path::new(""),
        std::path::Path::new(""),
    );
    let options = alas_pipeline::PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    alas_pipeline::DesignPipeline::new(config)
        .run(&options, &environment)
        .unwrap()
}

#[test]
fn every_tool_is_in_the_headless_reference() {
    let reference = include_str!("../../../../docs/HEADLESS.md");
    for tool in super::tools::catalogue() {
        let name = tool["name"].as_str().unwrap();
        assert!(
            reference.contains(&format!("| `{name}` |")),
            "{name} missing from the tool table of docs/HEADLESS.md"
        );
        for argument in tool["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
        {
            let row = reference
                .lines()
                .find(|line| line.starts_with(&format!("| `{name}` |")))
                .unwrap();
            assert!(
                row.contains(&format!("`{argument}`")),
                "{name}: {argument} undocumented"
            );
        }
    }
}
