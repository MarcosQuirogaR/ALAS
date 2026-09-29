// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded URL and request policy shared by every UAV source adapter.

use serde::{Deserialize, Serialize};

use super::{ensure_allowed_url, is_category_url, CollectorError, DEFAULT_USER_AGENT};

/// Restrictions applied to one crawl.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrawlPolicy {
    /// Stable identifier for the publisher-specific source adapter.
    pub source_id: String,
    /// Publisher name retained in collection artifacts.
    pub publisher: String,
    /// Robots policy checked before sitemap or product requests.
    pub robots_url: String,
    /// Sitemap from which product URLs are discovered.
    pub sitemap_url: String,
    /// Exact HTTPS hosts the collector may contact.
    pub allowed_hosts: Vec<String>,
    /// HTTP user-agent sent on every request.
    pub user_agent: String,
    /// Minimum delay between successive requests.
    pub min_request_interval_ms: u64,
    /// Category pages used to supplement a sitemap that omits products.
    pub seed_category_urls: Vec<String>,
    /// Hard cap across seed and pagination category pages.
    pub max_category_pages: usize,
    /// Hard page cap after sitemap discovery.
    pub max_product_pages: usize,
    /// URL path fragments that identify product pages for this source.
    pub product_path_fragments: Vec<String>,
    /// URL path fragments that identify category or collection pages.
    pub category_path_fragments: Vec<String>,
    /// Product-title and URL terms that make a page relevant to fixed-wing UAVs.
    pub relevance_terms: Vec<String>,
}

impl Default for CrawlPolicy {
    fn default() -> Self {
        Self {
            source_id: "rc-innovations".to_owned(),
            publisher: "RC Innovations".to_owned(),
            robots_url: "https://rc-innovations.es/robots.txt".to_owned(),
            sitemap_url: "https://rc-innovations.es/sitemap.xml".to_owned(),
            allowed_hosts: vec!["rc-innovations.es".to_owned()],
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            min_request_interval_ms: 1_500,
            seed_category_urls: vec![
                "https://rc-innovations.es/shop/category/baterias-lipo-167".to_owned(),
                "https://rc-innovations.es/shop/category/motores-t-motor-211".to_owned(),
                "https://rc-innovations.es/shop/category/electronica-variadores-escs-541".to_owned(),
                "https://rc-innovations.es/shop/category/electronica-bec-sbec-ubec-476".to_owned(),
                "https://rc-innovations.es/en/shop/category/electronics-servos-245".to_owned(),
                "https://rc-innovations.es/shop/category/electronica-emisoras-y-receptores-fr-sky-239".to_owned(),
                "https://rc-innovations.es/shop/category/accesorios-helices-aero-naut-433".to_owned(),
                "https://rc-innovations.es/shop/category/accesorios-fibra-carbono-vidrio-134".to_owned(),
                "https://rc-innovations.es/shop/category/multicopteros-accesorios-trenes-de-aterrizaje-543".to_owned(),
            ],
            max_category_pages: 60,
            max_product_pages: 250,
            product_path_fragments: vec!["/shop/".to_owned()],
            category_path_fragments: vec!["/shop/category/".to_owned()],
            relevance_terms: vec![
                "bateria".to_owned(),
                "battery".to_owned(),
                "lipo".to_owned(),
                "motor".to_owned(),
                "tmotor".to_owned(),
                "variador".to_owned(),
                "esc".to_owned(),
                "bec".to_owned(),
                "servo".to_owned(),
                "helice".to_owned(),
                "prop".to_owned(),
                "receptor".to_owned(),
                "receiver".to_owned(),
                "telemetr".to_owned(),
                "gps".to_owned(),
                "sensor".to_owned(),
                "fibra-carbono".to_owned(),
                "carbono".to_owned(),
                "carbon-".to_owned(),
                "tren-aterrizaje".to_owned(),
                "landing".to_owned(),
                "patin".to_owned(),
                "autopilot".to_owned(),
                "flight-controller".to_owned(),
            ],
        }
    }
}

impl CrawlPolicy {
    /// Reject a policy that could crawl outside its declared scope.
    pub fn validate(&self) -> Result<(), CollectorError> {
        if self.allowed_hosts.is_empty()
            || self.allowed_hosts.iter().any(|host| {
                host.is_empty() || host.contains('/') || host.bytes().any(|byte| !byte.is_ascii())
            })
        {
            return Err(CollectorError::new("crawl host allow-list is invalid"));
        }
        if self.user_agent.trim().is_empty() {
            return Err(CollectorError::new("crawl user-agent is empty"));
        }
        if self.max_product_pages == 0 {
            return Err(CollectorError::new("crawl page cap must be positive"));
        }
        if self.source_id.trim().is_empty() || self.publisher.trim().is_empty() {
            return Err(CollectorError::new(
                "crawl source id and publisher must be non-empty",
            ));
        }
        if self.product_path_fragments.is_empty() {
            return Err(CollectorError::new(
                "crawl product path allow-list must be non-empty",
            ));
        }
        if !self.seed_category_urls.is_empty() && self.max_category_pages == 0 {
            return Err(CollectorError::new(
                "category page cap must be positive when category seeds are configured",
            ));
        }
        ensure_allowed_url(&self.robots_url, self)?;
        ensure_allowed_url(&self.sitemap_url, self)?;
        for category_url in &self.seed_category_urls {
            ensure_allowed_url(category_url, self)?;
            if !is_category_url(category_url, self) {
                return Err(CollectorError::new(format!(
                    "category seed is not a category URL: '{category_url}'"
                )));
            }
        }
        Ok(())
    }
}
