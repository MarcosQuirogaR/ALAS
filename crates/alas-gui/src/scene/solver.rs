// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result-figure data selection for the VLM/AVL solver branches.

use alas_opt::OptimizationResult;

use crate::state::AppState;
use crate::views::results_view::SolverResultView;

pub(super) fn selected_result_report<'a>(
    state: &AppState,
    result: &'a alas_pipeline::PipelineResult,
) -> Option<&'a alas_pipeline::AnalysisReport> {
    match state.selected_solver_view {
        SolverResultView::Avl => result
            .solver_optimizations
            .as_ref()
            .and_then(|solutions| solutions.avl.report.as_ref())
            .or(result.optimized_report.as_ref()),
        SolverResultView::Vlm | SolverResultView::Comparison => result
            .solver_optimizations
            .as_ref()
            .and_then(|solutions| solutions.vlm.report.as_ref())
            .or(result.optimized_report.as_ref()),
    }
}

pub(super) fn selected_optimization_result<'a>(
    state: &AppState,
    result: &'a alas_pipeline::PipelineResult,
) -> Option<&'a OptimizationResult> {
    match state.selected_solver_view {
        SolverResultView::Avl => result
            .solver_optimizations
            .as_ref()
            .and_then(|solutions| solutions.avl.optimization.as_ref())
            .or(result.optimization_result.as_ref()),
        SolverResultView::Vlm | SolverResultView::Comparison => result
            .solver_optimizations
            .as_ref()
            .and_then(|solutions| solutions.vlm.optimization.as_ref())
            .or(result.optimization_result.as_ref()),
    }
}

pub(super) fn selected_avl_result<'a>(
    state: &AppState,
    result: &'a alas_pipeline::PipelineResult,
) -> Option<&'a alas_pipeline::AvlAnalysisResult> {
    match state.selected_solver_view {
        SolverResultView::Avl => result
            .solver_optimizations
            .as_ref()
            .and_then(|solutions| solutions.avl.avl_result.as_ref())
            .or(result.avl_result.as_ref()),
        SolverResultView::Vlm | SolverResultView::Comparison => result.avl_result.as_ref(),
    }
}

pub(super) fn solutions_avl_result(
    result: &alas_pipeline::PipelineResult,
) -> Option<&alas_pipeline::AvlAnalysisResult> {
    result
        .solver_optimizations
        .as_ref()
        .and_then(|solutions| solutions.avl.avl_result.as_ref())
}
