use super::html::{clean_text, parse_list_date};
use super::{NewsItem, ScraperError};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use url::Url;

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("embedded selector must be valid")
}

fn resolve(base: &str, link: &str) -> String {
    Url::parse(base)
        .ok()
        .and_then(|base| base.join(link).ok())
        .map(|url| url.to_string())
        .unwrap_or_else(|| link.to_owned())
}

fn item(source: &str, date: chrono::NaiveDate, title: String, link: String) -> NewsItem {
    NewsItem {
        source: source.to_owned(),
        date: date.to_string(),
        department: source.to_owned(),
        title,
        link,
        category: String::new(),
        summary: String::new(),
        date_source: "published".to_owned(),
    }
}

fn require_rows<'a>(
    source: &str,
    document: &'a Html,
    css: &str,
) -> Result<Vec<ElementRef<'a>>, ScraperError> {
    let rows: Vec<_> = document.select(&selector(css)).collect();
    if rows.is_empty() {
        Err(ScraperError::ParserRegression(format!(
            "{source} page does not contain {css}"
        )))
    } else {
        Ok(rows)
    }
}

pub fn parse_mofa(source: &str, body: &str, base: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let rows = require_rows(source, &document, "table tbody tr")?;
    let link_selector = selector("a[href]");
    let mut items = Vec::new();
    for row in rows {
        let Some(date) = parse_list_date(&clean_text(row)) else {
            continue;
        };
        let Some(anchor) = row.select(&link_selector).next() else {
            continue;
        };
        let title = clean_text(anchor);
        let Some(link) = anchor.value().attr("href") else {
            continue;
        };
        if !title.is_empty() {
            items.push(item(source, date, title, resolve(base, link)));
        }
    }
    Ok(items)
}

pub fn parse_ocac(source: &str, body: &str, base: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let rows = require_rows(source, &document, "ul.text_list li")?;
    let link_selector = selector("a[href]");
    let date_regex = Regex::new(r"^(\d{4}/\d{1,2}/\d{1,2})\s*(.+)$").expect("valid regex");
    let mut items = Vec::new();
    for row in rows {
        let Some(anchor) = row.select(&link_selector).next() else {
            continue;
        };
        let text = clean_text(anchor);
        let Some(captures) = date_regex.captures(&text) else {
            continue;
        };
        let Some(date) = parse_list_date(captures.get(1).expect("date capture").as_str()) else {
            continue;
        };
        let title = captures.get(2).expect("title capture").as_str().trim();
        let Some(link) = anchor.value().attr("href") else {
            continue;
        };
        if !title.is_empty() {
            items.push(item(source, date, title.to_owned(), resolve(base, link)));
        }
    }
    Ok(items)
}

pub fn parse_aphia(source: &str, body: &str, base: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let rows = require_rows(
        source,
        &document,
        "a[href*='theme_data.php?theme=NewInfoListWS']",
    )?;
    let date_regex = Regex::new(r"^(\d{2,3}-\d{1,2}-\d{1,2})\s+(.+)$").expect("valid regex");
    let mut items = Vec::new();
    for anchor in rows {
        let text = clean_text(anchor);
        let Some(captures) = date_regex.captures(&text) else {
            continue;
        };
        let Some(date) = parse_list_date(captures.get(1).expect("date capture").as_str()) else {
            continue;
        };
        let Some(link) = anchor.value().attr("href") else {
            continue;
        };
        items.push(item(
            source,
            date,
            captures
                .get(2)
                .expect("title capture")
                .as_str()
                .trim()
                .to_owned(),
            resolve(base, link),
        ));
    }
    Ok(items)
}

fn labeled_value(text: &str, label: &str) -> String {
    let pattern = format!(r"{}\s*[:：]?\s*([^\s]+)", regex::escape(label));
    Regex::new(&pattern)
        .ok()
        .and_then(|regex| regex.captures(text))
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_owned())
        .unwrap_or_default()
}

fn labeled_date(text: &str, label: &str) -> Option<chrono::NaiveDate> {
    let pattern = format!(
        r"{}\s*[:：]?\s*(\d{{2,4}}[-/.]\d{{1,2}}[-/.]\d{{1,2}})",
        regex::escape(label)
    );
    Regex::new(&pattern)
        .ok()?
        .captures(text)?
        .get(1)
        .and_then(|value| parse_list_date(value.as_str()))
}

pub fn parse_motc(source: &str, body: &str, base: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let rows = require_rows(source, &document, "div.list_group div.list ul > li")?;
    let link_selector = selector("a[href]");
    let mut items = Vec::new();
    for row in rows {
        let Some(anchor) = row.select(&link_selector).next() else {
            continue;
        };
        let text = clean_text(anchor);
        let Some(date) = parse_list_date(&text) else {
            continue;
        };
        let title = anchor
            .value()
            .attr("title")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| clean_text(anchor));
        let Some(link) = anchor.value().attr("href") else {
            continue;
        };
        let mut news = item(source, date, title, resolve(base, link));
        let department = labeled_value(&text, "發布單位");
        if !department.is_empty() && department != source {
            news.department = format!("{source}／{department}");
        }
        items.push(news);
    }
    Ok(items)
}

pub fn parse_labor_list(
    source: &str,
    body: &str,
    base: &str,
    title_css: &str,
) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let rows = require_rows(source, &document, "div.item_listblock div.item_list2")?;
    let title_selector = selector(title_css);
    let data_selector = selector("div.data");
    let mut items = Vec::new();
    for row in rows {
        let Some(anchor) = row.select(&title_selector).next() else {
            continue;
        };
        let Some(data) = row.select(&data_selector).next() else {
            continue;
        };
        let metadata = clean_text(data);
        let Some(date) =
            labeled_date(&metadata, "發布日期").or_else(|| labeled_date(&metadata, "更新日期"))
        else {
            continue;
        };
        let Some(link) = anchor.value().attr("href") else {
            continue;
        };
        let mut news = item(source, date, clean_text(anchor), resolve(base, link));
        let department = labeled_value(&metadata, "發布單位");
        if !department.is_empty() && department != source {
            news.department = format!("{source}／{department}");
        }
        items.push(news);
    }
    Ok(items)
}

pub fn parse_cec(source: &str, body: &str, base: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let rows = require_rows(source, &document, "div.article-item")?;
    let date_regex = Regex::new(r"\b(\d{3}\.\d{2}\.\d{2})\b").expect("valid regex");
    let mut items = Vec::new();
    for row in rows {
        let text = clean_text(row);
        let Some(captures) = date_regex.captures(&text) else {
            continue;
        };
        let raw_date = captures.get(1).expect("date capture").as_str();
        let Some(date) = parse_list_date(raw_date) else {
            continue;
        };
        let title = text.replace(raw_date, "").trim().to_owned();
        if !title.is_empty() {
            items.push(item(source, date, title, base.to_owned()));
        }
    }
    Ok(items)
}

pub fn parse_cdc(source: &str, body: &str, base: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let rows = require_rows(source, &document, "a[href*='/Bulletin/Detail/']")?;
    let year_selector = selector(".icon-year");
    let day_selector = selector(".icon-date");
    let year_month = Regex::new(r"(\d{4})\s*-\s*(\d{1,2})").expect("valid regex");
    let mut items = Vec::new();
    for anchor in rows {
        let Some(year_node) = anchor.select(&year_selector).next() else {
            continue;
        };
        let Some(day_node) = anchor.select(&day_selector).next() else {
            continue;
        };
        let year_text = clean_text(year_node);
        let Some(parts) = year_month.captures(&year_text) else {
            continue;
        };
        let date_text = format!(
            "{}-{}-{}",
            parts.get(1).expect("year").as_str(),
            parts.get(2).expect("month").as_str(),
            clean_text(day_node)
        );
        let Some(date) = parse_list_date(&date_text) else {
            continue;
        };
        let title = anchor
            .value()
            .attr("title")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| clean_text(anchor));
        let Some(link) = anchor.value().attr("href") else {
            continue;
        };
        items.push(item(source, date, title, resolve(base, link)));
    }
    Ok(items)
}

pub fn parse_cms_json(source: &str, body: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let rows: Vec<Value> = serde_json::from_str(body)
        .map_err(|error| ScraperError::ParserRegression(error.to_string()))?;
    let mut items = Vec::new();
    for row in rows {
        let Some(date) = row["publish_up"].as_str().and_then(parse_list_date) else {
            continue;
        };
        let Some(title) = row["title"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let id = row["id"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| row["id"].as_u64().map(|value| value.to_string()))
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let link = match source {
            "國家公園署" => format!("https://www.nps.gov.tw/ch/titlelist/parknews/{id}"),
            "國土管理署" => format!("https://www.nlma.gov.tw/ch/titlelist/news/{id}"),
            _ => {
                return Err(ScraperError::ParserRegression(
                    "unsupported CMS JSON source".into(),
                ))
            }
        };
        let mut news = item(source, date, title.to_owned(), link);
        if let Some(department) = row["jsh_unit"]["name"].as_str().map(str::trim) {
            if !department.is_empty() && !["內政部國家公園署", "國家公園署"].contains(&department)
            {
                news.department = format!("{source}／{department}");
            }
        }
        items.push(news);
    }
    Ok(items)
}

pub fn parse_thb_json(source: &str, body: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let rows: Vec<Value> = serde_json::from_str(body)
        .map_err(|error| ScraperError::ParserRegression(error.to_string()))?;
    let mut items = Vec::new();
    for row in rows {
        let Some(date) = row["公告日期"].as_str().and_then(parse_list_date) else {
            continue;
        };
        let Some(title) = row["Title"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(link) = row["Source"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let mut news = item(source, date, title.to_owned(), link.to_owned());
        if let Some(department) = row["資料來源"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != source)
        {
            news.department = format!("{source}／{department}");
        }
        if let Some(content) = row["內容"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            news.summary = clean_text(Html::parse_fragment(content).root_element());
        }
        items.push(news);
    }
    Ok(items)
}

pub fn parse_nics(source: &str, body: &str, base: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let script = document
        .select(&selector("script#__NEXT_DATA__"))
        .next()
        .ok_or_else(|| ScraperError::ParserRegression("missing __NEXT_DATA__".into()))?;
    let data: Value = serde_json::from_str(&script.text().collect::<String>())
        .map_err(|error| ScraperError::ParserRegression(error.to_string()))?;
    let rows = data["props"]["pageProps"]["data"]["content"][0]["data"]["item"]
        .as_array()
        .ok_or_else(|| ScraperError::ParserRegression("invalid NICS item array".into()))?;
    let mut items = Vec::new();
    for row in rows {
        let Some(date) = row["date"].as_str().and_then(parse_list_date) else {
            continue;
        };
        let Some(title) = row["label"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(link) = row["link"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        items.push(item(source, date, title.to_owned(), resolve(base, link)));
    }
    Ok(items)
}

pub fn parse_moea(source: &str, body: &str, base: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(body);
    let rows = require_rows(source, &document, "#holderContent_grdNews tbody tr")?;
    let year_selector = selector("span.begin-date-yy");
    let month_selector = selector("span.begin-date-mm");
    let day_selector = selector("span.begin-date-dd");
    let link_selector = selector("a[href*='news_id=']");
    let department_selector = selector(".org-name");
    let digits = Regex::new(r"\d+").expect("valid regex");
    let mut items = Vec::new();
    for row in rows {
        let Some(year) = row.select(&year_selector).next().map(clean_text) else {
            continue;
        };
        let Some(month_text) = row.select(&month_selector).next().map(clean_text) else {
            continue;
        };
        let Some(day) = row.select(&day_selector).next().map(clean_text) else {
            continue;
        };
        let Some(month) = digits.find(&month_text).map(|value| value.as_str()) else {
            continue;
        };
        let Some(date) = parse_list_date(&format!("{year}-{month}-{day}")) else {
            continue;
        };
        let Some(anchor) = row.select(&link_selector).next() else {
            continue;
        };
        let Some(link) = anchor.value().attr("href") else {
            continue;
        };
        let mut news = item(source, date, clean_text(anchor), resolve(base, link));
        if let Some(department) = row.select(&department_selector).next().map(clean_text) {
            let department = department
                .trim_start_matches("版權來自：")
                .trim_start_matches("版權來自:")
                .trim();
            if !department.is_empty() && department != source {
                news.department = if department.starts_with(source) {
                    department.to_owned()
                } else {
                    format!("{source}／{department}")
                };
            }
        }
        items.push(news);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ocac_date_prefix_without_leaking_it_into_title() {
        let items = parse_ocac(
            "僑委會",
            r#"<ul class="text_list"><li><a href="/1">2026/08/06 僑務新聞</a></li></ul>"#,
            "https://example.test/list",
        )
        .unwrap();
        assert_eq!(items[0].date, "2026-08-06");
        assert_eq!(items[0].title, "僑務新聞");
    }

    #[test]
    fn parses_nps_json_rows() {
        let items = parse_cms_json(
            "國家公園署",
            r#"[{"publish_up":"2026-08-06T01:00:00Z","title":"公園新聞","id":"abc","jsh_unit":{"name":"國家公園署"}}]"#,
        )
        .unwrap();
        assert_eq!(items[0].date, "2026-08-06");
        assert!(items[0].link.ends_with("/abc"));
    }

    #[test]
    fn parses_thb_open_data_rows() {
        let items = parse_thb_json(
            "公路局",
            r#"[{"Source":"https://www.thb.gov.tw/News_Content_table.aspx?n=12181&s=1","Title":"道路新聞","公告日期":"2026-08-03T16:37:00","內容":"<p>施工交管</p>","資料來源":"東區養護工程分局-秘書室"}]"#,
        )
        .unwrap();
        assert_eq!(items[0].date, "2026-08-03");
        assert_eq!(items[0].department, "公路局／東區養護工程分局-秘書室");
        assert_eq!(items[0].summary, "施工交管");
    }
}
