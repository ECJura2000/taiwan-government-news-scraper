use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SourceDefinition {
    pub name: String,
    pub urls: Vec<String>,
    #[serde(default)]
    pub aggregate_routes: bool,
    #[serde(default)]
    pub routes: Vec<SourceRoute>,
}

#[derive(Debug, Clone, Deserialize)]
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
        })
        .collect()
}

pub fn all_sources() -> &'static [SourceDefinition] {
    use std::sync::OnceLock;
    static CATALOG: OnceLock<Vec<SourceDefinition>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../resources/sources.json"))
                .expect("embedded source catalog must be valid JSON")
        })
        .as_slice()
}

pub fn find_source(name: &str) -> Option<&'static SourceDefinition> {
    all_sources().iter().find(|source| source.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_all_registered_sources() {
        assert_eq!(all_sources().len(), 72);
        assert!(find_source("行政院").is_some());
        assert!(find_source("中選會").is_some());
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

        let correction = routes_for(find_source("矯正署").unwrap());
        assert!(correction.iter().any(|route| route.coverage_reduced));
    }

    #[test]
    fn finance_catalog_aggregates_all_official_rss_routes() {
        let finance = find_source("財政部").unwrap();
        assert!(finance.aggregate_routes);
        assert_eq!(routes_for(finance).len(), 2);
    }
}
