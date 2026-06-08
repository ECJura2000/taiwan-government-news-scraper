import argparse
import logging
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

from .runtime import validate_runtime_environment

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
logger = logging.getLogger(__name__)

LAST_FAILED_SOURCES = []


def parse_args():
    from .config import DEFAULT_OUTPUT_DIR, MAX_WORKERS

    parser = argparse.ArgumentParser(description="抓取本週各部會新聞並匯出 Excel。")
    parser.add_argument("--sources", nargs="+", help="只抓指定來源，例如：--sources 財政部 法務部")
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Excel 輸出資料夾")
    parser.add_argument("--max-workers", type=int, default=MAX_WORKERS, help="同時抓取的最大併發數")
    parser.add_argument("--dedupe-affiliated", action="store_true", help="合併部會與所屬機關重複發布的同標題新聞")
    parser.add_argument("--list-sources", action="store_true", help="列出目前支援的來源後結束")
    return parser.parse_args()


def normalize_selected_sources(selected_sources):
    from .scrapers.registry import SCRAPER_REGISTRY
    from .utils.text import clean_text

    if not selected_sources:
        return list(SCRAPER_REGISTRY.keys())

    normalized = []
    seen = set()
    unsupported = []
    for source in selected_sources:
        source = clean_text(source)
        if source in SCRAPER_REGISTRY:
            if source in seen:
                continue
            seen.add(source)
            normalized.append(source)
        else:
            unsupported.append(source)

    if unsupported:
        raise ValueError(
            "不支援的來源：{}\n可用來源如下：{}".format(
                "、".join(unsupported),
                "、".join(SCRAPER_REGISTRY.keys()),
            )
        )
    return normalized


def order_sources_for_scraping(source_names):
    from .config import SCRAPE_DIFFICULTY_ORDER, SOURCE_ORDER

    return sorted(
        source_names,
        key=lambda source_name: (
            SCRAPE_DIFFICULTY_ORDER.get(source_name, 50),
            SOURCE_ORDER.get(source_name, 999),
        ),
    )


def run_scraper(source_name, scraper_func, log_exception=True):
    started_at = time.perf_counter()
    try:
        items = scraper_func()
        elapsed = time.perf_counter() - started_at
        logger.info("%s 完成，抓到 %s 筆，用時 %.2f 秒", source_name, len(items), elapsed)
        return source_name, items, None
    except Exception as exc:
        elapsed = time.perf_counter() - started_at
        if log_exception:
            logger.exception("%s 失敗，用時 %.2f 秒", source_name, elapsed)
        else:
            logger.warning("%s 第一輪失敗，用時 %.2f 秒，將於本輪結束後重試：%s", source_name, elapsed, exc)
        return source_name, [], exc


def collect_news_for_sources_once(source_names, worker_count, print_failures=True, log_exceptions=True):
    from .scrapers.registry import SCRAPER_REGISTRY

    if not source_names:
        return [], []

    all_results = []
    failed_sources = []
    with ThreadPoolExecutor(max_workers=worker_count) as executor:
        future_map = {
            executor.submit(run_scraper, source_name, SCRAPER_REGISTRY[source_name], log_exceptions): source_name
            for source_name in source_names
        }
        for future in as_completed(future_map):
            source_name = future_map[future]
            try:
                _, items, error = future.result()
                all_results.extend(items)
                if error is not None:
                    failed_sources.append(source_name)
                    if print_failures:
                        print("{} 爬取失敗：{}".format(source_name, error))
            except Exception as exc:
                failed_sources.append(source_name)
                if log_exceptions:
                    logger.exception("%s future 執行失敗", source_name)
                else:
                    logger.warning("%s future 第一輪執行失敗，將於本輪結束後重試：%s", source_name, exc)
                if print_failures:
                    print("{} 爬取失敗：{}".format(source_name, exc))
    return all_results, list(dict.fromkeys(failed_sources))


def collect_all_this_week_news_concurrent(selected_sources=None, max_workers=None, dedupe_affiliated=False):
    from .config import FAILED_SOURCE_RETRY_TIMEOUT_EXTRA_SECONDS, MAX_WORKERS, SOURCE_ORDER
    from .http.client import set_retry_timeout_extra_seconds

    global LAST_FAILED_SOURCES

    source_names = normalize_selected_sources(selected_sources)
    scrape_source_names = order_sources_for_scraping(source_names)
    worker_count = max_workers if max_workers is not None else MAX_WORKERS
    worker_count = max(1, min(worker_count, len(scrape_source_names)))

    all_results, failed_sources = collect_news_for_sources_once(
        scrape_source_names,
        worker_count,
        print_failures=False,
        log_exceptions=False,
    )
    retry_results = []
    final_failed_sources = list(failed_sources)

    if failed_sources:
        retry_worker_count = max(1, min(worker_count, len(failed_sources)))
        print(
            "\n偵測到抓取失敗部會，將以額外增加 {} 秒 timeout 併發重抓一次：{}".format(
                FAILED_SOURCE_RETRY_TIMEOUT_EXTRA_SECONDS,
                "、".join(failed_sources),
            )
        )
        set_retry_timeout_extra_seconds(FAILED_SOURCE_RETRY_TIMEOUT_EXTRA_SECONDS)
        try:
            retry_results, retry_failed_sources = collect_news_for_sources_once(
                failed_sources,
                retry_worker_count,
                print_failures=True,
                log_exceptions=True,
            )
        finally:
            set_retry_timeout_extra_seconds(0)

        final_failed_sources = list(retry_failed_sources)
        all_results.extend(retry_results)

    LAST_FAILED_SOURCES = sorted(set(final_failed_sources), key=lambda name: SOURCE_ORDER.get(name, 999))

    if dedupe_affiliated:
        from .utils.dedupe import dedupe_affiliated_news

        deduped_results = dedupe_affiliated_news(all_results)
        if len(deduped_results) != len(all_results):
            logger.info("隸屬機關新聞去重完成：原始 %s 筆，去重後 %s 筆", len(all_results), len(deduped_results))
        all_results = deduped_results

    all_results.sort(
        key=lambda x: (
            SOURCE_ORDER.get(x["source"], 999),
            x["date"],
            x["title"],
        )
    )
    return all_results


def collect_all_this_week_news(selected_sources=None, max_workers=None, dedupe_affiliated=False):
    return collect_all_this_week_news_concurrent(
        selected_sources=selected_sources,
        max_workers=max_workers,
        dedupe_affiliated=dedupe_affiliated,
    )


def build_table_data(news_list):
    from .utils.dates import format_ad_and_roc_date

    return [
        {
            "編號": idx,
            "新聞日期": format_ad_and_roc_date(item["date"]),
            "部會": item["source"],
            "發布單位": item["department"],
            "新聞標題": item["title"],
            "新聞連結": item["link"],
        }
        for idx, item in enumerate(news_list, 1)
    ]


def print_table(news_list):
    table_rows = build_table_data(news_list)
    if LAST_FAILED_SOURCES:
        print("抓取失敗部會：{}".format("、".join(LAST_FAILED_SOURCES)))
    else:
        print("全部抓取成功")

    print()
    if not table_rows:
        print("本週沒有抓到任何新聞。")
        return

    print("{:<6}{:<28}{:<12}{:<24}{:<50}{}".format("編號", "新聞日期", "部會", "發布單位", "新聞標題", "新聞連結"))
    print("-" * 180)
    for row in table_rows:
        print(
            "{:<6}{:<28}{:<12}{:<24}{:<50}{}".format(
                row["編號"],
                row["新聞日期"],
                row["部會"],
                row["發布單位"][:22],
                row["新聞標題"][:48],
                row["新聞連結"],
            )
        )


def main():
    args = parse_args()

    try:
        validate_runtime_environment(
            selected_sources=args.sources,
            needs_excel_export=not args.list_sources,
            list_sources_only=args.list_sources,
        )
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 1

    from .config import PARSER
    from .excel_exporter import export_to_excel
    from .scrapers.registry import SCRAPER_REGISTRY
    from .utils.dates import get_this_week_range

    if args.list_sources:
        print("目前支援來源：")
        for source_name in SCRAPER_REGISTRY:
            print("- {}".format(source_name))
        return 0

    print("Python 執行檔：{}".format(sys.executable))
    print("Python 版本：{}".format(sys.version.replace("\n", " ")))

    try:
        selected_sources = normalize_selected_sources(args.sources)
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    start_of_week, end_of_week = get_this_week_range(force_refresh=True)

    print("本次抓取區間：{} 至 {}".format(start_of_week, end_of_week))
    print("本次抓取來源：{}".format("、".join(selected_sources)))

    logger.info(
        "開始執行多來源併發爬取，max_workers=%s, parser=%s, sources=%s",
        args.max_workers,
        PARSER,
        "、".join(selected_sources),
    )

    news = collect_all_this_week_news(
        selected_sources=selected_sources,
        max_workers=args.max_workers,
        dedupe_affiliated=args.dedupe_affiliated,
    )
    print_table(news)
    export_to_excel(news, output_dir=args.output_dir, dedupe_affiliated=args.dedupe_affiliated)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
