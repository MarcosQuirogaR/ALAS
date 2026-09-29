// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! MSC launchers may return zero after an allocation failure with empty F06.
use std::path::Path;

pub(super) fn runtime_failure(deck: &Path) -> Option<String> {
    let log = deck.with_extension("log");
    let bytes = std::fs::read(&log).ok()?;
    let content = String::from_utf8_lossy(&bytes);
    let failures = failure_lines(&content);
    (!failures.is_empty()).then(|| {
        format!(
            "{} reports a runtime failure:\n{}",
            log.display(),
            failures.join("\n")
        )
    })
}

fn failure_lines(content: &str) -> Vec<&str> {
    content
        .lines()
        .filter(|line| {
            let upper = line.to_ascii_uppercase();
            upper.contains("OPEN CORE MEMORY ALLOCATION FAILED")
                || upper.contains("OPEN CORE ALLOCATION FAILED")
                || upper.contains("ERROR (MM_CORE)")
                || upper.contains("USER FATAL MESSAGE")
                || upper.contains("SYSTEM FATAL MESSAGE")
        })
        .take(5)
        .collect()
}

#[cfg(test)]
// Temporary-file setup failures should fail these diagnostic tests immediately.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn allocation_failure_is_retained_even_when_launcher_reports_analysis_complete() {
        let log = "Error (MM_CORE):Exception: malloc failed\nMAINAL: *** OPEN CORE MEMORY ALLOCATION FAILED *** ERROR = 1\nAnalysis complete 8\nMSC Nastran finished";
        assert_eq!(failure_lines(log).len(), 2);
        assert!(failure_lines("Analysis complete 0\nMSC Nastran finished").is_empty());
    }

    #[test]
    fn non_utf8_solver_banner_does_not_hide_runtime_failure() {
        let directory =
            std::env::temp_dir().join(format!("alas_log_encoding_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let deck = directory.join("case.bdf");
        std::fs::write(
            deck.with_extension("log"),
            b"MSC banner \xff\nOPEN CORE ALLOCATION Failed\n",
        )
        .unwrap();
        assert!(runtime_failure(&deck)
            .unwrap()
            .contains("OPEN CORE ALLOCATION Failed"));
        std::fs::remove_file(deck.with_extension("log")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
