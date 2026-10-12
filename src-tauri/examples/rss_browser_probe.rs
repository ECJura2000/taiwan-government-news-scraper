use taiwan_government_news_lib::scraper::{adapters, catalog};
#[tokio::main]
async fn main() {
    std::fs::create_dir_all("diagnostics").unwrap();
    let source = catalog::find_source("文化部").unwrap();
    let route = catalog::routes_for(source)
        .into_iter()
        .find(|r| r.kind == "browser")
        .unwrap();
    match taiwan_government_news_lib::browser::fetch_rendered_html_after(&route.url, None).await {
        Ok(body) => {
            std::fs::write("diagnostics/moc-browser-body.html", &body).unwrap();
            let parsed = adapters::parse_route("文化部", &route, &body);
            println!(
                "{}",
                serde_json::json!({"url":route.url,"bytes":body.len(),"prefix":body.chars().take(1800).collect::<String>(),"items":parsed.as_ref().ok().map(Vec::len),"error":parsed.err().map(|e|e.to_string())})
            );
        }
        Err(error) => println!("{}", serde_json::json!({"browser_error":error})),
    }
}
