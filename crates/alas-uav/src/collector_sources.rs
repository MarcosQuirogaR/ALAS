// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Publisher-specific URL scopes for the transport-neutral UAV collector.

use serde::{Deserialize, Serialize};

use crate::collector::{
    collect, CollectedCatalogue, CollectorError, CrawlPolicy, PageFetcher, Sleeper,
    DEFAULT_USER_AGENT,
};

/// A transport-neutral, bounded source definition for one publisher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAdapter {
    /// Stable source identifier used in collection artifacts.
    pub source_id: String,
    /// Publisher displayed in provenance.
    pub publisher: String,
    /// Robots policy endpoint.
    pub robots_url: String,
    /// Sitemap endpoint.
    pub sitemap_url: String,
    /// Exact HTTPS hosts allowed for this source.
    pub allowed_hosts: Vec<String>,
    /// Product URL path fragments.
    pub product_path_fragments: Vec<String>,
    /// Category or collection URL path fragments.
    pub category_path_fragments: Vec<String>,
    /// URL/title terms retained for fixed-wing relevance review.
    pub relevance_terms: Vec<String>,
    /// Optional category seeds; empty is valid when the sitemap is sufficient.
    pub seed_category_urls: Vec<String>,
    /// Category page cap.
    pub max_category_pages: usize,
    /// Product page cap.
    pub max_product_pages: usize,
}

impl SourceAdapter {
    /// Convert the source definition into the shared bounded crawl policy.
    pub fn crawl_policy(&self) -> CrawlPolicy {
        CrawlPolicy {
            source_id: self.source_id.clone(),
            publisher: self.publisher.clone(),
            robots_url: self.robots_url.clone(),
            sitemap_url: self.sitemap_url.clone(),
            allowed_hosts: self.allowed_hosts.clone(),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            min_request_interval_ms: 1_500,
            seed_category_urls: self.seed_category_urls.clone(),
            max_category_pages: self.max_category_pages,
            max_product_pages: self.max_product_pages,
            product_path_fragments: self.product_path_fragments.clone(),
            category_path_fragments: self.category_path_fragments.clone(),
            relevance_terms: self.relevance_terms.clone(),
        }
    }
}

/// Source adapters with publicly documented product pages and stable HTML.
pub fn built_in_sources() -> Vec<SourceAdapter> {
    let relevant: Vec<String> = [
        "fixed-wing",
        "motor",
        "propeller",
        "prop",
        "esc",
        "controller",
        "battery",
        "lipo",
        "gps",
        "flight-controller",
        "carbon",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    vec![
        SourceAdapter {
            source_id: "t-motor".to_owned(),
            publisher: "T-MOTOR".to_owned(),
            robots_url: "https://store.tmotor.com/robots.txt".to_owned(),
            sitemap_url: "https://store.tmotor.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["store.tmotor.com".to_owned()],
            product_path_fragments: vec!["/product/".to_owned(), "/goods-".to_owned()],
            category_path_fragments: vec!["/categorys/".to_owned()],
            relevance_terms: relevant.clone(),
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 200,
        },
        SourceAdapter {
            source_id: "hobbywing".to_owned(),
            publisher: "HOBBYWING".to_owned(),
            robots_url: "https://www.hobbywing.com/robots.txt".to_owned(),
            sitemap_url: "https://www.hobbywing.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["www.hobbywing.com".to_owned()],
            product_path_fragments: vec!["/en/products/".to_owned()],
            category_path_fragments: Vec::new(),
            relevance_terms: relevant.clone(),
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 120,
        },
        SourceAdapter {
            source_id: "holybro".to_owned(),
            publisher: "Holybro".to_owned(),
            robots_url: "https://holybro.com/robots.txt".to_owned(),
            sitemap_url: "https://holybro.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["holybro.com".to_owned()],
            product_path_fragments: vec!["/products/".to_owned()],
            category_path_fragments: vec!["/collections/".to_owned()],
            relevance_terms: relevant,
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 120,
        },
        SourceAdapter {
            source_id: "apc-propellers".to_owned(),
            publisher: "APC Propellers".to_owned(),
            robots_url: "https://www.apcprop.com/robots.txt".to_owned(),
            sitemap_url: "https://www.apcprop.com/wp-sitemap-posts-product-1.xml".to_owned(),
            allowed_hosts: vec!["www.apcprop.com".to_owned()],
            product_path_fragments: vec!["/product/".to_owned()],
            category_path_fragments: Vec::new(),
            relevance_terms: vec![
                "propeller".to_owned(),
                "electric".to_owned(),
                "ep".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 120,
        },
        SourceAdapter {
            source_id: "easy-composites".to_owned(),
            publisher: "Easy Composites".to_owned(),
            robots_url: "https://www.easycomposites.co.uk/robots.txt".to_owned(),
            sitemap_url: "https://www.easycomposites.co.uk/sitemap.xml".to_owned(),
            allowed_hosts: vec!["www.easycomposites.co.uk".to_owned()],
            product_path_fragments: vec!["/carbon-fibre-".to_owned(), "/high-strength-".to_owned()],
            category_path_fragments: Vec::new(),
            relevance_terms: vec!["carbon".to_owned(), "foam".to_owned(), "fibre".to_owned()],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 80,
        },
        SourceAdapter {
            source_id: "spektrum".to_owned(),
            publisher: "Spektrum".to_owned(),
            robots_url: "https://www.spektrumrc.com/robots.txt".to_owned(),
            sitemap_url: "https://www.spektrumrc.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["www.spektrumrc.com".to_owned()],
            product_path_fragments: vec!["/product/".to_owned()],
            category_path_fragments: Vec::new(),
            relevance_terms: vec![
                "servo".to_owned(),
                "receiver".to_owned(),
                "gps".to_owned(),
                "telemetry".to_owned(),
                "aircraft".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 150,
        },
        SourceAdapter {
            source_id: "horizon-hobby".to_owned(),
            publisher: "Horizon Hobby".to_owned(),
            robots_url: "https://www.horizonhobby.com/robots.txt".to_owned(),
            sitemap_url: "https://www.horizonhobby.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["www.horizonhobby.com".to_owned()],
            product_path_fragments: vec!["/product/".to_owned()],
            category_path_fragments: Vec::new(),
            relevance_terms: vec![
                "landing-gear".to_owned(),
                "landing".to_owned(),
                "aircraft".to_owned(),
                "servo".to_owned(),
                "battery".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 150,
        },
        SourceAdapter {
            source_id: "robart".to_owned(),
            publisher: "Robart Manufacturing".to_owned(),
            robots_url: "https://robart.com/robots.txt".to_owned(),
            sitemap_url: "https://robart.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["robart.com".to_owned()],
            product_path_fragments: vec!["/products/".to_owned()],
            category_path_fragments: vec!["/collections/".to_owned()],
            relevance_terms: vec![
                "landing".to_owned(),
                "gear".to_owned(),
                "retract".to_owned(),
                "tailwheel".to_owned(),
                "aircraft".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 100,
        },
        SourceAdapter {
            source_id: "sunnysky-usa".to_owned(),
            publisher: "SunnySky USA".to_owned(),
            robots_url: "https://sunnyskyusa.com/robots.txt".to_owned(),
            sitemap_url: "https://sunnyskyusa.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["sunnyskyusa.com".to_owned()],
            product_path_fragments: vec!["/products/".to_owned()],
            category_path_fragments: vec!["/collections/".to_owned()],
            relevance_terms: vec![
                "motor".to_owned(),
                "brushless".to_owned(),
                "prop".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 100,
        },
        SourceAdapter {
            source_id: "gensace".to_owned(),
            publisher: "Gens ace".to_owned(),
            robots_url: "https://gensace.de/robots.txt".to_owned(),
            sitemap_url: "https://gensace.de/sitemap.xml".to_owned(),
            allowed_hosts: vec!["gensace.de".to_owned()],
            product_path_fragments: vec!["/products/".to_owned()],
            category_path_fragments: vec!["/collections/".to_owned()],
            relevance_terms: vec!["battery".to_owned(), "lipo".to_owned(), "mah".to_owned()],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 100,
        },
        SourceAdapter {
            source_id: "emax".to_owned(),
            publisher: "Emax".to_owned(),
            robots_url: "https://emaxmodel.com/robots.txt".to_owned(),
            sitemap_url: "https://emaxmodel.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["emaxmodel.com".to_owned()],
            product_path_fragments: vec!["/products/".to_owned()],
            category_path_fragments: vec!["/collections/".to_owned()],
            relevance_terms: vec![
                "motor".to_owned(),
                "brushless".to_owned(),
                "servo".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 100,
        },
        SourceAdapter {
            source_id: "mateksys".to_owned(),
            publisher: "MATEKSYS".to_owned(),
            robots_url: "https://www.mateksys.com/robots.txt".to_owned(),
            sitemap_url: "https://www.mateksys.com/wp-sitemap.xml".to_owned(),
            allowed_hosts: vec!["www.mateksys.com".to_owned()],
            product_path_fragments: vec!["/?portfolio=".to_owned(), "/downloads/".to_owned()],
            category_path_fragments: Vec::new(),
            relevance_terms: vec![
                "f405".to_owned(),
                "f765".to_owned(),
                "h743".to_owned(),
                "wing".to_owned(),
                "vtol".to_owned(),
                "flight-controller".to_owned(),
                "gps".to_owned(),
                "gnss".to_owned(),
                "matek".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 100,
        },
        SourceAdapter {
            source_id: "tattu".to_owned(),
            publisher: "Tattu".to_owned(),
            robots_url: "https://genstattu.com/robots.txt".to_owned(),
            sitemap_url: "https://genstattu.com/sitemap.xml".to_owned(),
            allowed_hosts: vec!["genstattu.com".to_owned()],
            product_path_fragments: vec!["/tattu-".to_owned(), "/content/instock/".to_owned()],
            category_path_fragments: vec!["/tattu-batteries/".to_owned()],
            relevance_terms: vec![
                "tattu".to_owned(),
                "lipo".to_owned(),
                "battery".to_owned(),
                "22000".to_owned(),
                "16000".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 80,
        },
        SourceAdapter {
            source_id: "radiomaster".to_owned(),
            publisher: "RadioMaster".to_owned(),
            robots_url: "https://www.radiomasterrc.com/robots.txt".to_owned(),
            sitemap_url: "https://www.radiomasterrc.com/sitemap.xml".to_owned(),
            allowed_hosts: vec![
                "www.radiomasterrc.com".to_owned(),
                "radiomasterrc.com".to_owned(),
            ],
            product_path_fragments: vec!["/products/".to_owned()],
            category_path_fragments: Vec::new(),
            relevance_terms: vec![
                "receiver".to_owned(),
                "elrs".to_owned(),
                "pwm".to_owned(),
                "fixed-wing".to_owned(),
                "aircraft".to_owned(),
            ],
            seed_category_urls: Vec::new(),
            max_category_pages: 0,
            max_product_pages: 100,
        },
    ]
}

/// Collect each source in order, preserving independent per-source artifacts.
pub fn collect_sources(
    fetcher: &mut impl PageFetcher,
    sleeper: &mut impl Sleeper,
    sources: &[SourceAdapter],
) -> Result<Vec<CollectedCatalogue>, CollectorError> {
    sources
        .iter()
        .map(|source| collect(fetcher, sleeper, &source.crawl_policy()))
        .collect()
}
