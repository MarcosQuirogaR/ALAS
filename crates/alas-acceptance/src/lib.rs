// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Acceptance test matrix, cross-preset validation, and benchmark suite for ALAS.
//!
//! This crate exercises the complete ALAS pipeline end-to-end against all
//! published aircraft presets, verifies disciplinary consistency across widebodies
//! and narrowbodies, and measures computational performance against legacy
//! Python references.

pub mod benchmarks;
pub mod matrix;

pub use benchmarks::{
    format_benchmark_report, run_benchmark_suite, BenchmarkResult, BenchmarkSuiteReport,
};
pub use matrix::{
    evaluate_preset, format_matrix_json, format_matrix_report, run_acceptance_matrix,
    AcceptanceMatrixReport, PresetAcceptanceResult, PresetDesignMissionStatus,
};
