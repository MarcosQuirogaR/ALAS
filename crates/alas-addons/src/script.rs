// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rhai script add-ons.
//!
//! The engine is built without module loading, `eval`, or any file, network or
//! process function; the host registers none. What a script can do is
//! arithmetic, control flow, strings, arrays and maps, the unit helpers below,
//! and a read-only copy of the inputs and the design summary. Four limits bound
//! a run: wall-clock time, operation count, call depth and the size of any
//! string, array or map (the memory bound: Rhai does not meter bytes, but no
//! single value can grow past the caps below).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rhai::{Dynamic, Engine, EvalAltResult, Map, Scope};
use serde_json::Value;

use crate::error::AddonError;

/// Operation budget of one run.
pub const MAX_OPERATIONS: u64 = 5_000_000;
const MAX_STRING_BYTES: usize = 64 * 1024;
const MAX_ARRAY_LEN: usize = 10_000;
const MAX_MAP_LEN: usize = 1_000;
const MAX_LOG_BYTES: usize = 16 * 1024;

const FT_PER_M: f64 = 1.0 / 0.3048;
const LB_PER_KG: f64 = 1.0 / 0.453_592_37;
const KT_PER_MS: f64 = 3600.0 / 1852.0;

/// Limits applied to one script run.
#[derive(Debug, Clone, Copy)]
pub struct ScriptLimits {
    /// Wall-clock limit.
    pub timeout: Duration,
    /// Operation budget.
    pub max_operations: u64,
}

impl ScriptLimits {
    /// The default operation budget with a caller-chosen timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            max_operations: MAX_OPERATIONS,
        }
    }
}

fn json_to_dynamic(value: &Value) -> Dynamic {
    match value {
        Value::Null => Dynamic::UNIT,
        Value::Bool(flag) => Dynamic::from(*flag),
        Value::Number(number) => Dynamic::from_float(number.as_f64().unwrap_or(f64::NAN)),
        Value::String(text) => Dynamic::from(text.clone()),
        Value::Array(items) => Dynamic::from_array(items.iter().map(json_to_dynamic).collect()),
        Value::Object(entries) => {
            let mut map = Map::new();
            for (key, entry) in entries {
                map.insert(key.as_str().into(), json_to_dynamic(entry));
            }
            Dynamic::from_map(map)
        }
    }
}

fn float_of(value: &Dynamic) -> Option<f64> {
    value
        .as_float()
        .ok()
        .or_else(|| value.as_int().ok().map(|integer| integer as f64))
}

fn build_engine(
    limits: ScriptLimits,
    log: Arc<Mutex<String>>,
    timed_out: Arc<AtomicBool>,
) -> Engine {
    let mut engine = Engine::new();
    engine
        .set_max_operations(limits.max_operations)
        .set_max_call_levels(32)
        .set_max_expr_depths(64, 32)
        .set_max_string_size(MAX_STRING_BYTES)
        .set_max_array_size(MAX_ARRAY_LEN)
        .set_max_map_size(MAX_MAP_LEN)
        .set_max_variables(256)
        .set_max_functions(64)
        .set_strict_variables(true);
    engine.disable_symbol("eval");
    let deadline = Instant::now() + limits.timeout;
    engine.on_progress(move |_| {
        if Instant::now() >= deadline {
            timed_out.store(true, Ordering::Relaxed);
            Some(Dynamic::UNIT)
        } else {
            None
        }
    });
    let print_log = Arc::clone(&log);
    engine.on_print(move |text| append_log(&print_log, text));
    engine.on_debug(move |text, _, _| append_log(&log, text));

    engine.register_fn("ft_to_m", |x: f64| x / FT_PER_M);
    engine.register_fn("m_to_ft", |x: f64| x * FT_PER_M);
    engine.register_fn("ft2_to_m2", |x: f64| x / (FT_PER_M * FT_PER_M));
    engine.register_fn("m2_to_ft2", |x: f64| x * FT_PER_M * FT_PER_M);
    engine.register_fn("lb_to_kg", |x: f64| x / LB_PER_KG);
    engine.register_fn("kg_to_lb", |x: f64| x * LB_PER_KG);
    engine.register_fn("kt_to_ms", |x: f64| x / KT_PER_MS);
    engine.register_fn("ms_to_kt", |x: f64| x * KT_PER_MS);
    engine.register_fn("deg_to_rad", f64::to_radians);
    engine.register_fn("rad_to_deg", f64::to_degrees);
    engine.register_fn("clamp", |x: f64, low: f64, high: f64| x.clamp(low, high));
    engine
}

fn append_log(log: &Mutex<String>, text: &str) {
    if let Ok(mut buffer) = log.lock() {
        if buffer.len() + text.len() < MAX_LOG_BYTES {
            buffer.push_str(text);
            buffer.push('\n');
        }
    }
}

/// Evaluate `source` with `inputs` and a read-only `design` summary.
///
/// The script's last expression must be a map from output name to number
/// (`#{ wing_kg: w }`). Returns the raw numeric map and the captured
/// `print` output.
pub fn run_script(
    source: &str,
    inputs: &BTreeMap<String, f64>,
    design: &Value,
    limits: ScriptLimits,
) -> Result<(BTreeMap<String, f64>, String), AddonError> {
    let log = Arc::new(Mutex::new(String::new()));
    let timed_out = Arc::new(AtomicBool::new(false));
    let engine = build_engine(limits, Arc::clone(&log), Arc::clone(&timed_out));

    let mut input_map = Map::new();
    for (name, value) in inputs {
        input_map.insert(name.as_str().into(), Dynamic::from_float(*value));
    }
    let mut scope = Scope::new();
    scope.push_constant("inputs", input_map);
    scope.push_constant("design", json_to_dynamic(design));

    let result: Result<Dynamic, Box<EvalAltResult>> =
        engine.eval_with_scope::<Dynamic>(&mut scope, source);
    let log_text = log.lock().map(|text| text.clone()).unwrap_or_default();
    let value = match result {
        Ok(value) => value,
        Err(error) => {
            return Err(if timed_out.load(Ordering::Relaxed) {
                AddonError::Timeout(limits.timeout.as_secs_f64())
            } else if matches!(*error, EvalAltResult::ErrorTooManyOperations(_)) {
                AddonError::OperationLimit(limits.max_operations)
            } else {
                AddonError::Script(error.to_string())
            })
        }
    };
    let map = value.try_cast::<Map>().ok_or_else(|| {
        AddonError::Outputs("the script must end with a map such as #{ name: value }".to_owned())
    })?;
    let mut outputs = BTreeMap::new();
    for (key, entry) in map {
        let number = float_of(&entry)
            .ok_or_else(|| AddonError::Outputs(format!("output `{key}` is not a number")))?;
        outputs.insert(key.to_string(), number);
    }
    Ok((outputs, log_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(ms: u64) -> ScriptLimits {
        ScriptLimits::with_timeout(Duration::from_millis(ms))
    }

    fn run(source: &str, ms: u64) -> Result<(BTreeMap<String, f64>, String), AddonError> {
        let inputs = BTreeMap::from([("x".to_owned(), 3.0)]);
        let design = serde_json::json!({"geometry": {"area_m2": 100.0}});
        run_script(source, &inputs, &design, limits(ms))
    }

    #[test]
    fn inputs_design_and_helpers_are_available() {
        let (out, log) = run(
            "print(\"hi\"); #{ y: inputs.x * 2.0 + design.geometry.area_m2, ft: m_to_ft(0.3048), lb: kg_to_lb(0.45359237) }",
            2000,
        )
        .unwrap();
        assert_eq!(out["y"], 106.0);
        assert!((out["ft"] - 1.0).abs() < 1e-12);
        assert!((out["lb"] - 1.0).abs() < 1e-12);
        assert_eq!(log, "hi\n");
    }

    #[test]
    fn an_infinite_loop_times_out() {
        let mut limits = limits(100);
        limits.max_operations = u64::MAX;
        let started = Instant::now();
        let error = run_script("loop { }", &BTreeMap::new(), &Value::Null, limits).unwrap_err();
        assert!(matches!(error, AddonError::Timeout(_)), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn the_operation_budget_stops_a_long_loop_before_the_timeout() {
        let error = run(
            "let i = 0; while i < 1000000000 { i += 1; } #{ y: 1.0 }",
            60_000,
        )
        .unwrap_err();
        assert!(matches!(error, AddonError::OperationLimit(_)), "{error}");
    }

    #[test]
    fn size_and_depth_limits_stop_runaway_values() {
        let long = "let s = \"ab\"; while true { s += s; } #{ y: 1.0 }";
        assert!(matches!(
            run(long, 5000).unwrap_err(),
            AddonError::Script(_)
        ));
        let deep = "fn f(n) { f(n + 1) } f(0); #{ y: 1.0 }";
        assert!(matches!(
            run(deep, 5000).unwrap_err(),
            AddonError::Script(_)
        ));
        let arr = "let a = []; while true { a.push(1); } #{ y: 1.0 }";
        assert!(matches!(run(arr, 5000).unwrap_err(), AddonError::Script(_)));
    }

    #[test]
    fn there_is_no_file_module_eval_or_process_access() {
        for source in [
            "import \"x\" as m; #{ y: 1.0 }",
            "eval(\"1\"); #{ y: 1.0 }",
            "open(\"C:/x\"); #{ y: 1.0 }",
            "read_file(\"x\"); #{ y: 1.0 }",
            "system(\"cmd\"); #{ y: 1.0 }",
            "timestamp(); #{ y: 1.0 }",
        ] {
            assert!(run(source, 2000).is_err(), "{source} was accepted");
        }
    }

    #[test]
    fn the_design_summary_is_read_only_and_output_must_be_a_numeric_map() {
        assert!(run("design.geometry.area_m2 = 1.0; #{ y: 1.0 }", 2000).is_err());
        assert!(run("design = 1; #{ y: 1.0 }", 2000).is_err());
        assert!(matches!(
            run("42", 2000).unwrap_err(),
            AddonError::Outputs(_)
        ));
        assert!(matches!(
            run("#{ y: \"text\" }", 2000).unwrap_err(),
            AddonError::Outputs(_)
        ));
    }
}
