use super::catalog::SourceRoute;
use super::html::{self, DatedListSelectors};
use super::{rss, special, NewsItem, ScraperError};
use scraper::{Html, Selector};

fn looks_like_feed(url: &str, body: &str) -> bool {
    let lower_url = url.to_ascii_lowercase();
    let trimmed = body
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    lower_url.contains("rss")
        || lower_url.contains("feed")
        || trimmed.starts_with("<?xml")
        || trimmed.starts_with("<rss")
        || trimmed.starts_with("<feed")
}

fn html_profile(source: &str) -> Option<DatedListSelectors<'static>> {
    let profile = match source {
        "行政院" => DatedListSelectors {
            item: "ul.grid.effect.list-group-item > li",
            link: "a[href]",
            title: ".title",
            date: ".date",
            summary: None,
            department: None,
            category: None,
        },
        "國防部" => DatedListSelectors {
            item: "div.news_list_box a.news_list",
            link: "a[href]",
            title: "div.title.headline-4",
            date: "div.date span.en",
            summary: None,
            department: None,
            category: Some("div.category.body-2"),
        },
        "中科院" => DatedListSelectors {
            item: "div.newsNavArea ul",
            link: "li.newsTit03 a[href]",
            title: "li.newsTit03 a[href]",
            date: "li.newsTit02",
            summary: None,
            department: None,
            category: None,
        },
        "國防院" => DatedListSelectors {
            item: "div.col-lg-6.card",
            link: "a[href]",
            title: "div.card-title",
            date: "p.time1",
            summary: None,
            department: None,
            category: None,
        },
        "運動部" => DatedListSelectors {
            item: "table tbody tr",
            link: "td[data-title='主題'] a[href]",
            title: "td[data-title='主題'] a[href]",
            date: "td[data-title='發布日期'] div.in, td[data-title='上版日期'] div.in",
            summary: None,
            department: Some("td[data-title='資料來源'] div.in"),
            category: None,
        },
        "中央銀行" => DatedListSelectors {
            item: "section.lp div.list ul li",
            link: "a[href]",
            title: "a[href]",
            date: "time",
            summary: None,
            department: None,
            category: None,
        },
        "內政部" => DatedListSelectors {
            item: "table tbody tr",
            link: "td:nth-child(3) a[href]",
            title: "td:nth-child(3) a[href]",
            date: "td:nth-child(1)",
            summary: None,
            department: Some("td:nth-child(2)"),
            category: None,
        },
        "消防署" => DatedListSelectors {
            item: "table tbody tr",
            link: "td:nth-child(1) a[href]",
            title: "td:nth-child(1) a[href]",
            date: "td:nth-child(3)",
            summary: None,
            department: Some("td:nth-child(2)"),
            category: None,
        },
        "國土管理署" => DatedListSelectors {
            item: "a[role='row'][href*='/ch/titlelist/news/']",
            link: "a[href]",
            title: "div[data-th='標題'] span",
            date: "div[data-th='發布日期'] span",
            summary: None,
            department: Some("div[data-th='單位分類'] span"),
            category: None,
        },
        "公路局" => DatedListSelectors {
            item: "#table_0 tbody tr",
            link: "td[data-title='標題'] a[href]",
            title: "td[data-title='標題'] a[href]",
            date: "td[data-title='公告日期']",
            summary: None,
            department: Some("td[data-title='公告單位']"),
            category: None,
        },
        "司法院" => DatedListSelectors {
            item: "table.table_list tbody tr, div.table_list table tbody tr",
            link: "td[data-title='標題'] a[href]",
            title: "td[data-title='標題'] a[href]",
            date: "td[data-title='張貼日']",
            summary: None,
            department: Some("td[data-title='單位/機關']"),
            category: None,
        },
        "教育部" => DatedListSelectors {
            item: "tr.question_tr",
            link: "td:nth-child(2) a[href]",
            title: "td:nth-child(2) a[href]",
            date: "td:nth-child(1)",
            summary: None,
            department: Some("td:nth-child(3)"),
            category: None,
        },
        "國教院" => DatedListSelectors {
            item: "ul.page-list > li",
            link: "a.txt[href], a[href]",
            title: "a.txt[href], a[href]",
            date: "div.page-list-info span.date, span.date",
            summary: None,
            department: Some("div.page-list-info span.unit, span.unit"),
            category: Some("div.page-list-info span.type, span.type"),
        },
        "衛生福利部" => DatedListSelectors {
            item: "section.list ul li",
            link: "a[href]",
            title: "p",
            date: "time",
            summary: None,
            department: None,
            category: None,
        },
        "國科會" => DatedListSelectors {
            item: "div.news_list.marb_30 a[href]",
            link: "a[href]",
            title: "h3",
            date: "div.date",
            summary: None,
            department: None,
            category: None,
        },
        "國家實驗研究院" => DatedListSelectors {
            item: "table.rwdTable tr",
            link: "td.title a[href]",
            title: "td.title a[href]",
            date: "td.date",
            summary: None,
            department: None,
            category: None,
        },
        "客委會" => DatedListSelectors {
            item: "div.list ul li",
            link: "a[href]",
            title: "p.subject",
            date: "p.color02",
            summary: None,
            department: None,
            category: None,
        },
        "觀光署" => DatedListSelectors {
            item: "div.columnBlock",
            link: "a.columnBlock-title[href]",
            title: "a.columnBlock-title[href]",
            date: "span.date",
            summary: None,
            department: None,
            category: None,
        },
        "農業金融署" => DatedListSelectors {
            item: "tbody tr",
            link: "td[data-th='標題'] a[href]",
            title: "td[data-th='標題'] a[href]",
            date: "td[data-th='發布日期']",
            summary: None,
            department: None,
            category: None,
        },
        "漁業署" => DatedListSelectors {
            item: "tbody tr",
            link: "td:first-child a[href]",
            title: "td:first-child a[href]",
            date: "td:last-child",
            summary: None,
            department: None,
            category: None,
        },
        "數位發展部" | "數位產業署" | "資通安全署" => DatedListSelectors {
            item: "ul#ListTable > li",
            link: "a[href]",
            title: ".title5",
            date: ".listDate",
            summary: None,
            department: Some(".listUnit"),
            category: None,
        },
        "環境部" => DatedListSelectors {
            item: "ul.list_group li, article.idx-news-card",
            link: "a[href]",
            title: ".idx-news-card__title, div.title, .title, h2, h3, a[href]",
            date: ".idx-news-card__date, span.date, time, .date",
            summary: None,
            department: None,
            category: None,
        },
        "退輔會" => DatedListSelectors {
            item: "section.listTb table tbody tr",
            link: "td[data-title='標題'] a[href]",
            title: "td[data-title='標題'] a[href]",
            date: "td[data-title='發布日期']",
            summary: None,
            department: None,
            category: None,
        },
        "榮總" => DatedListSelectors {
            item: "table.stackedTable tbody tr, table tbody tr",
            link: "td:nth-child(3) a[href]",
            title: "td:nth-child(3) a[href]",
            date: "td:nth-child(1)",
            summary: None,
            department: Some("td:nth-child(2)"),
            category: None,
        },
        "故宮" => DatedListSelectors {
            item: "ul.mt-12.news-list > li",
            link: "a[href]",
            title: "a[href]",
            date: "span.mr-5",
            summary: None,
            department: None,
            category: None,
        },
        _ => return None,
    };
    Some(profile)
}

pub fn parse_route(
    source: &str,
    route: &SourceRoute,
    body: &str,
) -> Result<Vec<NewsItem>, ScraperError> {
    match source {
        "國家公園署" | "國土管理署" => return special::parse_cms_json(source, body),
        "公路局" if route.parser == "thb-json" => return special::parse_thb_json(source, body),
        "外交部" => return special::parse_mofa(source, body, &route.url),
        "僑委會" => return special::parse_ocac(source, body, &route.url),
        "防檢署" => return special::parse_aphia(source, body, &route.url),
        "交通部" => return special::parse_motc(source, body, &route.url),
        "勞動部" => return special::parse_labor_list(source, body, &route.url, "h3 a[href]"),
        "勞動基金運用局" => {
            return special::parse_labor_list(source, body, &route.url, "div.item_title a[href]")
        }
        "中選會" => return special::parse_cec(source, body, &route.url),
        "疾管署" => return special::parse_cdc(source, body, &route.url),
        "國家資通安全研究院" => return special::parse_nics(source, body, &route.url),
        "經濟部" => return special::parse_moea(source, body, &route.url),
        _ => {}
    }
    if route.kind == "rss" || looks_like_feed(&route.url, body) {
        return rss::parse_feed(source, body);
    }

    if let Some(profile) = html_profile(source) {
        let mut items = html::parse_dated_list(source, body, &route.url, &profile)?;
        if source == "運動部" && items.is_empty() {
            return Err(ScraperError::ParserRegression(
                "運動部頁面未完成新聞資料渲染".into(),
            ));
        }
        if source == "環境部" {
            let trailing_date = regex::Regex::new(r"\s+\d{2,4}-\d{1,2}-\d{1,2}$")
                .expect("valid trailing date regex");
            for item in &mut items {
                item.title = trailing_date.replace(&item.title, "").trim().to_owned();
            }
        }
        if source == "榮總" {
            let leading_label = regex::Regex::new(r"^點擊前往\s*")
                .expect("valid Veterans General Hospital title regex");
            for item in &mut items {
                item.title = leading_label.replace(&item.title, "").trim().to_owned();
            }
        }
        return Ok(items);
    }

    match route.parser.as_str() {
        "ey-html" => unreachable!("Executive Yuan profile dispatched above"),
        "primary"
        | "standard"
        | "mjac-html"
        | "afna-html"
        | "fa-html"
        | "sfaa-html"
        | "mohw-source-filter"
        | "moenv-html"
        | "moenv-news-portal-browser"
        | "pcc-html" => html::parse_link_list(source, body, Some(&route.url), "a[href]"),
        parser => Err(ScraperError::ParserRegression(format!(
            "unsupported Rust parser adapter: {parser}"
        ))),
    }
}

pub fn parse_detail_summary(source: &str, body: &str) -> String {
    let selectors: &[&str] = match source {
        "數位發展部" | "數位產業署" | "資通安全署" => {
            &[".article1.cpArticle", ".cpArticle", "article"]
        }
        "國科會" => &[".articleContent", ".article-content", "article"],
        "經濟部" => &[
            "#ctl00_ContentPlaceHolder1_divContent",
            ".article-content",
            ".cpArticle",
            "article",
        ],
        _ => return String::new(),
    };
    let document = Html::parse_document(body);
    for raw_selector in selectors {
        let Ok(selector) = Selector::parse(raw_selector) else {
            continue;
        };
        let Some(container) = document.select(&selector).next() else {
            continue;
        };
        let paragraph_selector = Selector::parse("p").expect("valid paragraph selector");
        let paragraphs = container
            .select(&paragraph_selector)
            .map(html::clean_text)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let text = if paragraphs.is_empty() {
            html::clean_text(container)
        } else {
            paragraphs.join(" ")
        };
        if !text.is_empty() {
            return text
                .chars()
                .take(1_200)
                .collect::<String>()
                .trim()
                .to_owned();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(parser: &str) -> SourceRoute {
        SourceRoute {
            id: "test".into(),
            url: "https://example.test/list".into(),
            kind: "html".into(),
            parser: parser.into(),
            priority: 1,
            official: true,
            coverage_reduced: false,
        }
    }

    #[test]
    fn dispatches_executive_yuan_adapter() {
        let items = parse_route(
            "行政院",
            &route("ey-html"),
            include_str!("../../tests/fixtures/ey_list.html"),
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].date, "2026-08-06");
        assert_eq!(items[0].title, "政院推動主權 AI 建設");
        assert_eq!(items[0].summary, "");
        assert_eq!(
            items[0].link,
            "https://example.test/Page/9277F759E41CCD91/item-one"
        );
        assert_eq!(items[1].date, "2026-08-05");
    }

    #[test]
    fn parses_current_sports_ministry_date_label() {
        let body = r#"
            <table><tbody><tr>
              <td data-title="主題"><div class="in"><a href="/News_Content/309/23641">總統盃街舞大賽預賽啟動</a></div></td>
              <td data-title="資料來源"><div class="in">運動部全民運動署</div></td>
              <td data-title="發布日期"><div class="in">2026-08-29</div></td>
            </tr></tbody></table>
        "#;
        let mut sports_route = route("sports-html");
        sports_route.url = "https://www.sports.gov.tw/News/309".into();
        let items = parse_route("運動部", &sports_route, body).unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].date, "2026-08-29");
        assert_eq!(items[0].title, "總統盃街舞大賽預賽啟動");
        assert_eq!(items[0].department, "運動部／運動部全民運動署");
        assert_eq!(
            items[0].link,
            "https://www.sports.gov.tw/News_Content/309/23641"
        );
    }

    #[test]
    fn rejects_unrendered_sports_ministry_template() {
        let body = r#"
            <table><tbody><tr v-for="item in items">
              <td data-title="主題"><a :href="item.Path">{{item.主題}}</a></td>
              <td data-title="發布日期"><div class="in">{{item.發布日期}}</div></td>
            </tr></tbody></table>
        "#;
        let mut sports_route = route("sports-html");
        sports_route.url = "https://www.sports.gov.tw/News/309".into();
        let error = parse_route("運動部", &sports_route, body).unwrap_err();

        assert!(matches!(error, ScraperError::ParserRegression(_)));
    }

    #[test]
    fn rejects_unregistered_adapter_names() {
        let error = parse_route("測試", &route("missing-adapter"), "<html></html>").unwrap_err();
        assert!(matches!(error, ScraperError::ParserRegression(_)));
    }

    #[test]
    fn extracts_detail_summary_with_python_length_limit() {
        let body = format!(
            "<article class='cpArticle'><p>{}</p><p>第二段</p></article>",
            "字".repeat(1_250)
        );
        let summary = parse_detail_summary("數位發展部", &body);
        assert_eq!(summary.chars().count(), 1_200);
        assert!(summary.starts_with("字字字"));
    }
}
