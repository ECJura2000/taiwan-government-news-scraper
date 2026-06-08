import re
import time
from datetime import datetime
from urllib.parse import urljoin

from ....config import MOEA_RSS_TIMEOUT
from ....models import make_news_item
from ....utils.text import clean_text
from ...base import By, EC, WebDriverWait, collect_weekly_results_from_ordered_rows, create_selenium_driver, make_soup


def scrape_moea_this_week():
    source = "經濟部"
    list_url = "https://www.moea.gov.tw/Mns/Populace/news/News.aspx?kind=1&menu_id=40"

    def normalize_moea_department(raw):
        text = clean_text(raw)
        if not text:
            return ""
        text = re.sub(r"^版權來自[：:]\s*", "", text)
        text = re.sub(r"^版權來自\s*", "", text)
        text = text.replace("\u3000", " ").strip()
        if re.fullmatch(r"\d+(?:\.\d+){3,}", text):
            return ""
        if re.fullmatch(r"[0-9A-Za-z._-]+", text) and all(
            keyword not in text for keyword in ["部", "署", "局", "司", "處", "會", "院", "中心", "分署", "園區"]
        ):
            return ""
        compact = text.replace(" ", "")
        if compact == source:
            return source
        if compact.startswith("經濟部"):
            suffix = clean_text(compact[len("經濟部"):].lstrip("／/-_：: "))
            suffix = re.sub(r"^(本部新聞|新聞稿)[／/-_：: ]*", "", suffix)
            return "經濟部／{}".format(suffix) if suffix else source
        if any(keyword in text for keyword in ["署", "局", "司", "處", "會", "中心", "園區", "分署"]):
            cleaned = re.sub(r"^(本部新聞|新聞稿)[／/-_：: ]*", "", text)
            return "{}／{}".format(source, cleaned)
        return ""

    def extract_row_date(row):
        year_text = clean_text(row.select_one("span.begin-date-yy").get_text(" ", strip=True)) if row.select_one("span.begin-date-yy") else ""
        month_text = clean_text(row.select_one("span.begin-date-mm").get_text(" ", strip=True)) if row.select_one("span.begin-date-mm") else ""
        day_text = clean_text(row.select_one("span.begin-date-dd").get_text(" ", strip=True)) if row.select_one("span.begin-date-dd") else ""
        month_match = re.search(r"(\d{1,2})", month_text)
        if not year_text.isdigit() or not month_match or not day_text.isdigit():
            return None
        try:
            return datetime(int(year_text), int(month_match.group(1)), int(day_text)).date()
        except ValueError:
            return None

    def build_row_item(row, news_date):
        a_tag = row.select_one('a[href*="news_id="]')
        if not a_tag:
            return None
        title_text = clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(list_url, clean_text(a_tag.get("href", "")))
        if not title_text or not link:
            return None
        department_text = clean_text(row.select_one(".org-name").get_text(" ", strip=True)) if row.select_one(".org-name") else ""
        department_label = normalize_moea_department(department_text) or source
        return make_news_item(source, department_label, news_date, title_text, link)

    driver = create_selenium_driver()
    try:
        driver.get(list_url)
        WebDriverWait(driver, MOEA_RSS_TIMEOUT + 10).until(
            EC.presence_of_element_located((By.ID, "holderContent_grdNews"))
        )

        results = []
        seen_links = set()
        for _ in range(20):
            soup = make_soup(driver.page_source)
            rows = [row for row in soup.select("#holderContent_grdNews tbody tr") if row.select("td")]
            if not rows:
                if results:
                    break
                raise ValueError("經濟部列表頁找不到新聞列。")

            page_results, reached_older_boundary = collect_weekly_results_from_ordered_rows(
                rows,
                extract_row_date,
                build_row_item,
            )
            for item in page_results:
                if item["link"] in seen_links:
                    continue
                seen_links.add(item["link"])
                results.append(item)

            if reached_older_boundary:
                break

            next_buttons = driver.find_elements(By.ID, "holderContent_grdNews_uctlPages_dltPage_btnNext")
            if not next_buttons:
                break

            current_first_link = ""
            current_first_anchor = rows[0].select_one('a[href*="news_id="]')
            if current_first_anchor:
                current_first_link = clean_text(current_first_anchor.get("href", ""))
            driver.execute_script("arguments[0].click();", next_buttons[0])
            if current_first_link:
                WebDriverWait(driver, MOEA_RSS_TIMEOUT + 10).until(lambda d: current_first_link not in d.page_source)
            else:
                time.sleep(1)
        return results
    finally:
        driver.quit()
