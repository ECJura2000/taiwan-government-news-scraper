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
    pub post_json: Option<serde_json::Value>,
    pub accept_compression: Option<bool>,
    pub tls_intermediate: Option<TlsIntermediatePolicy>,
    pub timeout_seconds: Option<u64>,
    pub retry_attempts: Option<usize>,
    pub host_concurrency: Option<usize>,
    pub cache: Option<bool>,
    pub max_response_bytes: Option<usize>,
    pub tls_fallback_host: Option<String>,
    pub browser_fallback: Option<bool>,
    pub browser_wait_for_selector: Option<String>,
    pub browser_native_user_agent: Option<bool>,
    pub browser_entry_url: Option<String>,
}

/// A verified intermediate chain helper, never an invalid-certificate exception.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TlsIntermediatePolicy {
    pub host: String,
    pub certificate: TlsIntermediateCertificate,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TlsIntermediateCertificate {
    TwcaSecure2023,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportPolicy {
    pub post_json: Option<serde_json::Value>,
    pub accept_compression: bool,
    pub tls_intermediate: Option<TlsIntermediatePolicy>,
    pub timeout: Duration,
    /// Only an explicitly declared timeout bounds the whole browser list page.
    pub browser_page_timeout: Option<Duration>,
    pub retry_attempts: usize,
    pub host_concurrency: usize,
    pub cache: bool,
    pub max_response_bytes: usize,
    pub tls_fallback_host: Option<String>,
    pub browser_fallback: bool,
    pub browser_wait_for_selector: Option<String>,
    pub browser_native_user_agent: bool,
    pub browser_entry_url: Option<String>,
}

impl TransportPolicy {
    pub fn list_default() -> Self {
        Self {
            post_json: None,
            accept_compression: true,
            tls_intermediate: None,
            timeout: Duration::from_secs(60),
            browser_page_timeout: None,
            retry_attempts: 3,
            host_concurrency: 2,
            cache: true,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            tls_fallback_host: None,
            browser_fallback: false,
            browser_wait_for_selector: None,
            browser_native_user_agent: false,
            browser_entry_url: None,
        }
    }

    pub fn detail_default() -> Self {
        Self {
            cache: false,
            ..Self::list_default()
        }
    }

    fn overlay(mut self, settings: &PolicyOverrides) -> Self {
        if let Some(value) = &settings.post_json {
            self.post_json = Some(value.clone());
        }
        if let Some(value) = settings.accept_compression {
            self.accept_compression = value;
        }
        if let Some(value) = &settings.tls_intermediate {
            self.tls_intermediate = Some(value.clone());
        }
        if let Some(value) = settings.timeout_seconds {
            self.timeout = Duration::from_secs(value);
            self.browser_page_timeout = Some(self.timeout);
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
        if let Some(value) = &settings.browser_wait_for_selector {
            self.browser_wait_for_selector = Some(value.clone());
        }
        if let Some(value) = settings.browser_native_user_agent {
            self.browser_native_user_agent = value;
        }
        if let Some(value) = &settings.browser_entry_url {
            self.browser_entry_url = Some(value.clone());
        }
        self
    }

    pub fn browser_wait_script(&self) -> Option<String> {
        self.browser_wait_for_selector.as_ref().map(|selector| {
            format!(
                "(async()=>{{for(let i=0;i<30;i++){{if(document.querySelector({}))return;await new Promise(r=>setTimeout(r,1000));}}throw new Error('official article selector not ready');}})()",
                serde_json::to_string(selector).expect("selector JSON")
            )
        })
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

    pub fn intermediate_certificate_for(&self, url: &str) -> Option<TlsIntermediateCertificate> {
        let declared = self.tls_intermediate.as_ref()?;
        // The pinned profile was explicitly approved for these hosts only.
        if !matches!(
            declared.host.as_str(),
            "www.nfa.gov.tw" | "www.atp.gov.tw" | "www.hpa.gov.tw"
        ) {
            return None;
        }
        let parsed = url::Url::parse(url).ok()?;
        (parsed.scheme() == "https"
            && parsed.host_str() == Some(declared.host.as_str())
            && parsed.username().is_empty()
            && parsed.password().is_none())
        .then_some(declared.certificate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scraper::catalog::{find_source, routes_for};

    #[test]
    fn browser_page_deadline_requires_a_declaration_and_honors_route_override() {
        let mut source = SourceDefinition::default();
        let mut route = SourceRoute::default();
        let default = TransportPolicy::for_route(&source, &route);
        assert_eq!(default.timeout, Duration::from_secs(60));
        assert_eq!(default.browser_page_timeout, None);
        source.transport = Some(TransportConfig {
            list: PolicyOverrides {
                timeout_seconds: Some(12),
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(
            TransportPolicy::for_route(&source, &route).browser_page_timeout,
            Some(Duration::from_secs(12))
        );
        route.transport = Some(PolicyOverrides {
            timeout_seconds: Some(30),
            ..Default::default()
        });
        assert_eq!(
            TransportPolicy::for_route(&source, &route).browser_page_timeout,
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn identity_encoding_is_scoped_to_wdas_official_rss_only() {
        let source = find_source("勞動力發展署").unwrap();
        for route in routes_for(source) {
            assert_eq!(
                TransportPolicy::for_route(source, &route).accept_compression,
                route.kind != "rss"
            );
        }
        assert!(
            TransportPolicy::for_detail(
                source,
                "https://www.wda.gov.tw/News_Content.aspx?n=31&s=1"
            )
            .accept_compression
        );
        assert!(TransportPolicy::list_default().accept_compression);
    }

    #[test]
    fn intermediate_profile_is_limited_to_the_three_approved_https_hosts() {
        let mut declared = Vec::new();
        for source in crate::scraper::catalog::all_sources() {
            for route in routes_for(source) {
                let policy = TransportPolicy::for_route(source, &route);
                if let Some(profile) = &policy.tls_intermediate {
                    declared.push(source.name.as_str());
                    assert!(policy.intermediate_certificate_for(&route.url).is_some());
                    assert!(policy
                        .intermediate_certificate_for("https://example.com/")
                        .is_none());
                    assert!(policy
                        .intermediate_certificate_for(&format!("http://{}/", profile.host))
                        .is_none());
                    assert!(policy
                        .intermediate_certificate_for(&format!(
                            "https://user:password@{}/",
                            profile.host
                        ))
                        .is_none());
                    assert!(!policy.allows_tls_fallback_for(&route.url));
                    let mut undeclared = policy.clone();
                    undeclared.tls_intermediate.as_mut().unwrap().host = "example.com".into();
                    assert!(undeclared
                        .intermediate_certificate_for("https://example.com/")
                        .is_none());
                }
            }
        }
        declared.sort();
        declared.dedup();
        assert_eq!(declared, vec!["國健署", "消防署", "農科園區"]);
    }

    #[test]
    fn homepage_entry_is_scoped_to_declared_browser_route() {
        let source = find_source("公路局").unwrap();
        for route in routes_for(source) {
            let policy = TransportPolicy::for_route(source, &route);
            assert_eq!(
                policy.browser_entry_url.as_deref(),
                if route.id == "official-browser" {
                    Some("https://www.thb.gov.tw/")
                } else {
                    None
                }
            );
            assert!(!policy.allows_tls_fallback_for(&route.url));
        }
        assert!(
            TransportPolicy::for_detail(source, "https://www.thb.gov.tw/article")
                .browser_entry_url
                .is_none()
        );
    }

    #[test]
    fn native_browser_identification_is_declared_and_scoped() {
        let source = find_source("農業部種苗改良繁殖場").unwrap();
        let route = routes_for(source)
            .into_iter()
            .find(|r| r.kind == "browser")
            .unwrap();
        assert!(TransportPolicy::for_route(source, &route).browser_native_user_agent);
        assert!(
            !TransportPolicy::for_detail(source, "https://www.tss.gov.tw/article")
                .browser_native_user_agent
        );
        let legacy = find_source("經濟部").unwrap();
        for route in routes_for(legacy) {
            assert!(!TransportPolicy::for_route(legacy, &route).browser_native_user_agent);
        }
    }

    #[test]
    fn detail_selector_wait_is_host_scoped_and_keeps_certificate_checks() {
        let source: SourceDefinition = serde_json::from_value(serde_json::json!({
            "name":"Dynamic official source", "urls":["https://official.test/news"],
            "transport":{"detail":[{"host":"official.test","browser_fallback":true,
                "browser_wait_for_selector":"article[data-ready=\"yes\"]"}]}
        }))
        .unwrap();
        let policy = TransportPolicy::for_detail(&source, "https://official.test/#/article/123");
        assert!(policy.browser_fallback);
        assert_eq!(
            policy.browser_wait_for_selector.as_deref(),
            Some("article[data-ready=\"yes\"]")
        );
        let script = policy.browser_wait_script().unwrap();
        assert!(script.contains("i<30"));
        assert!(script.contains("document.querySelector(\"article[data-ready=\\\"yes\\\"]\")"));
        assert!(script.contains("throw new Error"));
        assert!(!policy.allows_tls_fallback_for("https://official.test/article/123"));
        let other = TransportPolicy::for_detail(&source, "https://sub.official.test/article/123");
        assert!(!other.browser_fallback);
        assert!(other.browser_wait_script().is_none());
        assert!(other.tls_fallback_host.is_none());
    }

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
