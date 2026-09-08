// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::{mark_mses_not_configured, run_bounded_indexed};
    use crate::types::AirfoilCandidateResult;

    #[test]
    fn selected_mses_stage_reports_an_unresolved_installation_per_finalist() {
        let mut candidates = vec![
            AirfoilCandidateResult {
                name: "finalist".to_owned(),
                refined: true,
                ..AirfoilCandidateResult::default()
            },
            AirfoilCandidateResult {
                name: "proxy_only".to_owned(),
                refined: false,
                ..AirfoilCandidateResult::default()
            },
        ];

        mark_mses_not_configured(&mut candidates);

        assert_eq!(candidates[0].mses_status.as_deref(), Some("not_configured"));
        assert!(candidates[0]
            .mses_error
            .as_deref()
            .is_some_and(|message| message.contains("Setup > External Tools")));
        assert!(candidates[1].mses_status.is_none());
    }

    #[test]
    fn bounded_scheduler_returns_index_order_and_keeps_job_errors_isolated() {
        let output = run_bounded_indexed(
            8,
            3,
            Arc::new(AtomicBool::new(false)),
            |index| {
                thread::sleep(Duration::from_millis((8 - index) as u64));
                if index == 3 {
                    Err("controlled failure")
                } else {
                    Ok(index)
                }
            },
            |_, _| {},
        );
        let indices: Vec<usize> = output.iter().map(|(index, _)| *index).collect();
        assert_eq!(indices, (0..8).collect::<Vec<_>>());
        assert!(output.iter().any(|(index, result)| {
            *index == 3
                && result
                    .as_ref()
                    .is_err_and(|error| *error == "controlled failure")
        }));
        assert_eq!(
            output.iter().filter(|(_, result)| result.is_ok()).count(),
            7
        );
    }

    #[test]
    fn bounded_scheduler_caps_concurrency_and_honors_coordinator_cancellation() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut completed = 0;
        let active_for_job = active.clone();
        let maximum_for_job = maximum.clone();
        let cancellation_for_callback = cancellation.clone();
        let output = run_bounded_indexed(
            32,
            3,
            cancellation,
            move |index| {
                let now = active_for_job.fetch_add(1, Ordering::Relaxed) + 1;
                maximum_for_job.fetch_max(now, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(2));
                active_for_job.fetch_sub(1, Ordering::Relaxed);
                index
            },
            move |_, _| {
                completed += 1;
                if completed >= 4 {
                    cancellation_for_callback.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(maximum.load(Ordering::Relaxed) <= 3);
        assert!(output.len() < 32);
        assert!(output.len() >= 4);
    }
}

