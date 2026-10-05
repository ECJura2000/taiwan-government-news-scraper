use super::*;

pub(super) struct ClassifiedNews<'a> {
    pub item: &'a NewsItem,
    pub classification: &'a serde_json::Value,
}
pub(super) const EXCEL_HEADERS: [&str; 17] = [
    "部會",
    "新聞日期",
    "單位分類",
    "新聞標題",
    "新聞連結",
    "新聞全文",
    "日期來源",
    "關聯主題",
    "優先關聯機關",
    "關聯性",
    "關聯分數",
    "判定理由",
    "命中關鍵字",
    "排除關鍵字",
    "各主題評分",
    "BM25 排序分數",
    "開啟原文",
];

pub(super) const EXCEL_CELL_CHAR_LIMIT: usize = 32_767;
const EXCEL_LATIN_FONT: &str = "Times New Roman";
const EXCEL_CJK_FONT: &str = "標楷體";

pub(super) fn bounded_excel_text(value: &str) -> String {
    value.chars().take(EXCEL_CELL_CHAR_LIMIT).collect()
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch as u32,
            0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xF900..=0xFAFF
                | 0x20000..=0x2A6DF
                | 0x2A700..=0x2B73F
                | 0x2B740..=0x2B81F
                | 0x2B820..=0x2CEAF
        )
    })
}

pub(super) fn excel_row(item: &NewsItem, result: &serde_json::Value) -> (Vec<String>, u32, String) {
    if result["evaluated"] == false {
        let mut values = vec![String::new(); EXCEL_HEADERS.len()];
        values[0] = item.source.clone();
        values[1] = excel_date(&item.date);
        values[3] = item.title.clone();
        values[4] = item.link.clone();
        values[5] = item.summary.clone();
        return (values, 0, String::new());
    }
    let (parent_source, department_path) = excel_agency_path(&item.source, &item.department);
    let strings = |key: &str| {
        result[key]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join("、")
            })
            .unwrap_or_default()
    };
    let relevance = result["relevance"].as_str().unwrap_or("").to_owned();
    let score = result["score"].as_u64().unwrap_or(0) as u32;
    let reasons = result["reasons"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join("；")
        })
        .unwrap_or_default();
    let topic_scores = result["topic_matches"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    Some(format!(
                        "{}（{}分，{}）",
                        value["name"].as_str()?,
                        value["score"].as_u64()?,
                        value["relevance"].as_str()?
                    ))
                })
                .collect::<Vec<_>>()
                .join("；")
        })
        .unwrap_or_default();
    let source_link = if item.link.starts_with("http://") || item.link.starts_with("https://") {
        format!("{}官網：{}", item.source, item.link)
    } else {
        item.link.clone()
    };
    let values = vec![
        parent_source,
        excel_date(&item.date),
        department_path,
        item.title.clone(),
        source_link.clone(),
        item.full_text.clone(),
        item.date_source.clone(),
        if result["topics"]
            .as_array()
            .is_some_and(|topics| topics.len() >= 2)
        {
            format!("綜整性、{}", strings("topics"))
        } else {
            strings("topics")
        },
        strings("priority_sources"),
        relevance.clone(),
        score.to_string(),
        reasons,
        strings("matched_keywords"),
        strings("excluded_keywords"),
        topic_scores,
        result["bm25_score"].as_f64().unwrap_or(0.0).to_string(),
        source_link,
    ];
    (values, score, relevance)
}

pub(super) fn excel_agency_path(source: &str, department: &str) -> (String, String) {
    let base = crate::scraper::quality::affiliated_path(source);
    let mut path = if base.is_empty() {
        vec![source.to_owned()]
    } else {
        base.iter().map(|value| (*value).to_owned()).collect()
    };
    for part in department
        .split('／')
        .flat_map(|value| value.split(" / "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let part = if part == "海洋委員會海巡署" && path.iter().any(|unit| unit == "海巡署")
        {
            "海巡署"
        } else {
            part
        };
        if !path.iter().any(|existing| existing == part) {
            path.push(part.to_owned());
        }
    }
    let parent = path.first().cloned().unwrap_or_else(|| source.to_owned());
    let department = path.into_iter().skip(1).collect::<Vec<_>>().join(" / ");
    (parent, department)
}

pub(super) fn extract_http_url(value: &str) -> Option<&str> {
    let value = value.trim();
    let start = value.find("https://").or_else(|| value.find("http://"))?;
    let value = &value[start..];
    let parsed = url::Url::parse(value).ok()?;
    (matches!(parsed.scheme(), "http" | "https")
        && parsed.host_str().is_some()
        && !value.chars().any(char::is_whitespace))
    .then_some(value)
}

pub(super) fn roc_date(value: &str) -> Option<String> {
    let date = parse_date(value)?;
    Some(format!(
        "{:03}/{:02}/{:02}",
        date.year() - 1911,
        date.month(),
        date.day()
    ))
}

pub(super) fn excel_date(value: &str) -> String {
    parse_date(value)
        .map(|date| date.format("%Y/%m/%d").to_string())
        .unwrap_or_else(|| value.to_owned())
}

struct ExcelFormats {
    summary_mode: bool,
    header: Format,
    body: Format,
    latin_body: Format,
    high: Format,
    latin_high: Format,
    possible: Format,
    latin_possible: Format,
}

fn write_table_sheet(
    workbook: &mut Workbook,
    name: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    formats: &ExcelFormats,
) -> Result<(), String> {
    let worksheet = workbook
        .add_worksheet()
        .set_name(name)
        .map_err(|error| error.to_string())?;
    for (column, title) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, column as u16, *title, &formats.header)
            .map_err(|error| error.to_string())?;
    }
    for (row, values) in rows.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            let value = bounded_excel_text(value);
            let cell_format = if contains_cjk(&value) {
                &formats.body
            } else {
                &formats.latin_body
            };
            worksheet
                .write_string_with_format((row + 1) as u32, column as u16, &value, cell_format)
                .map_err(|error| error.to_string())?;
        }
    }
    worksheet
        .set_freeze_panes(1, 0)
        .map_err(|error| error.to_string())?;
    worksheet
        .autofilter(
            0,
            0,
            rows.len() as u32,
            headers.len().saturating_sub(1) as u16,
        )
        .map_err(|error| error.to_string())?;
    worksheet
        .set_row_height(0, 22)
        .map_err(|error| error.to_string())?;
    for row in 1..=rows.len() as u32 {
        worksheet
            .set_row_height(row, 22)
            .map_err(|error| error.to_string())?;
    }
    let widths: &[f64] = match name {
        "主題規則對照" => &[
            34.0, 12.0, 28.0, 16.0, 16.0, 72.0, 72.0, 72.0, 72.0, 55.0, 55.0,
        ],
        "關聯性規則" => &[34.0, 16.0, 55.0, 18.0, 10.0, 12.0, 42.0],
        "規則版本" => &[28.0, 80.0],
        _ => &[],
    };
    for (column, width) in widths.iter().enumerate() {
        worksheet
            .set_column_width(column as u16, *width)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn json_string_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(String::from)
        .collect()
}

fn policy_reference_rows(document: &serde_json::Value) -> Vec<Vec<String>> {
    let global_context = json_string_list(&document["general_keywords"]).join("、");
    let global_exclusions = json_string_list(&document["negative_keywords"]).join("、");
    document["initiatives"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|initiative| {
            vec![
                initiative["name"].as_str().unwrap_or("").into(),
                if initiative["enabled"] == false {
                    "否"
                } else {
                    "是"
                }
                .into(),
                initiative["lead_source"].as_str().unwrap_or("").into(),
                "是".into(),
                "#FFFF00".into(),
                json_string_list(&initiative["strong_keywords"])
                    .into_iter()
                    .chain(
                        initiative["weighted_keywords"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|k| {
                                k["weight"].as_f64().unwrap_or(0.0) >= 3.0 && k["enabled"] != false
                            })
                            .filter_map(|k| k["text"].as_str().map(String::from)),
                    )
                    .collect::<Vec<_>>()
                    .join("、"),
                json_string_list(&initiative["context_keywords"])
                    .into_iter()
                    .chain(
                        initiative["weighted_keywords"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|k| {
                                k["weight"].as_f64().unwrap_or(0.0) < 3.0 && k["enabled"] != false
                            })
                            .filter_map(|k| k["text"].as_str().map(String::from)),
                    )
                    .collect::<Vec<_>>()
                    .join("、"),
                String::new(),
                global_context.clone(),
                format!(
                    "扣分：{}；完全排除：{}",
                    initiative["penalty_keywords"], initiative["exclude_keywords"]
                ),
                global_exclusions.clone(),
            ]
        })
        .collect()
}

fn policy_rule_rows(document: &serde_json::Value) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for topic in document["initiatives"].as_array().into_iter().flatten() {
        let name = topic["name"].as_str().unwrap_or("");
        for key in ["weighted_keywords", "penalty_keywords", "exclude_keywords"] {
            for rule in topic[key].as_array().into_iter().flatten() {
                rows.push(vec![
                    name.into(),
                    match key {
                        "penalty_keywords" => "扣分詞",
                        "exclude_keywords" => "完全排除詞",
                        _ => "加權政策詞",
                    }
                    .into(),
                    rule["text"].as_str().unwrap_or("").into(),
                    rule.get("match_fields")
                        .map(|v| {
                            json_string_list(v)
                                .iter()
                                .map(|f| if f == "title" { "標題" } else { "摘要" })
                                .collect::<Vec<_>>()
                                .join("、")
                        })
                        .unwrap_or("標題與摘要".into()),
                    if topic["enabled"] == false {
                        "否（主題停用）"
                    } else if rule["enabled"] == false {
                        "否"
                    } else {
                        "是"
                    }
                    .into(),
                    match rule["origin"].as_str() {
                        Some("pdf") => "PDF 原詞",
                        Some("synonym") => "補充同義詞",
                        _ => "自行設定",
                    }
                    .into(),
                    match key {
                        "weighted_keywords" => {
                            format!("權重={}；依據={}", rule["weight"], rule["references"])
                        }
                        "penalty_keywords" => {
                            format!("扣分={}（多詞命中取最高值）", rule["penalty"])
                        }
                        _ => "命中即禁止本主題收錄".into(),
                    },
                ]);
            }
        }
        for (key, kind) in [
            ("exact_phrases", "完整片語"),
            ("strong_keywords", "核心詞"),
            ("context_keywords", "脈絡詞"),
        ] {
            for word in json_string_list(&topic[key]) {
                rows.push(vec![
                    name.into(),
                    kind.into(),
                    word,
                    "標題與摘要".into(),
                    if topic["enabled"] == false {
                        "否（主題停用）"
                    } else {
                        "是"
                    }
                    .into(),
                    "設定".into(),
                    if key == "context_keywords" {
                        "權重=1"
                    } else {
                        "權重=3"
                    }
                    .into(),
                ]);
            }
        }
    }
    for word in json_string_list(&document["general_keywords"]) {
        rows.push(vec![
            "共用".into(),
            "一般詞".into(),
            word,
            "標題與摘要".into(),
            "是".into(),
            "設定".into(),
            document["scoring"]["general_weight"].to_string(),
        ]);
    }
    rows
}

fn policy_version_rows(summary: &serde_json::Value) -> Vec<Vec<String>> {
    [
        ("設定格式版本", "schema_version"),
        ("設定名稱", "name"),
        ("範本版本", "template_version"),
        ("有效規則雜湊", "ruleset_hash"),
        ("設定來源", "source"),
        ("主題總數", "topic_count"),
        ("啟用主題數", "enabled_topic_count"),
        ("停用主題數", "disabled_topic_count"),
        ("關鍵字總數", "keyword_count"),
        ("啟用關鍵字數", "enabled_keyword_count"),
        ("停用關鍵字數", "disabled_keyword_count"),
        ("排除詞總數", "exclusion_count"),
        ("啟用排除詞數", "enabled_exclusion_count"),
        ("停用排除詞數", "disabled_exclusion_count"),
        ("扣分詞總數", "penalty_count"),
        ("啟用扣分詞數", "enabled_penalty_count"),
    ]
    .into_iter()
    .map(|(label, key)| {
        vec![
            label.into(),
            summary[key]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| summary[key].to_string()),
        ]
    })
    .chain(std::iter::once(vec![
        "匯出時間".into(),
        Local::now().to_rfc3339(),
    ]))
    .collect()
}

fn write_news_sheet(
    workbook: &mut Workbook,
    name: &str,
    rows: &[(Vec<String>, u32, String)],
    formats: &ExcelFormats,
) -> Result<(), String> {
    let worksheet = workbook
        .add_worksheet()
        .set_name(name)
        .map_err(|error| error.to_string())?;
    for (column, title) in EXCEL_HEADERS.iter().enumerate() {
        let title = if column == 5 && formats.summary_mode {
            "列表摘要"
        } else {
            *title
        };
        worksheet
            .write_string_with_format(0, column as u16, title, &formats.header)
            .map_err(|error| error.to_string())?;
    }
    let mut date_cells: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (row, (values, score, relevance)) in rows.iter().enumerate() {
        let row = (row + 1) as u32;
        for (column, value) in values.iter().enumerate() {
            let bounded_value = bounded_excel_text(value);
            let base_format = if contains_cjk(&bounded_value) {
                &formats.body
            } else {
                &formats.latin_body
            };
            let highlight_format = if contains_cjk(&bounded_value) {
                &formats.high
            } else {
                &formats.latin_high
            };
            let possible_relevance_format = if contains_cjk(&bounded_value) {
                &formats.possible
            } else {
                &formats.latin_possible
            };
            let cell_format = if column == 7 && !value.is_empty() {
                highlight_format
            } else if column == 10 {
                &formats.latin_body
            } else if relevance == "高度相關" {
                highlight_format
            } else if relevance == "可能相關" {
                possible_relevance_format
            } else {
                base_format
            };
            if column == 16 && extract_http_url(value).is_some() {
                let url = extract_http_url(value).expect("URL checked");
                worksheet
                    .write_url_with_text(row, column as u16, url, &bounded_value)
                    .map_err(|error| error.to_string())?;
                worksheet
                    .set_cell_format(row, column as u16, cell_format)
                    .map_err(|error| error.to_string())?;
            } else if column == 16 {
                worksheet
                    .write_string_with_format(row, column as u16, "", cell_format)
                    .map_err(|e| e.to_string())?;
            } else if column == 15 && !value.is_empty() {
                worksheet
                    .write_number_with_format(
                        row,
                        column as u16,
                        value.parse::<f64>().unwrap_or(0.0),
                        cell_format,
                    )
                    .map_err(|e| e.to_string())?;
            } else if column == 10 && !value.is_empty() {
                worksheet
                    .write_number_with_format(row, column as u16, *score as f64, cell_format)
                    .map_err(|error| error.to_string())?;
            } else {
                worksheet
                    .write_string_with_format(row, column as u16, &bounded_value, cell_format)
                    .map_err(|error| error.to_string())?;
            }
        }
        if let Some(roc) = roc_date(&values[1]) {
            date_cells
                .entry((values[1].clone(), roc))
                .or_default()
                .push(format!("B{}", row + 1));
        }
        worksheet
            .set_row_height(row, 22)
            .map_err(|error| error.to_string())?;
    }
    for ((date, roc), cells) in date_cells {
        let validation = DataValidation::new()
            .allow_list_strings(&[date.as_str(), roc.as_str()])
            .map_err(|error| error.to_string())?
            .set_input_title("新聞日期格式")
            .map_err(|error| error.to_string())?
            .set_input_message("可選擇西元紀年或民國紀年。")
            .map_err(|error| error.to_string())?
            .set_error_title("日期格式不正確")
            .map_err(|error| error.to_string())?
            .set_error_message("請選擇西元日期或民國日期。")
            .map_err(|error| error.to_string())?
            .set_multi_range(cells.join(" "));
        worksheet
            .add_data_validation(1, 1, 1, 1, &validation)
            .map_err(|error| error.to_string())?;
    }
    worksheet
        .set_row_height(0, 22)
        .map_err(|error| error.to_string())?;
    worksheet
        .set_freeze_panes(1, 0)
        .map_err(|error| error.to_string())?;
    worksheet
        .autofilter(0, 0, rows.len() as u32, (EXCEL_HEADERS.len() - 1) as u16)
        .map_err(|error| error.to_string())?;
    for (column, width) in [
        23.2, 28.0, 45.0, 120.0, 130.0, 90.0, 22.0, 36.0, 16.0, 14.0, 12.0, 65.0, 55.0, 32.0, 65.0,
        20.0, 16.0,
    ]
    .iter()
    .enumerate()
    {
        worksheet
            .set_column_width(column as u16, *width)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn sort_news_rows(rows: &mut [(Vec<String>, u32, String)]) {
    rows.sort_by(|a, b| {
        let rank = |s: &str| match s {
            "高度相關" => 0,
            "可能相關" => 1,
            "待人工判讀" => 2,
            _ => 3,
        };
        rank(&a.2)
            .cmp(&rank(&b.2))
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| {
                b.0[15]
                    .parse::<f64>()
                    .unwrap_or(0.0)
                    .total_cmp(&a.0[15].parse::<f64>().unwrap_or(0.0))
            })
            .then_with(|| b.0[1].cmp(&a.0[1]))
            .then_with(|| a.0[3].cmp(&b.0[3]))
            .then_with(|| a.0[16].cmp(&b.0[16]))
    });
}
pub(super) fn unique_sheet_name(
    name: &str,
    used: &mut std::collections::BTreeSet<String>,
) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if "[]:*?/\\".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim_matches('\'')
        .to_owned();
    let base = if base.is_empty() {
        "主題".to_owned()
    } else {
        base
    };
    for index in 0.. {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!(" ({index})")
        };
        let mut out = String::new();
        for c in base.chars() {
            if out.encode_utf16().count() + c.len_utf16() + suffix.len() > 31 {
                break;
            }
            out.push(c);
        }
        out = out.trim_end_matches('\'').to_owned();
        out.push_str(&suffix);
        if out.eq_ignore_ascii_case("history") {
            continue;
        }
        if used.insert(out.to_lowercase()) {
            return out;
        }
    }
    unreachable!()
}

pub(super) fn write_outputs(
    options: &crate::RunOptions,
    entries: &[ClassifiedNews<'_>],
    date_range: DateRange,
    profile: &crate::policy::Profile,
) -> Result<(PathBuf, PathBuf), String> {
    let output_dir = PathBuf::from(options.output_dir.as_deref().unwrap_or(DEFAULT_OUTPUT_DIR));
    let report_dir = options
        .report_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| output_dir.join("執行紀錄"));
    std::fs::create_dir_all(&output_dir).map_err(|error| {
        format!(
            "無法建立 Excel 輸出資料夾 {}：{error}",
            output_dir.display()
        )
    })?;
    std::fs::create_dir_all(&report_dir)
        .map_err(|error| format!("無法建立 JSON 報告資料夾 {}：{error}", report_dir.display()))?;
    let stamp = Local::now().format("%Y%m%d_%H%M%S_%6f").to_string();
    let workbook_path = output_dir.join(format!(
        "本週新聞整理（{}至{}）{}.xlsx",
        roc_compact(date_range.start),
        roc_compact(date_range.end),
        if options.content_mode == crate::ContentMode::Summary {
            "_摘要"
        } else {
            ""
        }
    ));
    let report_path = report_dir.join(format!("news_scraper_run_{stamp}.json"));

    let mut workbook = Workbook::new();
    let mut rows: Vec<(Vec<String>, u32, String)> = entries
        .iter()
        .map(|entry| excel_row(entry.item, entry.classification))
        .collect();
    if options.content_mode == crate::ContentMode::Summary {
        rows.sort_by(|a, b| {
            b.0[1]
                .cmp(&a.0[1])
                .then_with(|| a.0[0].cmp(&b.0[0]))
                .then_with(|| a.0[3].cmp(&b.0[3]))
                .then_with(|| a.0[4].cmp(&b.0[4]))
        });
    } else {
        sort_news_rows(&mut rows);
    }
    let mut selected_rows: Vec<(Vec<String>, u32, String)> = rows
        .iter()
        .filter(|(_, _, relevance)| relevance == "高度相關" || relevance == "可能相關")
        .cloned()
        .collect();
    sort_news_rows(&mut selected_rows);
    let formats = ExcelFormats {
        summary_mode: options.content_mode == crate::ContentMode::Summary,
        header: Format::new()
            .set_bold()
            .set_font_name(EXCEL_CJK_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_text_wrap(),
        body: Format::new()
            .set_font_name(EXCEL_CJK_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap(),
        latin_body: Format::new()
            .set_font_name(EXCEL_LATIN_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap(),
        high: Format::new()
            .set_font_name(EXCEL_CJK_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap()
            .set_background_color(Color::Yellow),
        latin_high: Format::new()
            .set_font_name(EXCEL_LATIN_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap()
            .set_background_color(Color::Yellow),
        possible: Format::new()
            .set_font_name(EXCEL_CJK_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap()
            .set_background_color(Color::RGB(0xFFF2CC)),
        latin_possible: Format::new()
            .set_font_name(EXCEL_LATIN_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap()
            .set_background_color(Color::RGB(0xFFF2CC)),
    };
    write_news_sheet(&mut workbook, "全部新聞", &rows, &formats)?;
    write_news_sheet(&mut workbook, "已初步篩選工作表", &selected_rows, &formats)?;
    let mut sheet_names: std::collections::BTreeSet<String> = [
        "全部新聞",
        "已初步篩選工作表",
        "主題規則對照",
        "關聯性規則",
        "規則版本",
        "財政部",
        "國發會",
        "國科會",
        "數發部",
        "經濟部",
    ]
    .iter()
    .map(|s| s.to_lowercase())
    .collect();
    let mut mapping = Vec::new();
    for topic in profile.initiatives.iter().filter(|t| t.enabled) {
        let sheet = unique_sheet_name(&topic.name, &mut sheet_names);
        let mut topic_rows = Vec::new();
        for entry in entries {
            let item = entry.item;
            let result = entry.classification;
            if let Some(matched) = result["topic_matches"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|m| {
                    m["name"] == topic.name
                        && m["score"].as_u64().unwrap_or(0) >= profile.thresholds.possible as u64
                        && m["hard_excluded"] != true
                })
            {
                let mut topic_result = matched.clone();
                topic_result["topics"] = json!([topic.name]);
                topic_result["topic_matches"] = json!([matched]);
                topic_rows.push(excel_row(item, &topic_result));
            }
        }
        sort_news_rows(&mut topic_rows);
        write_news_sheet(&mut workbook, &sheet, &topic_rows, &formats)?;
        mapping.push(vec![format!("主題工作表：{}", topic.name), sheet]);
    }
    let policy_document = serde_json::to_value(profile).map_err(|e| e.to_string())?;
    let reference_rows = policy_reference_rows(&policy_document);
    write_table_sheet(
        &mut workbook,
        "主題規則對照",
        &[
            "主題",
            "啟用",
            "優先關聯機關",
            "比對主題名稱",
            "顯示顏色",
            "核心詞",
            "輔助詞",
            "主題脈絡詞",
            "全域脈絡詞",
            "主題排除詞",
            "全域排除詞",
        ],
        &reference_rows,
        &formats,
    )?;
    let rule_rows = policy_rule_rows(&policy_document);
    write_table_sheet(
        &mut workbook,
        "關聯性規則",
        &[
            "主題",
            "規則類型",
            "關鍵字",
            "比對欄位",
            "啟用",
            "來源",
            "權重、扣分及政策來源",
        ],
        &rule_rows,
        &formats,
    )?;
    let policy_summary = profile.summary();
    let mut version_rows = policy_version_rows(&policy_summary);
    version_rows.extend(mapping);
    for (label, key) in [
        ("分詞器版本", "tokenizer"),
        ("政策詞典版本", "dictionary_version"),
        ("評分設定", "scoring"),
        ("政策來源", "references"),
    ] {
        version_rows.push(vec![label.into(), policy_summary[key].to_string()]);
    }
    write_table_sheet(
        &mut workbook,
        "規則版本",
        &["項目", "內容"],
        &version_rows,
        &formats,
    )?;
    for (source, sheet_name) in [
        ("財政部", "財政部"),
        ("國發會", "國發會"),
        ("國科會", "國科會"),
        ("數位發展部", "數發部"),
        ("經濟部", "經濟部"),
    ] {
        let source_rows: Vec<_> = rows
            .iter()
            .filter(|row| row.0[0] == source)
            .cloned()
            .collect();
        write_news_sheet(&mut workbook, sheet_name, &source_rows, &formats)?;
    }
    workbook
        .save(&workbook_path)
        .map_err(|error| error.to_string())?;
    Ok((workbook_path, report_path))
}
