use super::transport::{PolicyOverrides, TransportConfig};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SourceDefinition {
    pub name: String,
    pub urls: Vec<String>,
    #[serde(default)]
    pub parent_ministry: Option<String>,
    #[serde(default)]
    pub official_name: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub parent_source: Option<String>,
    #[serde(default)]
    pub filters: ItemFilters,
    #[serde(default)]
    pub aggregate_routes: bool,
    #[serde(default)]
    pub routes: Vec<SourceRoute>,
    #[serde(default)]
    pub transport: Option<TransportConfig>,
    #[serde(default)]
    pub detail_quality: Option<DetailQualityPolicy>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetailQualityPolicy {
    pub expectation: DetailExpectation,
    pub min_items: usize,
    pub min_coverage_percent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailExpectation {
    FullText,
    SummaryOnly,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SourceRoute {
    pub id: String,
    pub url: String,
    #[serde(default = "default_route_kind")]
    pub kind: String,
    #[serde(default = "default_route_parser")]
    pub parser: String,
    #[serde(default = "default_route_priority")]
    pub priority: u32,
    #[serde(default = "default_true")]
    pub official: bool,
    #[serde(default)]
    pub coverage_reduced: bool,
    #[serde(default)]
    pub selectors: Option<RouteSelectors>,
    #[serde(default)]
    pub transport: Option<PolicyOverrides>,
    #[serde(default)]
    pub filters: ItemFilters,
    #[serde(default)]
    pub pagination: Option<Pagination>,
    #[serde(default)]
    pub wait_for_selector: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemFilters {
    #[serde(default = "default_true")]
    pub exclude_administrative: bool,
    #[serde(default)]
    pub exclude_categories: Vec<String>,
    #[serde(default)]
    pub exclude_title_prefixes: Vec<String>,
    #[serde(default)]
    pub exclude_title_patterns: Vec<String>,
    #[serde(default)]
    pub publisher_allowlist: Vec<String>,
    #[serde(default)]
    pub publisher_prefixes: Vec<String>,
    #[serde(default)]
    pub include_categories: Vec<String>,
}

impl Default for ItemFilters {
    fn default() -> Self {
        Self {
            exclude_administrative: true,
            exclude_categories: Vec::new(),
            exclude_title_prefixes: Vec::new(),
            exclude_title_patterns: Vec::new(),
            publisher_allowlist: Vec::new(),
            publisher_prefixes: Vec::new(),
            include_categories: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pagination {
    pub parameter: String,
    #[serde(default = "default_page")]
    pub first_page: u32,
    #[serde(default = "default_max_pages")]
    pub max_pages: u32,
}
const fn default_page() -> u32 {
    1
}
const fn default_max_pages() -> u32 {
    20
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RouteSelectors {
    pub item: String,
    pub link: String,
    pub title: String,
    #[serde(default)]
    pub title_attribute: Option<String>,
    pub date: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub exclude_title_prefixes: Vec<String>,
    #[serde(default)]
    pub category_label: Option<String>,
    #[serde(default)]
    pub strip_leading_date: bool,
}

fn default_route_kind() -> String {
    "html".into()
}

fn default_route_parser() -> String {
    "primary".into()
}

const fn default_route_priority() -> u32 {
    1
}

const fn default_true() -> bool {
    true
}

pub fn routes_for(source: &SourceDefinition) -> Vec<SourceRoute> {
    if !source.routes.is_empty() {
        let mut routes = source.routes.clone();
        routes.sort_by_key(|route| route.priority);
        return routes;
    }
    source
        .urls
        .iter()
        .enumerate()
        .map(|(index, url)| SourceRoute {
            id: if index == 0 {
                "primary".into()
            } else {
                format!("alternate-{index}")
            },
            url: url.clone(),
            kind: if url.to_ascii_lowercase().contains("rss")
                || url.to_ascii_lowercase().contains("feed")
                || url.to_ascii_lowercase().ends_with(".xml")
            {
                "rss".into()
            } else {
                "html".into()
            },
            parser: "standard".into(),
            priority: index as u32 + 1,
            official: true,
            coverage_reduced: false,
            selectors: None,
            transport: None,
            filters: ItemFilters::default(),
            pagination: None,
            wait_for_selector: None,
        })
        .collect()
}

pub fn all_sources() -> &'static [SourceDefinition] {
    use std::sync::OnceLock;
    static CATALOG: OnceLock<Vec<SourceDefinition>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            parse_catalog(include_str!("../../resources/sources.json"))
                .expect("embedded source catalog validation failed")
        })
        .as_slice()
}

pub fn parse_catalog(json: &str) -> Result<Vec<SourceDefinition>, String> {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| format!("來源目錄 JSON：{e}"))?;
    if rows.is_empty() {
        return Err("來源目錄不得空白".into());
    }
    let sources: Vec<SourceDefinition> = rows
        .into_iter()
        .map(|row| {
            let name = row["name"].as_str().unwrap_or("未命名").to_owned();
            // Serde cannot deny unknown fields on a flattened struct. Check the
            // detail policy keys here before deserializing the flattened settings.
            if let Some(details) = row["transport"]["detail"].as_array() {
                for detail in details {
                    if let Some(fields) = detail.as_object() {
                        for field in fields.keys() {
                            if !matches!(
                                field.as_str(),
                                "host"
                                    | "accept_compression"
                                    | "timeout_seconds"
                                    | "retry_attempts"
                                    | "host_concurrency"
                                    | "cache"
                                    | "max_response_bytes"
                                    | "tls_fallback_host"
                                    | "tls_intermediate"
                                    | "browser_fallback"
                                    | "browser_wait_for_selector"
                                    | "browser_native_user_agent"
                                    | "browser_entry_url"
                            ) {
                                return Err(format!(
                                    "來源 {name} transport.detail：未知欄位 {field}"
                                ));
                            }
                        }
                    }
                }
            }
            serde_json::from_value(row).map_err(|e| format!("來源 {name}：{e}"))
        })
        .collect::<Result<_, _>>()?;
    let mut names = std::collections::HashSet::new();
    for source in &sources {
        if source.name.trim().is_empty() || !names.insert(&source.name) {
            return Err(format!("來源 {}：name 空白或重複", source.name));
        }
        if source.urls.is_empty() {
            return Err(format!("來源 {}：urls 不得空白", source.name));
        }
        validate_filters(&source.filters)
            .map_err(|error| format!("來源 {} filters：{error}", source.name))?;
        if source.parent_source.as_deref() == Some(source.name.as_str())
            || source.parent_ministry.as_deref() == Some(source.name.as_str())
        {
            return Err(format!("來源 {}：不得以自己作為上級", source.name));
        }
        for value in &source.urls {
            validate_url(value).map_err(|e| format!("來源 {} urls：{e}", source.name))?;
        }
        if let Some(policy) = &source.detail_quality {
            if policy.min_items == 0
                || policy.min_coverage_percent == 0
                || policy.min_coverage_percent > 100
            {
                return Err(format!(
                    "來源 {} detail_quality：min_items 須大於零，min_coverage_percent 須為 1–100",
                    source.name
                ));
            }
        }
        if let Some(config) = &source.transport {
            validate_settings(&config.list)
                .map_err(|e| format!("來源 {} transport.list：{e}", source.name))?;
            if config.list.post_json.is_some() {
                return Err(format!("來源 {} post_json 僅限 JSON route", source.name));
            }
            if config.list.tls_fallback_host.is_some() {
                return Err(format!(
                    "來源 {}：TLS 例外只能指定於國防部 route",
                    source.name
                ));
            }
            let mut hosts = std::collections::HashSet::new();
            for detail in &config.detail {
                let url = url::Url::parse(&format!("https://{}/", detail.host))
                    .map_err(|e| e.to_string())?;
                if url.host_str() != Some(detail.host.as_str())
                    || url.path() != "/"
                    || url.port().is_some()
                    || !url.username().is_empty()
                    || !hosts.insert(&detail.host)
                {
                    return Err(format!(
                        "來源 {} transport.detail.host：{} 必須為不重複的完整小寫主機名稱",
                        source.name, detail.host
                    ));
                }
                validate_settings(&detail.settings)
                    .map_err(|e| format!("來源 {} detail {}：{e}", source.name, detail.host))?;
                if detail
                    .settings
                    .tls_intermediate
                    .as_ref()
                    .is_some_and(|profile| profile.host != detail.host)
                {
                    return Err(format!(
                        "來源 {} detail：中繼政策主機必須與詳細頁主機一致",
                        source.name
                    ));
                }
                if detail.settings.tls_fallback_host.is_some() {
                    return Err(format!("來源 {} detail：不得設定 TLS 例外", source.name));
                }
            }
        }
        let mut ids = std::collections::HashSet::new();
        for route in routes_for(source) {
            let context = format!("來源 {} route {}", source.name, route.id);
            if route.id.trim().is_empty() || !ids.insert(route.id.clone()) || route.priority == 0 {
                return Err(format!("{context}：id 空白／重複或 priority 為零"));
            }
            let url = validate_url(&route.url).map_err(|e| format!("{context} url：{e}"))?;
            validate_filters(&route.filters)
                .map_err(|error| format!("{context} filters：{error}"))?;
            if let Some(pagination) = &route.pagination {
                if pagination.parameter.trim().is_empty()
                    || pagination.first_page == 0
                    || pagination.max_pages == 0
                    || pagination.max_pages > 100
                {
                    return Err(format!("{context} pagination：須提供參數及1–100頁上限"));
                }
            }
            if let Some(selector) = &route.wait_for_selector {
                scraper::Selector::parse(selector)
                    .map_err(|error| format!("{context} wait_for_selector：{error:?}"))?;
            }
            if !matches!(
                route.kind.as_str(),
                "html" | "rss" | "json" | "xml" | "browser"
            ) {
                return Err(format!("{context} kind：不支援 {}", route.kind));
            }
            if !matches!(
                route.parser.as_str(),
                "primary"
                    | "standard"
                    | "ey-html"
                    | "standard-rss"
                    | "nlma-json"
                    | "nps-json"
                    | "mnd-browser-tls-fallback"
                    | "taicca-html"
                    | "sports-html"
                    | "mjac-html"
                    | "moea-rss-full-text"
                    | "moea-html"
                    | "thb-html"
                    | "thb-json"
                    | "thb-xml"
                    | "afna-html"
                    | "fa-html"
                    | "sfaa-html"
                    | "mohw-source-filter"
                    | "moenv-open-data-json"
                    | "moenv-html"
                    | "moenv-news-portal-browser"
                    | "hakka-html"
                    | "vghtpe-html"
                    | "pcc-html"
                    | "228-announcements"
            ) {
                return Err(format!("{context} parser：不支援 {}", route.parser));
            }
            if let Some(settings) = &route.transport {
                if settings.post_json.is_some()
                    && (route.kind != "json" || settings.tls_fallback_host.is_some())
                {
                    return Err(format!("{context} post_json 僅限無 TLS 例外的 JSON route"));
                }
                validate_settings(settings).map_err(|e| format!("{context} transport：{e}"))?;
                if let Some(entry) = &settings.browser_entry_url {
                    if validate_url(entry)?.origin() != url.origin() {
                        return Err(format!("{context} browser_entry_url 必須與 route 同來源"));
                    }
                }
                if let Some(host) = &settings.tls_fallback_host {
                    if source.name != "國防部"
                        || host != "www.mnd.gov.tw"
                        || url.scheme() != "https"
                        || url.host_str() != Some(host)
                        || !matches!(
                            route.id.as_str(),
                            "official-html" | "official-browser-tls-fallback"
                        )
                    {
                        return Err(format!(
                            "{context} tls_fallback_host：只允許已宣告國防部官方 HTTPS route"
                        ));
                    }
                }
            }
            if let Some(selectors) = &route.selectors {
                for (field, value) in [
                    ("item", Some(selectors.item.as_str())),
                    ("link", Some(selectors.link.as_str())),
                    ("title", Some(selectors.title.as_str())),
                    ("date", Some(selectors.date.as_str())),
                    ("summary", selectors.summary.as_deref()),
                    ("department", selectors.department.as_deref()),
                    ("category", selectors.category.as_deref()),
                ] {
                    if let Some(value) = value {
                        scraper::Selector::parse(value)
                            .map_err(|e| format!("{context} selectors.{field}：{e:?}"))?;
                    }
                }
            }
        }
    }
    let mut identifiers = std::collections::HashMap::new();
    for source in &sources {
        for identifier in std::iter::once(&source.name)
            .chain(source.official_name.iter())
            .chain(source.aliases.iter())
        {
            if identifier.trim().is_empty() {
                return Err(format!("來源 {}：別名不得空白", source.name));
            }
            if let Some(owner) = identifiers.insert(identifier, &source.name) {
                if owner != &source.name {
                    return Err(format!(
                        "來源 {}：別名 {identifier} 與 {owner} 衝突",
                        source.name
                    ));
                }
            }
        }
    }
    for source in &sources {
        let mut seen = std::collections::HashSet::new();
        let mut node = source;
        while let Some(parent) = node.parent_source.as_deref().and_then(|name| {
            sources.iter().find(|source| {
                source.name == name
                    || source.official_name.as_deref() == Some(name)
                    || source.aliases.iter().any(|alias| alias == name)
            })
        }) {
            if !seen.insert(&node.name) {
                return Err(format!("來源 {}：上級關係循環", source.name));
            }
            node = parent;
        }
    }
    Ok(sources)
}

fn validate_filters(filters: &ItemFilters) -> Result<(), String> {
    for value in filters
        .exclude_categories
        .iter()
        .chain(&filters.exclude_title_prefixes)
        .chain(&filters.exclude_title_patterns)
        .chain(&filters.publisher_allowlist)
        .chain(&filters.publisher_prefixes)
        .chain(&filters.include_categories)
    {
        if value.trim().is_empty() {
            return Err("條件不得為空字串".into());
        }
    }
    for pattern in &filters.exclude_title_patterns {
        regex::Regex::new(pattern).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn validate_url(value: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(value).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(format!("{value} 必須為無帳密的 HTTP(S) URL"));
    }
    Ok(url)
}

fn validate_settings(settings: &PolicyOverrides) -> Result<(), String> {
    if let Some(body) = &settings.post_json {
        if !body.is_object()
            || body.as_object().is_none_or(|value| value.is_empty())
            || body.to_string().len() > 16 * 1024
        {
            return Err("post_json 必須為非空且不超過 16 KiB 的 JSON object".into());
        }
    }
    if let Some(profile) = &settings.tls_intermediate {
        if !matches!(
            profile.host.as_str(),
            "www.nfa.gov.tw" | "www.atp.gov.tw" | "www.hpa.gov.tw"
        ) || settings.tls_fallback_host.is_some()
        {
            return Err("tls_intermediate 僅限已核准的三個主機，且不得與 TLS 例外合用".into());
        }
    }
    if let Some(entry) = &settings.browser_entry_url {
        validate_url(entry).map_err(|error| format!("browser_entry_url：{error}"))?;
    }
    if let Some(selector) = &settings.browser_wait_for_selector {
        if selector.trim().is_empty() {
            return Err("browser_wait_for_selector 不得空白".into());
        }
        scraper::Selector::parse(selector)
            .map_err(|error| format!("browser_wait_for_selector：{error:?}"))?;
    }
    for (field, value, max) in [
        (
            "timeout_seconds",
            settings.timeout_seconds.map(|n| n as u128),
            3600,
        ),
        (
            "retry_attempts",
            settings.retry_attempts.map(|n| n as u128),
            10,
        ),
        (
            "host_concurrency",
            settings.host_concurrency.map(|n| n as u128),
            64,
        ),
        (
            "max_response_bytes",
            settings.max_response_bytes.map(|n| n as u128),
            128 * 1024 * 1024,
        ),
    ] {
        if value.is_some_and(|n| n == 0 || n > max) {
            return Err(format!("{field} 必須介於 1 與 {max}"));
        }
    }
    Ok(())
}

pub fn find_source(name: &str) -> Option<&'static SourceDefinition> {
    all_sources()
        .iter()
        .find(|source| matches_source_name(source, name))
}

fn matches_source_name(source: &SourceDefinition, name: &str) -> bool {
    source.name == name
        || source.official_name.as_deref() == Some(name)
        || source.aliases.iter().any(|alias| alias == name)
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_post_is_route_scoped_and_rejects_invalid_payloads() {
        let base = serde_json::json!({"name":"測試機關","urls":["https://official.test/query"],
            "routes":[{"id":"query","url":"https://official.test/query","kind":"json",
                "parser":"moenv-open-data-json","transport":{"post_json":{"offset":0}}}]});
        assert!(super::parse_catalog(&serde_json::json!([base.clone()]).to_string()).is_ok());
        for body in [
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::json!({"large":"x".repeat(16*1024)}),
        ] {
            let mut source = base.clone();
            source["routes"][0]["transport"]["post_json"] = body;
            assert!(
                super::parse_catalog(&serde_json::json!([source]).to_string())
                    .unwrap_err()
                    .contains("post_json")
            );
        }
        let mut source = base.clone();
        source["routes"][0]["kind"] = serde_json::json!("browser");
        assert!(
            super::parse_catalog(&serde_json::json!([source]).to_string())
                .unwrap_err()
                .contains("post_json")
        );
        let mut source = base;
        source["transport"] = serde_json::json!({"list":{"post_json":{"offset":0}}});
        assert!(
            super::parse_catalog(&serde_json::json!([source]).to_string())
                .unwrap_err()
                .contains("post_json")
        );
    }

    #[test]
    fn browser_entry_url_requires_same_origin_and_no_credentials() {
        let mut source = serde_json::json!({"name":"測試機關","urls":["https://official.test/news"],"routes":[{"id":"browser","url":"https://official.test/news","kind":"browser","parser":"standard","transport":{"browser_entry_url":"https://official.test/"}}]});
        assert!(super::parse_catalog(&serde_json::json!([source.clone()]).to_string()).is_ok());
        for entry in [
            "https://other.test/",
            "http://official.test/",
            "https://user:password@official.test/",
            "file:///tmp/page",
        ] {
            source["routes"][0]["transport"]["browser_entry_url"] = serde_json::json!(entry);
            assert!(
                super::parse_catalog(&serde_json::json!([source.clone()]).to_string()).is_err()
            );
        }
    }

    #[test]
    fn detail_browser_wait_requires_valid_css_without_tls_exemption() {
        let mut source = serde_json::json!({"name":"Official dynamic source",
            "urls":["https://official.test/news"],
            "transport":{"detail":[{"host":"official.test","browser_fallback":true,
                "browser_wait_for_selector":"article .article-content"}]}});
        assert!(super::parse_catalog(&serde_json::json!([source.clone()]).to_string()).is_ok());
        for selector in ["", "  ", "article["] {
            source["transport"]["detail"][0]["browser_wait_for_selector"] =
                serde_json::json!(selector);
            assert!(
                super::parse_catalog(&serde_json::json!([source.clone()]).to_string())
                    .unwrap_err()
                    .contains("browser_wait_for_selector")
            );
        }
        source["transport"]["detail"][0]["browser_wait_for_selector"] =
            serde_json::json!("article");
        source["transport"]["detail"][0]["tls_fallback_host"] = serde_json::json!("official.test");
        assert!(
            super::parse_catalog(&serde_json::json!([source]).to_string())
                .unwrap_err()
                .contains("TLS")
        );
    }
    use super::*;

    #[test]
    fn detail_quality_policy_is_validated_with_source_context() {
        for (expectation, min_items, percent) in [
            ("full_text", 0, 70),
            ("full_text", 5, 101),
            ("full_text", 5, 0),
            ("typo", 5, 70),
        ] {
            let value = serde_json::json!([{"name":"測試機關","urls":["https://example.test/news"],"detail_quality":{"expectation":expectation,"min_items":min_items,"min_coverage_percent":percent}}]);
            assert!(parse_catalog(&value.to_string())
                .unwrap_err()
                .contains("測試機關"));
        }
        let value = r#"[{"name":"測試機關","urls":["https://example.test/news"],"detail_quality":{"expectation":"summary_only","min_items":5,"min_coverage_percent":70}}]"#;
        assert_eq!(
            parse_catalog(value).unwrap()[0]
                .detail_quality
                .as_ref()
                .unwrap()
                .expectation,
            DetailExpectation::SummaryOnly
        );
    }

    #[test]
    fn rejects_bad_settings_with_source_and_field_context() {
        for field in [
            "timeout_seconds",
            "retry_attempts",
            "host_concurrency",
            "max_response_bytes",
        ] {
            for value in [0_u64, u64::MAX] {
                let document = serde_json::json!([{"name":"測試機關","urls":["https://example.test/news"],"transport":{"list":{field:value}}}]);
                let error = parse_catalog(&document.to_string()).unwrap_err();
                assert!(
                    error.contains("測試機關") && error.contains(field),
                    "{error}"
                );
            }
        }
        let typo = r#"[{"name":"測試機關","urls":["https://example.test"],"transport":{"list":{"timout_seconds":5}}}]"#;
        let error = parse_catalog(typo).unwrap_err();
        assert!(error.contains("測試機關") && error.contains("timout_seconds"));
        let detail_typo = r#"[{"name":"測試機關","urls":["https://example.test"],"transport":{"detail":[{"host":"example.test","timout_seconds":5}]}}]"#;
        assert!(parse_catalog(detail_typo)
            .unwrap_err()
            .contains("timout_seconds"));
        assert!(parse_catalog("[]").is_err());
    }

    #[test]
    fn rejects_invalid_routes_selectors_hosts_and_tls_exceptions() {
        let base = serde_json::json!({"name":"測試機關","urls":["https://example.test"],"routes":[{"id":"official-html","url":"https://example.test","kind":"html","parser":"standard","priority":1}]});
        for (field, value) in [
            ("kind", serde_json::json!("unknown")),
            ("parser", serde_json::json!("typo")),
            ("priority", serde_json::json!(0)),
            ("url", serde_json::json!("file:///tmp/news")),
        ] {
            let mut source = base.clone();
            source["routes"][0][field] = value;
            assert!(parse_catalog(&serde_json::json!([source]).to_string())
                .unwrap_err()
                .contains(field));
        }
        let mut source = base.clone();
        source["routes"][0]["transport"] =
            serde_json::json!({"tls_fallback_host":"www.mnd.gov.tw"});
        assert!(parse_catalog(&serde_json::json!([source]).to_string())
            .unwrap_err()
            .contains("tls_fallback_host"));
        let mut source = base.clone();
        source["transport"] = serde_json::json!({"detail":[{"host":"example.test/path"}]});
        assert!(parse_catalog(&serde_json::json!([source]).to_string()).is_err());
        let mut source = base.clone();
        source["routes"][0]["selectors"] =
            serde_json::json!({"item":"[","link":"a","title":"a","date":"time"});
        assert!(parse_catalog(&serde_json::json!([source]).to_string())
            .unwrap_err()
            .contains("selectors.item"));
        assert!(
            parse_catalog(&serde_json::json!([base.clone(), base]).to_string())
                .unwrap_err()
                .contains("重複")
        );
    }

    #[test]
    fn catalog_contains_all_registered_sources() {
        let approved: Vec<String> =
            serde_json::from_str(include_str!("../../resources/approved_sources.json"))
                .expect("approved source manifest must be valid JSON");
        let expected: std::collections::BTreeSet<_> = approved.iter().map(String::as_str).collect();
        let actual: std::collections::BTreeSet<_> = all_sources()
            .iter()
            .map(|source| source.name.as_str())
            .collect();
        assert!(!approved.is_empty());
        assert_eq!(
            expected.len(),
            approved.len(),
            "approved names must be unique"
        );
        assert_eq!(
            actual, expected,
            "catalog must contain the exact approved names"
        );
        assert_eq!(all_sources().len(), approved.len());
        assert!(find_source("行政院").is_some());
        assert!(find_source("中選會").is_some());
        assert!(find_source("文策院").is_some());
    }

    #[test]
    fn approved_foundations_have_matching_ministry_and_affiliation() {
        let approved = [
            ("國家文化藝術基金會", "文化部"),
            ("金門酒廠胡璉文化藝術基金會", "文化部"),
            ("文化臺灣基金會", "文化部"),
            ("臺灣美術基金會", "文化部"),
            ("臺灣博物館文教基金會", "文化部"),
            ("二二八事件紀念基金會", "內政部"),
            ("威權統治時期國家不法行為被害者權利回復基金會", "內政部"),
            ("台灣建築中心", "內政部"),
            ("藥害救濟基金會", "衛生福利部"),
            ("國家衛生研究院", "衛生福利部"),
            ("賑災基金會", "衛生福利部"),
            ("醫藥品查驗中心", "衛生福利部"),
            ("器官捐贈移植登錄及病人自主推廣中心", "衛生福利部"),
            ("婦女權益促進發展基金會", "衛生福利部"),
            ("醫院評鑑暨醫療品質策進會", "衛生福利部"),
        ];
        for (name, ministry) in approved {
            let source = find_source(name).expect("approved foundation must be registered");
            assert_eq!(source.parent_ministry.as_deref(), Some(ministry));
            assert_eq!(
                crate::scraper::quality::affiliated_path(name),
                &[ministry, name]
            );
        }
    }

    #[test]
    fn official_names_and_aliases_resolve_to_the_canonical_source() {
        let document = serde_json::json!([{
            "name":"國合會", "official_name":"財團法人國際合作發展基金會",
            "aliases":["國際合作發展基金會", "TaiwanICDF"],
            "urls":["https://www.icdf.org.tw/"]
        }]);
        let source = &parse_catalog(&document.to_string()).unwrap()[0];
        for name in [
            "國合會",
            "財團法人國際合作發展基金會",
            "國際合作發展基金會",
            "TaiwanICDF",
        ] {
            assert!(matches_source_name(source, name));
        }
        assert!(!matches_source_name(source, "unregistered"));
        for source in all_sources() {
            for identifier in std::iter::once(&source.name)
                .chain(source.official_name.iter())
                .chain(source.aliases.iter())
            {
                assert_eq!(find_source(identifier).unwrap().name, source.name);
            }
        }
    }

    #[test]
    fn identifier_collisions_across_sources_and_blank_identifiers_are_rejected() {
        let first = serde_json::json!({"name":"來源甲", "official_name":"正式甲",
            "aliases":["別名甲"], "urls":["https://example.test/a"]});
        let second = serde_json::json!({"name":"來源乙", "urls":["https://example.test/b"]});
        for (field, identifier) in [
            ("name", "別名甲"),
            ("official_name", "來源甲"),
            ("official_name", "正式甲"),
            ("aliases", "正式甲"),
            ("aliases", "來源甲"),
            ("aliases", "別名甲"),
        ] {
            let mut row = second.clone();
            row[field] = if field == "aliases" {
                serde_json::json!([identifier])
            } else {
                serde_json::json!(identifier)
            };
            let error =
                parse_catalog(&serde_json::json!([first.clone(), row]).to_string()).unwrap_err();
            assert!(error.contains("衝突"), "{error}");
        }
        for field in ["official_name", "aliases"] {
            let mut row = first.clone();
            row[field] = if field == "aliases" {
                serde_json::json!([" "])
            } else {
                serde_json::json!(" ")
            };
            assert!(parse_catalog(&serde_json::json!([row]).to_string())
                .unwrap_err()
                .contains("別名不得空白"));
        }
        let same_owner = serde_json::json!([{"name":"來源甲", "official_name":"來源甲",
            "aliases":["來源甲"], "urls":["https://example.test/"]}]);
        assert!(parse_catalog(&same_owner.to_string()).is_ok());
    }

    #[test]
    fn parent_cycles_resolving_official_names_and_aliases_are_rejected() {
        let first = serde_json::json!({"name":"來源甲", "official_name":"正式甲",
            "aliases":["別名甲"], "urls":["https://example.test/a"]});
        for self_parent in ["來源甲", "正式甲", "別名甲"] {
            let mut row = first.clone();
            row["parent_source"] = serde_json::json!(self_parent);
            assert!(parse_catalog(&serde_json::json!([row]).to_string()).is_err());
        }
        let mut child = first.clone();
        child["parent_source"] = serde_json::json!("正式乙");
        let second = serde_json::json!({"name":"來源乙", "official_name":"正式乙",
            "parent_source":"別名甲", "urls":["https://example.test/b"]});
        assert!(
            parse_catalog(&serde_json::json!([child, second]).to_string())
                .unwrap_err()
                .contains("上級關係循環")
        );
        let valid = serde_json::json!([first,
            {"name":"來源乙", "parent_source":"別名甲", "urls":["https://example.test/b"]}]);
        assert!(parse_catalog(&valid.to_string()).is_ok());
    }

    #[test]
    fn invalid_regex_and_empty_filter_conditions_have_source_and_route_context() {
        let base = serde_json::json!({"name":"測試機關", "urls":["https://example.test/"],
            "routes":[{"id":"news", "url":"https://example.test/", "parser":"standard"}]});
        for at_route in [false, true] {
            for filters in [
                serde_json::json!({"exclude_title_patterns":["["]}),
                serde_json::json!({"publisher_prefixes":[""]}),
            ] {
                let mut row = base.clone();
                if at_route {
                    row["routes"][0]["filters"] = filters;
                } else {
                    row["filters"] = filters;
                }
                let error = parse_catalog(&serde_json::json!([row]).to_string()).unwrap_err();
                assert!(
                    error.contains("測試機關") && error.contains("filters"),
                    "{error}"
                );
                if at_route {
                    assert!(error.contains("route news"), "{error}");
                }
            }
        }
        let mut row = base;
        row["filters"] = serde_json::json!({"exclude_title_patterns":["(?i)^徵才"],
            "publisher_prefixes":["外交部"]});
        assert!(parse_catalog(&serde_json::json!([row]).to_string()).is_ok());
    }

    #[test]
    fn every_source_has_an_official_url() {
        assert!(all_sources()
            .iter()
            .all(|source| !source.name.is_empty() && !source.urls.is_empty()));
    }

    #[test]
    fn every_source_has_effective_route_metadata() {
        assert!(all_sources().iter().all(|source| {
            let routes = routes_for(source);
            !routes.is_empty()
                && routes
                    .iter()
                    .all(|route| route.official && !route.id.is_empty() && !route.url.is_empty())
        }));
    }

    #[test]
    fn special_sources_keep_declared_fallback_and_browser_routes() {
        let fishery = routes_for(find_source("漁業署").unwrap());
        assert_eq!(fishery[0].kind, "rss");
        assert_eq!(fishery[1].id, "primary-html");

        let environment = routes_for(find_source("環境部").unwrap());
        assert!(environment.iter().any(|route| route.kind == "browser"));

        let culture = routes_for(find_source("文化部").unwrap());
        assert_eq!(culture[0].kind, "rss");
        assert_eq!(culture[1].kind, "browser");

        let nics = routes_for(find_source("國家資通安全研究院").unwrap());
        assert_eq!(nics[0].kind, "html");
        assert_eq!(nics[1].kind, "browser");

        let correction = routes_for(find_source("矯正署").unwrap());
        assert!(correction.iter().any(|route| route.coverage_reduced));

        let economy = routes_for(find_source("經濟部").unwrap());
        assert_eq!(economy[0].id, "official-rss-full-text");
        assert_eq!(economy[0].kind, "rss");
        assert_eq!(economy[0].parser, "moea-rss-full-text");
        assert!(economy[1].coverage_reduced);
    }

    #[test]
    fn finance_catalog_aggregates_all_official_rss_routes() {
        let finance = find_source("財政部").unwrap();
        assert!(finance.aggregate_routes);
        assert_eq!(routes_for(finance).len(), 2);
    }
}
