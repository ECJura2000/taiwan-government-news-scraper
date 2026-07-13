import argparse
import logging
import requests
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime
from pathlib import Path

if __package__ in (None, ""):
    # Allow `python news_scraper/main.py` and frozen entrypoints to resolve package imports.
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    __package__ = "news_scraper"

from .runtime import validate_runtime_environment

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
logger = logging.getLogger(__name__)


def parse_args():
    from .config import DEFAULT_OUTPUT_DIR, MAX_WORKERS

    parser = argparse.ArgumentParser(description="抓取本週各部會新聞並匯出 Excel。")
    parser.add_argument("--sources", nargs="+", help="只抓指定來源，例如：--sources 財政部 法務部")
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Excel 輸出資料夾")
    parser.add_argument("--max-workers", type=int, default=MAX_WORKERS, help="同時抓取的最大併發數")
    parser.add_argument("--dedupe-affiliated", action="store_true", help="合併部會與所屬機關重複發布的同標題新聞")
    parser.add_argument("--report-dir", help="JSON 執行報告輸出資料夾；預設為 Excel 輸出資料夾下的執行紀錄")
    parser.add_argument("--alert-webhook", help="異常時接收 JSON 告警的 webhook URL；也可使用 NEWS_SCRAPER_ALERT_WEBHOOK")
    parser.add_argument("--report-retention-days", type=int, default=180, help="執行報告保留天數，預設 180 天")
    parser.add_argument("--fail-on-source-error", action="store_true", help="任一來源重試後仍失敗時，以結束碼 3 結束")
    parser.add_argument("--list-sources", action="store_true", help="列出目前支援的來源後結束")
    return parser.parse_args()


def normalize_selected_sources(selected_sources):
    from .config import SOURCE_ALIASES
    from .scrapers.registry import SCRAPER_REGISTRY
    from .utils.text import clean_text

    if not selected_sources:
        return list(SCRAPER_REGISTRY.keys())

    normalized = []
    seen = set()
    unsupported = []
    for source in selected_sources:
        source = clean_text(source)
        source = SOURCE_ALIASES.get(source, source)
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


def order_sources_for_scraping(source_names, recent_reports=None, context=None):
    from .scheduler import prioritize_sources

    jobs = prioritize_sources(source_names, recent_reports=recent_reports)
    if context is not None:
        context.scheduling_plan = [
            {"position": position, "source": job.source, **job.reason}
            for position, job in enumerate(jobs, 1)
        ]
    return [job.source for job in jobs]


def run_scraper(source_name, scraper_func, log_exception=True, attempt=1, context=None):
    from .errors import NewsScraperError
    from .monitoring import RunContext, use_run_context
    from requests import RequestException
    from selenium.common.exceptions import TimeoutException as SeleniumTimeoutException, WebDriverException

    context = context or RunContext()
    started_at = time.perf_counter()
    with use_run_context(context):
        try:
            items = scraper_func()
            elapsed = time.perf_counter() - started_at
            context.record_source_attempt(source_name, attempt, len(items), elapsed)
            logger.info("%s 完成，抓到 %s 筆，用時 %.2f 秒", source_name, len(items), elapsed)
            return source_name, items, None
        except (NewsScraperError, RequestException, OSError, TimeoutError, SeleniumTimeoutException, WebDriverException) as exc:
            elapsed = time.perf_counter() - started_at
            context.record_source_attempt(source_name, attempt, 0, elapsed, error=exc)
            if log_exception:
                logger.exception("%s 失敗，用時 %.2f 秒", source_name, elapsed)
            else:
                logger.warning("%s 第一輪失敗，用時 %.2f 秒，將於本輪結束後重試：%s", source_name, elapsed, exc)
            return source_name, [], exc
        except (ValueError, KeyError, TypeError) as exc:
            # Parser/schema failures are recorded but intentionally not retried.
            elapsed = time.perf_counter() - started_at
            context.record_source_attempt(source_name, attempt, 0, elapsed, error=exc)
            return source_name, [], exc
        except Exception as exc:
            # A third-party scraper failure must never abort unrelated sources.
            elapsed = time.perf_counter() - started_at
            context.record_source_attempt(source_name, attempt, 0, elapsed, error=exc)
            if log_exception:
                logger.exception("%s 未預期失敗，用時 %.2f 秒", source_name, elapsed)
            else:
                logger.warning("%s 第一輪未預期失敗，用時 %.2f 秒，將於本輪結束後重試：%s", source_name, elapsed, exc)
            return source_name, [], exc


def collect_news_for_sources_once(source_names, worker_count, print_failures=True, log_exceptions=True, attempt=1, context=None):
    from .errors import NewsScraperError, is_retryable_error
    from .scrapers.registry import SCRAPER_REGISTRY
    from selenium.common.exceptions import TimeoutException as SeleniumTimeoutException, WebDriverException

    if not source_names:
        return [], []

    all_results = []
    failed_sources = []
    with ThreadPoolExecutor(max_workers=worker_count) as executor:
        future_map = {
            executor.submit(run_scraper, source_name, SCRAPER_REGISTRY[source_name], log_exceptions, attempt, context): source_name
            for source_name in source_names
        }
        for future in as_completed(future_map):
            source_name = future_map[future]
            try:
                _, items, error = future.result()
                all_results.extend(items)
                if error is not None:
                    if (
                        is_retryable_error(error)
                        or isinstance(
                            error,
                            (requests.RequestException, OSError, TimeoutError, SeleniumTimeoutException, WebDriverException),
                        )
                        or not isinstance(error, (NewsScraperError, ValueError, KeyError, TypeError))
                    ):
                        failed_sources.append(source_name)
                    elif context is not None:
                        context.failed_sources.append(source_name)
                    if print_failures:
                        print("{} 爬取失敗：{}".format(source_name, error))
            except (
                NewsScraperError,
                requests.RequestException,
                OSError,
                TimeoutError,
                SeleniumTimeoutException,
                WebDriverException,
                ValueError,
                KeyError,
                TypeError,
            ) as exc:
                if is_retryable_error(exc) or isinstance(
                    exc,
                    (requests.RequestException, OSError, TimeoutError, SeleniumTimeoutException, WebDriverException),
                ):
                    failed_sources.append(source_name)
                elif context is not None:
                    context.failed_sources.append(source_name)
                if log_exceptions:
                    logger.exception("%s future 執行失敗", source_name)
                else:
                    logger.warning("%s future 第一輪執行失敗，將於本輪結束後重試：%s", source_name, exc)
                if print_failures:
                    print("{} 爬取失敗：{}".format(source_name, exc))
            except Exception as exc:
                failed_sources.append(source_name)
                if log_exceptions:
                    logger.exception("%s future 未預期失敗", source_name)
                if print_failures:
                    print("{} 爬取失敗：{}".format(source_name, exc))
    return all_results, list(dict.fromkeys(failed_sources))


def collect_all_this_week_news_concurrent(
    selected_sources=None,
    max_workers=None,
    dedupe_affiliated=False,
    context=None,
    recent_reports=None,
):
    from .config import FAILED_SOURCE_RETRY_TIMEOUT_EXTRA_SECONDS, MAX_WORKERS, SOURCE_ORDER
    from .monitoring import RunContext

    context = context or RunContext()
    source_names = normalize_selected_sources(selected_sources)
    scrape_source_names = order_sources_for_scraping(source_names, recent_reports=recent_reports, context=context)
    worker_count = max_workers if max_workers is not None else MAX_WORKERS
    worker_count = max(1, min(worker_count, len(scrape_source_names)))

    all_results, failed_sources = collect_news_for_sources_once(
        scrape_source_names,
        worker_count,
        print_failures=False,
        log_exceptions=False,
        context=context,
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
        context.retry_timeout_extra_seconds = FAILED_SOURCE_RETRY_TIMEOUT_EXTRA_SECONDS
        try:
            retry_results, retry_failed_sources = collect_news_for_sources_once(
                failed_sources,
                retry_worker_count,
                print_failures=True,
                log_exceptions=True,
                attempt=2,
                context=context,
            )
        finally:
            context.retry_timeout_extra_seconds = 0

        final_failed_sources = list(retry_failed_sources)
        all_results.extend(retry_results)

    context.failed_sources = sorted(
        set(context.failed_sources) | set(final_failed_sources),
        key=lambda name: SOURCE_ORDER.get(name, 999),
    )

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


def collect_all_this_week_news(
    selected_sources=None,
    max_workers=None,
    dedupe_affiliated=False,
    context=None,
    recent_reports=None,
):
    return collect_all_this_week_news_concurrent(
        selected_sources=selected_sources,
        max_workers=max_workers,
        dedupe_affiliated=dedupe_affiliated,
        context=context,
        recent_reports=recent_reports,
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


def print_table(news_list, failed_sources=None):
    table_rows = build_table_data(news_list)
    if failed_sources:
        print("抓取失敗部會：{}".format("、".join(failed_sources)))
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
    run_started_at = datetime.now().astimezone()

    try:
        validate_runtime_environment(
            selected_sources=args.sources,
            needs_excel_export=not args.list_sources,
            list_sources_only=args.list_sources,
        )
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 1

    from .config import PARSER, SSL_FALLBACK_HOSTS
    from .excel_exporter import export_to_excel
    from .monitoring import (
        RunContext,
        build_alert_payload,
        build_run_report,
        build_trend_summary,
        detect_run_anomalies,
        load_recent_reports,
        prune_old_reports,
        send_webhook_alert,
        should_send_alert,
        write_json_file,
        write_run_report,
    )
    from .quality import process_news_quality
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

    report_dir = Path(args.report_dir) if args.report_dir else Path(args.output_dir) / "執行紀錄"
    recent_reports = load_recent_reports(report_dir)
    context = RunContext()
    news = collect_all_this_week_news(
        selected_sources=selected_sources,
        max_workers=args.max_workers,
        dedupe_affiliated=args.dedupe_affiliated,
        context=context,
        recent_reports=recent_reports,
    )
    news, context.quality_summary = process_news_quality(news, selected_sources)
    print_table(news, failed_sources=context.failed_sources)
    output_path = export_to_excel(news, output_dir=args.output_dir, dedupe_affiliated=args.dedupe_affiliated)

    run_finished_at = datetime.now().astimezone()
    detect_run_anomalies(context, selected_sources, recent_reports)
    report = build_run_report(
        context=context,
        started_at=run_started_at,
        finished_at=run_finished_at,
        selected_sources=selected_sources,
        news_count=len(news),
        output_path=output_path,
    )
    if should_send_alert(report):
        try:
            alert_result = send_webhook_alert(build_alert_payload(report), webhook_url=args.alert_webhook)
        except Exception as exc:
            alert_result = {"status": "failed", "error": str(exc)}
            logger.exception("異常告警傳送失敗")
        context.alerts.append(alert_result)
        report["alerts"] = list(context.alerts)

    report_path = write_run_report(report, report_dir)
    prune_old_reports(report_dir, retention_days=args.report_retention_days)
    trend_reports = [report] + recent_reports
    write_json_file(
        build_trend_summary(trend_reports[:52], allowed_ssl_hosts=SSL_FALLBACK_HOSTS),
        report_dir / "trend_summary.json",
    )
    logger.info("執行報告已輸出：%s", report_path)

    if args.fail_on_source_error and context.failed_sources:
        return 3
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
