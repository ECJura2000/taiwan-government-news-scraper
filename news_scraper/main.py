import argparse
import json
import logging
import sys
import time
from concurrent.futures import Future, ThreadPoolExecutor, as_completed
from pathlib import Path

if __package__ in (None, ""):
    # Allow `python news_scraper/main.py` and frozen entrypoints to resolve package imports.
    package_dir = str(Path(__file__).resolve().parent)
    sys.path[:] = [entry for entry in sys.path if str(Path(entry or ".").resolve()) != package_dir]
    sys.path.insert(0, str(Path(package_dir).parent))
    __package__ = "news_scraper"

import requests

from .runtime import validate_runtime_environment

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
logger = logging.getLogger(__name__)


def parse_args(argv=None):
    from .config import MAX_WORKERS
    from .paths import get_default_output_dir

    parser = argparse.ArgumentParser(description="抓取本週各部會新聞並匯出 Excel。")
    parser.add_argument("--sources", nargs="+", help="只抓指定來源，例如：--sources 財政部 法務部")
    parser.add_argument("--output-dir", default=str(get_default_output_dir()), help="Excel 輸出資料夾")
    parser.add_argument("--max-workers", type=int, default=MAX_WORKERS, help="同時抓取的最大併發數")
    parser.add_argument("--dedupe-affiliated", action="store_true", help="合併部會與所屬機關重複發布的同標題新聞")
    parser.add_argument("--report-dir", help="JSON 執行報告輸出資料夾；預設為 Excel 輸出資料夾下的執行紀錄")
    parser.add_argument("--alert-webhook", help="異常時接收 JSON 告警的 webhook URL；也可使用 NEWS_SCRAPER_ALERT_WEBHOOK")
    parser.add_argument("--report-retention-days", type=int, default=180, help="執行報告保留天數，預設 180 天")
    parser.add_argument("--fail-on-source-error", action="store_true", help="任一來源重試後仍失敗時，以結束碼 3 結束")
    parser.add_argument("--list-sources", action="store_true", help="列出目前支援的來源後結束")
    parser.add_argument("--headless", action="store_true", help="明確以無圖形介面模式執行")
    parser.add_argument("--json-summary", action="store_true", help="最後輸出一行機器可讀 JSON 摘要")
    parser.add_argument("--gui", action="store_true", help="開啟桌面圖形介面")
    return parser.parse_args(argv)


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


def collect_news_for_sources_once(
    source_names,
    worker_count,
    print_failures=True,
    log_exceptions=True,
    attempt=1,
    context=None,
    cancel_event=None,
    progress_callback=None,
):
    from .errors import NewsScraperError, is_retryable_error
    from .scrapers.registry import SCRAPER_REGISTRY
    from selenium.common.exceptions import TimeoutException as SeleniumTimeoutException, WebDriverException

    if not source_names:
        return [], []

    all_results = []
    failed_sources = []
    completed_count = 0
    source_iterator = iter(source_names)

    def submit_next(executor, future_map):
        if cancel_event is not None and cancel_event.is_set():
            return False
        try:
            source_name = next(source_iterator)
        except StopIteration:
            return False
        future = executor.submit(
            run_scraper,
            source_name,
            SCRAPER_REGISTRY[source_name],
            log_exceptions,
            attempt,
            context,
        )
        future_map[future] = source_name
        return True

    with ThreadPoolExecutor(max_workers=worker_count) as executor:
        future_map: dict[Future, str] = {}
        for _ in range(worker_count):
            if not submit_next(executor, future_map):
                break

        while future_map:
            future = next(as_completed(tuple(future_map)))
            source_name = future_map.pop(future)
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
            finally:
                completed_count += 1
                if progress_callback is not None:
                    try:
                        progress_callback(source_name, completed_count, len(source_names), attempt)
                    except Exception:
                        logger.exception("進度回呼失敗")
                submit_next(executor, future_map)

    if cancel_event is not None and cancel_event.is_set() and context is not None:
        context.cancelled = True
    return all_results, list(dict.fromkeys(failed_sources))


def collect_all_this_week_news_concurrent(
    selected_sources=None,
    max_workers=None,
    dedupe_affiliated=False,
    context=None,
    recent_reports=None,
    cancel_event=None,
    progress_callback=None,
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
        cancel_event=cancel_event,
        progress_callback=progress_callback,
    )
    retry_results = []
    final_failed_sources = list(failed_sources)

    if failed_sources and not context.cancelled:
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
                cancel_event=cancel_event,
                progress_callback=progress_callback,
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
    cancel_event=None,
    progress_callback=None,
):
    return collect_all_this_week_news_concurrent(
        selected_sources=selected_sources,
        max_workers=max_workers,
        dedupe_affiliated=dedupe_affiliated,
        context=context,
        recent_reports=recent_reports,
        cancel_event=cancel_event,
        progress_callback=progress_callback,
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


def print_json_error_summary(status, message):
    print(
        json.dumps(
            {
                "status": status,
                "output_file": "",
                "report_file": "",
                "news_count": 0,
                "failed_sources": [],
                "anomalies": [],
                "quality": {},
                "insecure_ssl_hosts": [],
                "cancelled": False,
                "error": message,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    )


def main(argv=None):
    args = parse_args(argv)

    if args.gui:
        from .gui import main as gui_main

        return gui_main()

    from .config import PARSER
    from .scrapers.registry import SCRAPER_REGISTRY
    from .utils.dates import get_this_week_range

    if args.list_sources:
        try:
            validate_runtime_environment(list_sources_only=True, needs_excel_export=False)
        except RuntimeError as exc:
            print(str(exc), file=sys.stderr)
            return 1
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

    from .application import RunOptions, run_news_scraper
    from .run_lock import RunAlreadyActiveError

    try:
        result = run_news_scraper(
            RunOptions(
                sources=tuple(selected_sources),
                output_dir=Path(args.output_dir),
                report_dir=Path(args.report_dir) if args.report_dir else None,
                max_workers=args.max_workers,
                dedupe_affiliated=args.dedupe_affiliated,
                report_retention_days=args.report_retention_days,
                fail_on_source_error=args.fail_on_source_error,
                alert_webhook=args.alert_webhook,
                mode="headless",
            )
        )
    except RunAlreadyActiveError as exc:
        print(str(exc), file=sys.stderr)
        if args.json_summary:
            print_json_error_summary("locked", str(exc))
        return 4
    except (OSError, RuntimeError) as exc:
        message = "新聞整理無法完成：{}".format(exc)
        print(message, file=sys.stderr)
        if args.json_summary:
            print_json_error_summary("failed", message)
        return 1

    print_table(result.news_items, failed_sources=result.failed_sources)
    if result.report_path:
        logger.info("執行報告已輸出：%s", result.report_path)
    if args.json_summary:
        print(json.dumps(result.to_summary(), ensure_ascii=False, separators=(",", ":")))

    if result.cancelled:
        return 130
    if args.fail_on_source_error and result.failed_sources:
        return 3
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
