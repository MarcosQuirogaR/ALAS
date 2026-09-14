// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_exec::ToolLocator;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// Discovery is filesystem work: never perform it while painting a frame.
#[derive(Default)]
pub(crate) struct MsesReadiness {
    key: Option<(ToolLocator, String)>,
    rx: Option<Receiver<Option<PathBuf>>>,
    pub path: Option<PathBuf>,
    checked_at: Option<Instant>,
}

impl MsesReadiness {
    pub fn refresh(&mut self, locator: &ToolLocator, configured: &str, enabled: bool) {
        self.refresh_with(locator, configured, enabled, |locator, configured| {
            locator.resolve_mses_dir(Path::new(&configured))
        });
    }

    fn refresh_with(
        &mut self,
        locator: &ToolLocator,
        configured: &str,
        enabled: bool,
        resolve: impl FnOnce(ToolLocator, String) -> Option<PathBuf> + Send + 'static,
    ) {
        if !enabled {
            *self = Self::default();
            return;
        }
        let changed = self
            .key
            .as_ref()
            .is_none_or(|(old_locator, old_path)| old_locator != locator || old_path != configured);
        if changed {
            // Dropping the old receiver prevents a late check from publishing
            // readiness for a path that the user has since replaced.
            *self = Self::default();
            self.key = Some((locator.clone(), configured.to_owned()));
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(path) => {
                    self.path = path;
                    self.rx = None;
                    self.checked_at = Some(Instant::now());
                }
                Err(TryRecvError::Disconnected) => {
                    self.path = None;
                    self.rx = None;
                    self.checked_at = Some(Instant::now());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.rx.is_none()
            && self
                .checked_at
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(5))
        {
            let (tx, rx) = channel();
            let locator = locator.clone();
            let configured = configured.to_owned();
            self.rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(resolve(locator, configured));
            });
        }
    }

    pub fn pending(&self) -> bool {
        self.rx.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screening_readiness_is_nonblocking_reuses_checks_and_discards_stale_paths() {
        let locator = ToolLocator::new("unused", "unused");
        let mut cache = MsesReadiness::default();
        let (release, blocked) = channel();
        let (started, entered) = channel();
        cache.refresh_with(&locator, "old", true, move |_, _| {
            started.send(()).unwrap();
            blocked.recv().unwrap();
            Some(PathBuf::from("old"))
        });
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(cache.pending());
        cache.refresh_with(&locator, "old", true, |_, _| {
            panic!("must not rediscover each frame")
        });
        cache.refresh_with(&locator, "new", true, |_, _| Some(PathBuf::from("new")));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while cache.pending() && Instant::now() < deadline {
            cache.refresh_with(&locator, "new", true, |_, _| panic!("duplicate discovery"));
            std::thread::yield_now();
        }
        assert_eq!(cache.path.as_deref(), Some(Path::new("new")));
        cache.refresh_with(&locator, "new", true, |_, _| {
            panic!("cached result must be reused")
        });
        cache.refresh_with(&locator, "new", false, |_, _| {
            panic!("disabled tools are not probed")
        });
        assert!(!cache.pending());
        assert!(cache.path.is_none());
    }
}
