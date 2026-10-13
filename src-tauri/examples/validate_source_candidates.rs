use std::{env, fs};
use taiwan_government_news_lib::scraper::{adapters, catalog::SourceDefinition, filter};

fn valid_date(value: &str) -> bool {
    ["%Y-%m-%d", "%Y/%m/%d", "%Y年%m月%d日"]
        .iter()
        .any(|format| chrono::NaiveDate::parse_from_str(value, format).is_ok())
        || chrono::NaiveDateTime::parse_from_str(value, "%m/%d/%Y %H:%M:%S").is_ok()
        || chrono::DateTime::parse_from_rfc3339(value).is_ok()
        || chrono::DateTime::parse_from_rfc2822(value).is_ok()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut failed = 0;
    for file in env::args().skip(1) {
        let audit: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file)?)?;
        for entry in audit["entries"].as_array().ok_or("missing entries")? {
            let Some(definition) = entry.get("source_definition") else {
                continue;
            };
            let source: SourceDefinition = serde_json::from_value(definition.clone())?;
            let Some(fixtures) = entry["fixtures"].as_array() else {
                if entry["status"] == "active" {
                    failed += 1;
                }
                continue;
            };
            let mut retained = 0;
            for fixture in fixtures {
                let id = fixture["route_id"]
                    .as_str()
                    .ok_or("missing fixture route_id")?;
                let route = source
                    .routes
                    .iter()
                    .find(|route| route.id == id)
                    .ok_or("unknown fixture route")?;
                let body =
                    fs::read_to_string(fixture["path"].as_str().ok_or("missing fixture path")?)?;
                match adapters::parse_route_records(&source.name, route, &body) {
                    Ok(records)
                        if !records.is_empty()
                            && records.iter().all(|record| {
                                valid_date(&record.item.date)
                                    && !record.item.title.is_empty()
                                    && url::Url::parse(&record.item.link).is_ok_and(|url| {
                                        matches!(url.scheme(), "http" | "https")
                                            && url.host_str().is_some()
                                    })
                            }) =>
                    {
                        let raw_count = records.len();
                        let (items, exclusions) = filter::apply(&source, route, records);
                        retained += items.len();
                        println!(
                            "{}",
                            serde_json::json!({"source":source.name,"route":id,"raw_items":raw_count,"items":items.len(),"exclusions":exclusions,"sample":items.iter().take(2).map(|item|serde_json::json!({"date":item.date,"title":item.title,"link":item.link})).collect::<Vec<_>>() })
                        );
                    }
                    outcome => {
                        eprintln!("{} / {}: invalid fixture: {:?}", source.name, id, outcome);
                        failed += 1;
                    }
                }
            }
            if retained == 0 {
                eprintln!("{}: no attributable in-scope fixture items", source.name);
                failed += 1;
            }
        }
    }
    if failed > 0 {
        return Err(format!("{failed} candidate fixture failures").into());
    }
    Ok(())
}
