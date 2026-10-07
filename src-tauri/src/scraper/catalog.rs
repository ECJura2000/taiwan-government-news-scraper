use super::transport::{PolicyOverrides, TransportConfig};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDefinition {
    pub name: String,
    pub urls: Vec<String>,
    #[serde(default)]
    pub parent_ministry: Option<String>,
    #[serde(default)]
    pub aggregate_routes: bool,
    #[serde(default)]
    pub routes: Vec<SourceRoute>,
    #[serde(default)]
    pub transport: Option<TransportConfig>,
}

#[derive(Debug, Clone, Deserialize)]
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteSelectors {
    pub item: String,
    pub link: String,
    pub title: String,
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
                                    | "timeout_seconds"
                                    | "retry_attempts"
                                    | "host_concurrency"
                                    | "cache"
                                    | "max_response_bytes"
                                    | "tls_fallback_host"
                                    | "browser_fallback"
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
        for value in &source.urls {
            validate_url(value).map_err(|e| format!("來源 {} urls：{e}", source.name))?;
        }
        if let Some(config) = &source.transport {
            validate_settings(&config.list)
                .map_err(|e| format!("來源 {} transport.list：{e}", source.name))?;
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
            if !matches!(route.kind.as_str(), "html" | "rss" | "json" | "browser") {
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
                    | "afna-html"
                    | "fa-html"
                    | "sfaa-html"
                    | "mohw-source-filter"
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
                validate_settings(settings).map_err(|e| format!("{context} transport：{e}"))?;
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
    Ok(sources)
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
    all_sources().iter().find(|source| source.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(all_sources().len(), 88);
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
        let registered: Vec<_> = all_sources()
            .iter()
            .filter(|source| source.parent_ministry.is_some())
            .collect();
        assert_eq!(registered.len(), approved.len());
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
        assert_eq!(fishery[0].id, "primary-html");
        assert_eq!(fishery[1].kind, "rss");

        let environment = routes_for(find_source("環境部").unwrap());
        assert!(environment.iter().any(|route| route.kind == "browser"));

        let culture = routes_for(find_source("文化部").unwrap());
        assert_eq!(culture[0].kind, "html");
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
