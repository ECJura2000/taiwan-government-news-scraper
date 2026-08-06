use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SourceDefinition {
    pub name: String,
    pub urls: Vec<String>,
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
}
