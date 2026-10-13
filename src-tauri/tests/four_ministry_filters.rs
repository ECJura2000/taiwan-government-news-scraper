use serde_json::{json, Value};
use taiwan_government_news_lib::scraper::{
    adapters,
    catalog::{SourceDefinition, SourceRoute},
    filter, rss,
};

fn definition(filters: Value) -> SourceDefinition {
    serde_json::from_value(json!({
        "name": "子館", "urls": ["https://official.test/news/list"],
        "filters": filters
    }))
    .unwrap()
}

fn route(kind: &str) -> SourceRoute {
    serde_json::from_value(json!({
        "id": "test-route", "url": "https://official.test/news/list",
        "kind": kind, "parser": if kind == "rss" { "standard-rss" } else { "standard" },
        "selectors": if kind == "html" {
            json!({"item":"article", "link":"a", "title":"a", "date":"time", "category":".category"})
        } else { Value::Null }
    }))
    .unwrap()
}

fn rss_item(title: &str, categories: &[&str], metadata: &str) -> String {
    let categories = categories
        .iter()
        .map(|category| format!("<category>{category}</category>"))
        .collect::<String>();
    format!(
        "<item><title>{title}</title><link>https://official.test/article</link>\
         <pubDate>2026-10-05</pubDate>{categories}{metadata}</item>"
    )
}

fn feed(items: &str) -> String {
    format!("<rss><channel>{items}</channel></rss>")
}

#[test]
fn rss_and_html_make_the_same_announcement_selection() {
    let policy = definition(json!({"include_categories":["新聞","活動","公告"]}));
    let articles = [
        ("新展覽開幕", "新聞"),
        ("藝術家駐村甄選公告", "活動"),
        ("課程報名開放", "公告"),
        ("【徵才】研究助理", "徵才"),
        ("【招標公告】空調工程", "採購"),
    ];
    let xml = feed(
        &articles
            .iter()
            .map(|(title, category)| rss_item(title, &[*category], ""))
            .collect::<String>(),
    );
    let html = articles
        .iter()
        .map(|(title, category)| {
            format!("<article><a href='/article'>{title}</a><time>2026-10-05</time><span class='category'>{category}</span></article>")
        })
        .collect::<String>();
    let mut selections = Vec::new();
    for (kind, body) in [("rss", xml), ("html", html)] {
        let route = route(kind);
        let records = adapters::parse_route_records("子館", &route, &body).unwrap();
        let (items, decisions) = filter::apply(&policy, &route, records);
        assert_eq!(decisions.len(), 2);
        selections.push(items.into_iter().map(|item| item.title).collect::<Vec<_>>());
    }
    let expected = ["新展覽開幕", "藝術家駐村甄選公告", "課程報名開放"];
    assert_eq!(selections[0], expected);
    assert_eq!(selections[1], expected);
}

#[test]
fn procurement_in_a_second_category_is_excluded_consistently_in_rss_and_html() {
    let xml = feed(&rss_item("展覽設備公告", &["新聞", "採購"], ""));
    let html = "<article><a href='/article'>展覽設備公告</a><time>2026-10-05</time><span class='category'>新聞</span><span class='category'>採購</span></article>";
    for (kind, body) in [("rss", xml.as_str()), ("html", html)] {
        let route = route(kind);
        let records = adapters::parse_route_records("子館", &route, body).unwrap();
        assert_eq!(records[0].categories, ["新聞", "採購"]);
        assert_eq!(records[0].item.category, "新聞");
        let (items, decisions) = filter::apply(
            &definition(json!({"include_categories":["新聞"]})),
            &route,
            records,
        );
        assert!(items.is_empty(), "{kind}");
        assert_eq!(decisions[0]["reason"], "administrative_category", "{kind}");
    }
}

#[test]
fn cms_json_preserves_official_single_category_and_publishing_unit_for_shared_filters() {
    let route: SourceRoute = serde_json::from_value(json!({
        "id":"official-json", "url":"https://www.nps.gov.tw/sites/www.nps.gov.tw/ch/main/parknews/list.json",
        "kind":"json", "parser":"nps-json"
    })).unwrap();
    let body = r#"[{"id":5195,"title":"海洋保育新利器","publish_up":"2026-10-07T06:00:00.000Z","jsh_unit":{"name":"海洋國家公園管理處"},"jsh_titlelist_categories":{"title":"公園新聞"}}]"#;
    let records = adapters::parse_route_records("國家公園署", &route, body).unwrap();
    assert_eq!(records[0].categories, ["公園新聞"]);
    assert_eq!(records[0].item.category, "公園新聞");
    assert_eq!(records[0].publisher.as_deref(), Some("海洋國家公園管理處"));
    let retained_policy = definition(
        json!({"include_categories":["公園新聞"],"publisher_allowlist":["海洋國家公園管理處"]}),
    );
    let (items, decisions) = filter::apply(&retained_policy, &route, records.clone());
    assert_eq!(items.len(), 1);
    assert!(decisions.is_empty());
    let excluded_policy = definition(json!({"exclude_categories":["公園新聞"]}));
    let (items, decisions) = filter::apply(&excluded_policy, &route, records);
    assert!(items.is_empty());
    assert_eq!(decisions[0]["reason"], "excluded_category");
    let procurement = body.replace("公園新聞", "採購");
    let records = adapters::parse_route_records("國家公園署", &route, &procurement).unwrap();
    let (items, decisions) = filter::apply(&definition(json!({})), &route, records);
    assert!(items.is_empty());
    assert_eq!(decisions[0]["reason"], "administrative_category");
    let mut creator_only: Value = serde_json::from_str(body).unwrap();
    creator_only[0].as_object_mut().unwrap().remove("jsh_unit");
    creator_only[0]["created_by"] = json!(105);
    let records =
        adapters::parse_route_records("國家公園署", &route, &creator_only.to_string()).unwrap();
    assert!(records[0].publisher.is_none());
}

#[test]
fn html_multiple_categories_follow_their_own_rows_even_with_a_shared_link() {
    let route = route("html");
    let html = "<article><a href='/article'>展覽公告</a><time>2026-10-05</time><span class='category'>新聞</span><span class='category'>活動</span></article><article><a href='/article'>設備公告</a><time>2026-10-05</time><span class='category'>新聞</span><span class='category'>採購</span></article><article><a href='/empty-first'>工程公告</a><time>2026-10-05</time><span class='category'></span><span class='category'>採購</span></article>";
    let records = adapters::parse_route_records("子館", &route, html).unwrap();
    assert_eq!(records[0].categories, ["新聞", "活動"]);
    assert_eq!(records[1].categories, ["新聞", "採購"]);
    assert_eq!(records[2].categories, ["採購"]);
    let (items, decisions) = filter::apply(&definition(json!({})), &route, records);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "展覽公告");
    assert_eq!(decisions.len(), 2);
}

#[test]
fn arts_calls_and_course_registration_are_not_staff_recruitment() {
    let route = route("rss");
    let retained = [
        "展覽徵件公告",
        "藝術創作徵件",
        "藝術家駐村甄選",
        "課程報名公告",
    ];
    let excluded = ["【徵才】館務研究員", "【採購公告】展覽設備財物採購"];
    let xml = feed(
        &retained
            .iter()
            .chain(excluded.iter())
            .map(|title| rss_item(title, &["公告"], ""))
            .collect::<String>(),
    );
    let records = adapters::parse_route_records("子館", &route, &xml).unwrap();
    let (items, decisions) = filter::apply(&definition(json!({})), &route, records);
    assert_eq!(
        items
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        retained
    );
    assert_eq!(decisions.len(), 2);
    assert!(decisions
        .iter()
        .all(|decision| decision["reason"] == "administrative_title"));
}

#[test]
fn explicitly_administrative_categories_are_excluded_even_with_arts_titles() {
    let route = route("rss");
    let xml = feed(&format!(
        "{}{}",
        rss_item("展覽研究", &["徵才公告"], ""),
        rss_item("藝術節舞臺", &["招標公告"], "")
    ));
    let records = adapters::parse_route_records("子館", &route, &xml).unwrap();
    let (items, decisions) = filter::apply(&definition(json!({})), &route, records);
    assert!(items.is_empty());
    assert_eq!(decisions.len(), 2);
}

#[test]
fn a_requested_source_name_is_not_evidence_of_a_shared_feeds_publisher() {
    let route = route("rss");
    let records = adapters::parse_route_records(
        "子館",
        &route,
        &feed(&rss_item("子館展覽開幕", &["新聞"], "")),
    )
    .unwrap();
    assert!(records[0].publisher.is_none());
    let (items, decisions) = filter::apply(
        &definition(json!({"publisher_allowlist":["子館"]})),
        &route,
        records,
    );
    assert!(items.is_empty());
    assert_eq!(decisions[0]["reason"], "publisher_not_proven");
}

#[test]
fn rss_author_does_not_prove_an_institution_published_the_article() {
    let route = route("rss");
    let records = adapters::parse_route_records(
        "子館",
        &route,
        &feed(&rss_item("藝術家投稿", &["新聞"], "<author>子館</author>")),
    )
    .unwrap();
    let (items, decisions) = filter::apply(
        &definition(json!({"publisher_allowlist":["子館"]})),
        &route,
        records,
    );
    assert!(
        items.is_empty(),
        "author is authorship, not the issuing institution"
    );
    assert_eq!(decisions[0]["reason"], "publisher_not_proven");
}

#[test]
fn publisher_allowlist_uses_explicit_normalized_evidence_and_records_rejections() {
    let route = route("rss");
    let xml = feed(&format!(
        "{}{}{}",
        rss_item(
            "正確發布單位",
            &["新聞"],
            "<publisher> Museum   Board </publisher>"
        ),
        rss_item("其他單位", &["新聞"], "<publisher>Other Board</publisher>"),
        rss_item("發布單位未明", &["新聞"], "")
    ));
    let records = adapters::parse_route_records("子館", &route, &xml).unwrap();
    let (items, decisions) = filter::apply(
        &definition(json!({"publisher_allowlist":["museum board"]})),
        &route,
        records,
    );
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "正確發布單位");
    assert_eq!(decisions[0]["reason"], "different_publisher");
    assert_eq!(decisions[1]["reason"], "publisher_not_proven");
}

#[test]
fn atom_publication_date_wins_even_when_updated_comes_first() {
    let xml = "<feed><entry><title>館舍活動</title><link rel='alternate' href='https://official.test/a'/><updated>2026-10-12T10:00:00+08:00</updated><published>2026-10-05T09:00:00+08:00</published></entry></feed>";
    let records = rss::parse_feed_records("子館", xml).unwrap();
    assert_eq!(records[0].item.date, "2026-10-05T09:00:00+08:00");
    assert_eq!(records[0].item.date_source, "published");
}

#[test]
fn atom_updated_only_preserves_its_date_evidence_label() {
    let xml = "<feed><entry><title>更新公告</title><link href='https://official.test/a'/><updated>2026-10-05T09:00:00Z</updated></entry></feed>";
    let records = rss::parse_feed_records("子館", xml).unwrap();
    assert_eq!(records[0].item.date_source, "updated");
}

#[test]
fn atom_alternate_is_the_article_not_self_or_enclosure() {
    let xml = "<feed><entry><title>館舍活動</title><link rel='self' href='https://official.test/atom/entry'/><link rel='alternate' href='https://official.test/article'/><link rel='enclosure' href='https://official.test/audio.mp3'/><published>2026-10-05</published></entry></feed>";
    let records = rss::parse_feed_records("子館", xml).unwrap();
    assert_eq!(records[0].item.link, "https://official.test/article");
    let attachments_only = "<feed><entry><title>無文章連結</title><link rel='self' href='https://official.test/atom/entry'/><link rel='enclosure' href='https://official.test/audio.mp3'/></entry></feed>";
    assert!(rss::parse_feed_records("子館", attachments_only)
        .unwrap()
        .is_empty());
}

#[test]
fn relative_atom_alternate_resolves_against_the_official_route() {
    let xml = "<feed><entry><title>館舍活動</title><link rel='alternate' href='../articles/42'/><published>2026-10-05</published></entry></feed>";
    let records = adapters::parse_route_records("子館", &route("rss"), xml).unwrap();
    assert_eq!(records[0].item.link, "https://official.test/articles/42");
}

#[test]
fn non_permalink_guid_is_never_an_article_url() {
    let xml = "<rss><channel><item><title>內部識別碼</title><guid isPermaLink='false'>https://official.test/private-id/42</guid><pubDate>2026-10-05</pubDate></item></channel></rss>";
    assert!(rss::parse_feed_records("子館", xml).unwrap().is_empty());
}

#[test]
fn html_dates_support_separated_dom_and_english_ordinals() {
    for date in [
        "2026 10/05",
        "<span>2026/10</span><span>/</span><span>05</span>",
        "October 5th, 2026",
        "5th October 2026",
    ] {
        let html = format!("<article><a href='/article'>館舍活動</a><time>{date}</time></article>");
        let items = adapters::parse_route("子館", &route("html"), &html).unwrap();
        assert_eq!(items.len(), 1, "publication date {date}");
        assert_eq!(items[0].date, "2026-10-05");
    }
}

#[test]
fn title_attribute_removes_row_numbers_categories_and_dates_from_the_title() {
    let mut route = route("html");
    route.selectors.as_mut().unwrap().title_attribute = Some("title".into());
    let html = "<article><a href='/article' title='館舍展覽開幕'><span>1</span><b>新聞</b>館舍展覽開幕<time>115-10-05</time></a></article>";
    let items = adapters::parse_route("子館", &route, html).unwrap();
    assert_eq!(items[0].title, "館舍展覽開幕");
    assert_eq!(items[0].date, "2026-10-05");
}

#[test]
fn missing_dates_are_left_unproven_instead_of_becoming_the_requested_week() {
    let xml = "<rss><channel><item><title>未附發布日期</title><link>https://official.test/article</link></item></channel></rss>";
    let records = rss::parse_feed_records("子館", xml).unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].item.date.is_empty());
    let html =
        "<article><a href='/article'>未附發布日期</a><time>活動將於十月舉辦</time></article>";
    assert!(adapters::parse_route("子館", &route("html"), html)
        .unwrap()
        .is_empty());
}
