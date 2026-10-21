use taiwan_government_news_lib::scraper::{
    adapters, catalog, filter, http::HttpClient, quality, transport::TransportPolicy,
};

#[tokio::test]
#[ignore = "requires live official public datastore network access"]
async fn official_public_fallback_returns_distinct_dated_news_with_full_text() {
    let source = catalog::find_source("環境部").unwrap();
    let route = source
        .routes
        .iter()
        .find(|route| route.id == "official-open-data")
        .unwrap();
    let policy = TransportPolicy::for_route(source, route);
    assert!(route.coverage_reduced);
    assert!(!policy.cache);
    let body = HttpClient::new()
        .unwrap()
        .fetch_text(&route.url, &policy)
        .await
        .unwrap();
    let records = adapters::parse_route_records(&source.name, route, &body).unwrap();
    let raw = records.len();
    let (items, _) = filter::apply(source, route, records);
    let result = quality::process(items);
    assert!(
        result.items.len() > 10,
        "distinct articles must survive their shared official dataset link"
    );
    assert!(result.items.iter().all(|item| !item.full_text.is_empty()
        && chrono::NaiveDate::parse_from_str(&item.date, "%Y-%m-%d").is_ok()));
    println!(
        "{}",
        serde_json::json!({"source":source.name,"route_id":route.id,"coverage_reduced":route.coverage_reduced,"raw_items":raw,"retained_items":result.items.len(),"invalid_count":result.invalid_count,"duplicates":result.duplicate_count,"latest_date":result.items.iter().map(|item| &item.date).max(),"oldest_date":result.items.iter().map(|item| &item.date).min(),"status":"success"})
    );
}
