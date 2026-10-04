// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evidence-preserving collection of RC component retail pages.
//!
//! Crawling keeps evidence and volatile procurement text outside reviewed physics.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use std::time::Duration;
mod sources;
pub use sources::{built_in_sources, collect_sources, SourceAdapter};
mod policy;
pub use policy::CrawlPolicy;

mod crawl;
pub use crawl::*;
mod extract;
use extract::*;
