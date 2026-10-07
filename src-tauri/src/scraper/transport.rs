//! Source-owned transport settings. The HTTP client receives only resolved policies.
use super::catalog::{SourceDefinition, SourceRoute};
use serde::Deserialize;
use std::time::Duration;

const DEFAULT_MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportConfig {
    #[serde(default)]
    pub list: PolicyOverrides,
    #[serde(default)]
    pub detail: Vec<DetailPolicy>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DetailPolicy {
    pub host: String,
    #[serde(flatten)]
    pub settings: PolicyOverrides,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyOverrides {
    pub timeout_seconds: Option<u64>,
    pub retry_attempts: Option<usize>,
    pub host_concurrency: Option<usize>,
    pub cache: Option<bool>,
    pub max_response_bytes: Option<usize>,
    pub tls_fallback_host: Option<String>,
    pub browser_fallback: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportPolicy {
    pub timeout: Duration,
    pub retry_attempts: usize,
    pub host_concurrency: usize,
    pub cache: bool,
    pub max_response_bytes: usize,
    pub tls_fallback_host: Option<String>,
    pub browser_fallback: bool,
}

impl TransportPolicy {
    pub fn list_default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            retry_attempts: 3,
            host_concurrency: 2,
            cache: true,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            tls_fallback_host: None,
            browser_fallback: false,
        }
    }

    pub fn detail_default() -> Self {
        Self {
            cache: false,
            ..Self::list_default()
        }
    }

    fn overlay(mut self, settings: &PolicyOverrides) -> Self {
        if let Some(value) = settings.timeout_seconds {
            self.timeout = Duration::from_secs(value);
        }
        if let Some(value) = settings.retry_attempts {
            self.retry_attempts = value;
        }
        if let Some(value) = settings.host_concurrency {
            self.host_concurrency = value;
        }
        if let Some(value) = settings.cache {
            self.cache = value;
        }
        if let Some(value) = settings.max_response_bytes {
            self.max_response_bytes = value;
        }
        if let Some(value) = &settings.tls_fallback_host {
            self.tls_fallback_host = Some(value.to_ascii_lowercase());
        }
        if let Some(value) = settings.browser_fallback {
            self.browser_fallback = value;
        }
        self
    }

    pub fn for_route(source: &SourceDefinition, route: &SourceRoute) -> Self {
        let policy = Self::list_default();
        let policy = source
            .transport
            .as_ref()
            .map_or(policy.clone(), |config| policy.overlay(&config.list));
        route
            .transport
            .as_ref()
            .map_or(policy.clone(), |settings| policy.overlay(settings))
    }

    pub fn for_detail(source: &SourceDefinition, url: &str) -> Self {
        let host = url::Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase));
        source
            .transport
            .as_ref()
            .and_then(|config| {
                config
                    .detail
                    .iter()
                    .find(|item| Some(item.host.as_str()) == host.as_deref())
            })
            .map_or_else(Self::detail_default, |item| {
                Self::detail_default().overlay(&item.settings)
            })
    }

    pub fn allows_tls_fallback_for(&self, url: &str) -> bool {
        let Some(allowed) = &self.tls_fallback_host else {
            return false;
        };
        url::Url::parse(url)
            .ok()
            .is_some_and(|parsed| parsed.scheme() == "https" && parsed.host_str() == Some(allowed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scraper::catalog::{find_source, routes_for};

    #[test]
    fn catalog_policies_preserve_current_fast_fail_contract() {
        let wda = find_source("勞動力發展署").unwrap();
        let policy = TransportPolicy::for_route(wda, &routes_for(wda)[0]);
        assert_eq!(policy.timeout, Duration::from_secs(25));
        assert_eq!(policy.retry_attempts, 1);

        let nics = find_source("國家資通安全研究院").unwrap();
        let routes = routes_for(nics);
        assert_eq!(
            TransportPolicy::for_route(nics, &routes[0]).timeout,
            Duration::from_secs(8)
        );
        assert_eq!(
            TransportPolicy::for_route(nics, &routes[0]).retry_attempts,
            1
        );
        assert_eq!(
            TransportPolicy::for_route(nics, &routes[1]).timeout,
            Duration::from_secs(60)
        );

        for (source, host) in [
            ("法務部", "www.moj.gov.tw"),
            ("國家公園署", "www.nps.gov.tw"),
            ("農業部", "www.moa.gov.tw"),
        ] {
            let source = find_source(source).unwrap();
            let detail = TransportPolicy::for_detail(source, &format!("https://{host}/news"));
            assert_eq!(detail.timeout, Duration::from_secs(8));
            assert_eq!(detail.retry_attempts, 1);
            assert!(!detail.cache);
        }
    }

    #[test]
    fn tls_exception_is_exact_host_and_https_only() {
        let mnd = find_source("國防部").unwrap();
        let route = routes_for(mnd).remove(0);
        let policy = TransportPolicy::for_route(mnd, &route);
        assert!(policy.allows_tls_fallback_for(&route.url));
        assert!(!policy.allows_tls_fallback_for("https://mnd.gov.tw/news"));
        assert!(!policy.allows_tls_fallback_for("http://www.mnd.gov.tw/news"));
        assert!(!policy.allows_tls_fallback_for("https://example.test/news"));
        assert!(!TransportPolicy::list_default().allows_tls_fallback_for(&route.url));
    }

    #[test]
    fn unstable_sources_have_bounded_http_and_official_browser_fallbacks() {
        for (name, seconds) in [("國防部", 12), ("客委會", 8), ("公路局", 8)] {
            let source = find_source(name).unwrap();
            let routes = routes_for(source);
            let policy = TransportPolicy::for_route(source, &routes[0]);
            assert_eq!(policy.timeout, Duration::from_secs(seconds));
            assert_eq!(policy.retry_attempts, 1);
            assert_eq!(routes.last().unwrap().kind, "browser");
            assert!(routes.iter().all(|route| route.official));
            if name != "國防部" {
                assert!(routes
                    .iter()
                    .all(|route| TransportPolicy::for_route(source, route)
                        .tls_fallback_host
                        .is_none()));
            }
        }
        let hakka = find_source("客委會").unwrap();
        let detail = TransportPolicy::for_detail(
            hakka,
            "https://www.hakka.gov.tw/chhakka/app/data/view?id=25",
        );
        assert!(detail.browser_fallback);
        assert!(detail.tls_fallback_host.is_none());
        assert!(!TransportPolicy::for_detail(hakka, "https://other.test/news").browser_fallback);
    }

    #[test]
    fn embedded_tls_policy_is_only_on_declared_defense_routes() {
        for source in crate::scraper::catalog::all_sources() {
            for route in routes_for(source) {
                let policy = TransportPolicy::for_route(source, &route);
                if policy.tls_fallback_host.is_some() {
                    assert_eq!(source.name, "國防部");
                    assert!(matches!(
                        route.id.as_str(),
                        "official-html" | "official-browser-tls-fallback"
                    ));
                    assert!(policy.allows_tls_fallback_for(&route.url));
                }
            }
        }
    }
}
