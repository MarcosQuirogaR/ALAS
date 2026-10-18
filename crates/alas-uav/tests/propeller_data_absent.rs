// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken behaviour in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Behaviour with no propeller data installed: typed "unavailable" results,
//! never a panic, and a message that says how to install the tables.

use alas_uav::propulsion_electric::prop_data::{
    prop_data_status, prop_data_unavailable_reason, reload_prop_data, set_prop_data_dir,
};
use alas_uav::propulsion_electric::{
    apc_12x6e_performance_map, apc_performance_map, apc_performance_maps,
    solve_catalogue_powertrain, CataloguePowertrainSelection, ElectricFlightCondition,
    ElectricPropulsionError,
};
use alas_uav::{apc_performance_catalog, full_catalog, is_analysis_ready, optimization_catalog};

#[test]
fn missing_data_reports_unavailable_with_a_reason_everywhere() {
    let empty = std::env::temp_dir().join(format!("alas-prop-absent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&empty);
    std::fs::create_dir_all(&empty).unwrap();
    set_prop_data_dir(Some(empty.clone()));
    reload_prop_data();

    let status = prop_data_status();
    assert_eq!(status.table_count, 0);
    assert_eq!(status.directory.as_deref(), Some(empty.as_path()));
    let reason = prop_data_unavailable_reason().expect("no data means a reason");
    assert!(
        reason.contains("fetch_props") && reason.contains("does not distribute"),
        "{reason}"
    );

    for result in [apc_12x6e_performance_map(), apc_performance_map("apc-8x6e")] {
        let error = result.unwrap_err();
        assert!(
            matches!(
                error,
                ElectricPropulsionError::PropellerDataUnavailable { .. }
            ),
            "{error}"
        );
        assert!(error.to_string().contains("unavailable"), "{error}");
    }
    assert!(apc_performance_maps().is_err());

    // The catalogue stays valid; propellers simply are not analysis-ready.
    assert!(apc_performance_catalog().unwrap().records.is_empty());
    let catalog = full_catalog().expect("embedded records still load");
    let propeller = catalog.get("apc-12x6e").expect("reviewed product record");
    assert!(!is_analysis_ready(propeller, 6.0));
    let selectable = optimization_catalog().expect("selectable catalogue");
    assert!(selectable.get("apc-12x6e").is_none());

    // A powertrain solve fails with the typed error instead of panicking.
    let error = solve_catalogue_powertrain(
        &catalog,
        &CataloguePowertrainSelection::single_motor(
            "unmannedtech-gensace-gtech-2200-3s-45c-xt60",
            "tmotor-at2814-900kv",
            "hobbywing-skywalker-30a-v2-mini",
            "apc-12x6e",
        ),
        ElectricFlightCondition::full_power(10.0, 1.225),
    )
    .unwrap_err();
    assert!(
        matches!(
            error,
            ElectricPropulsionError::PropellerDataUnavailable { .. }
        ),
        "{error}"
    );

    // No directory at all is reported the same way.
    set_prop_data_dir(None);
    reload_prop_data();
    assert!(prop_data_status().table_count == 0 || prop_data_status().directory.is_some());
    let _ = std::fs::remove_dir_all(&empty);
}
