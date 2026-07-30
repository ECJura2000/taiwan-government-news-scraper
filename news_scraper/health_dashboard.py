from __future__ import annotations

from typing import Any

from .monitoring import build_trend_summary


def _diagnostic_for_source(report: dict, source: str) -> dict | None:
    selected = report.get("selected_sources")
    if isinstance(selected, list) and source not in selected:
        return None
    diagnostics = report.get("source_diagnostics")
    if isinstance(diagnostics, list):
        for diagnostic in diagnostics:
            if diagnostic.get("source") == source:
                return diagnostic
    attempts = [item for item in report.get("source_attempts", []) if item.get("source") == source]
    final = attempts[-1] if attempts else {}
    return {
        "source": source,
        "status": "failed" if source in report.get("failed_sources", []) else "success",
        "item_count": report.get("quality", {}).get("source_counts", {}).get(source, final.get("item_count", 0)),
        "elapsed_seconds": sum(float(item.get("elapsed_seconds") or 0) for item in attempts),
        "failure_class": final.get("failure_class", ""),
        "error_category": final.get("error_category", ""),
        "final_route": {},
    }


def build_health_dashboard_model(reports: list[dict], sources: list[str]) -> dict[str, Any]:
    ordered_reports = list(reports[:52])
    latest = ordered_reports[0] if ordered_reports else {}
    trend = build_trend_summary(ordered_reports)
    source_rows = []
    timelines: dict[str, list[dict]] = {}
    for source in sources:
        stats = trend["sources"].get(source, {})
        source_timeline = []
        ssl_used = False
        for report in ordered_reports[:12]:
            diagnostic = _diagnostic_for_source(report, source)
            if diagnostic is None:
                continue
            route_host = str((diagnostic.get("final_route") or {}).get("url_host") or "")
            ssl_used = ssl_used or route_host in set(report.get("insecure_ssl_hosts", []))
            source_timeline.append(
                {
                    "finished_at": str(report.get("finished_at") or ""),
                    "status": diagnostic.get("status", ""),
                    "item_count": int(diagnostic.get("item_count") or 0),
                    "elapsed_seconds": float(diagnostic.get("elapsed_seconds") or 0),
                    "failure_class": str(diagnostic.get("failure_class") or ""),
                    "error_category": str(diagnostic.get("error_category") or ""),
                    "route_id": str((diagnostic.get("final_route") or {}).get("route_id") or ""),
                    "unstable": bool(diagnostic.get("unstable")),
                    "failure_evidence": dict(diagnostic.get("failure_evidence") or {}),
                }
            )
        timelines[source] = source_timeline
        consecutive_zero_items = 0
        for item in source_timeline:
            if item["status"] != "success" or item["item_count"] != 0:
                break
            consecutive_zero_items += 1
        latest_failure = next(
            (item for item in source_timeline if item["status"] == "failed"),
            None,
        )
        source_rows.append(
            {
                "source": source,
                "success_rate": stats.get("success_rate"),
                "average_elapsed_seconds": stats.get("average_elapsed_seconds"),
                "latest_elapsed_seconds": (
                    source_timeline[0]["elapsed_seconds"] if source_timeline else None
                ),
                "consecutive_zero_items": consecutive_zero_items,
                "failure_count": stats.get("failures", 0),
                "unstable_runs": sum(bool(item["unstable"]) for item in source_timeline),
                "last_failure_class": stats.get("last_failure_class", ""),
                "last_failure_at": (
                    latest_failure["finished_at"] if latest_failure else ""
                ),
                "last_route": stats.get("last_route", ""),
                "fallback_uses": stats.get("fallback_uses", 0),
                "ssl_used": ssl_used,
            }
        )
    health = latest.get("source_health") or {}
    return {
        "report_count": len(ordered_reports),
        "latest_status": str(latest.get("status") or ""),
        "latest_finished_at": str(latest.get("finished_at") or ""),
        "healthy_count": int(health.get("healthy_count") or 0),
        "failed_count": int(health.get("failed_count") or len(latest.get("failed_sources", []))),
        "unstable_count": int(health.get("unstable_count") or 0),
        "fallback_source_count": int(health.get("fallback_source_count") or 0),
        "ssl_fallback_host_count": int(
            health.get("ssl_fallback_host_count") or len(latest.get("insecure_ssl_hosts", []))
        ),
        "source_rows": source_rows,
        "timelines": timelines,
    }


def open_health_dashboard(parent, model: dict) -> Any:
    import tkinter as tk
    from tkinter import ttk

    window = tk.Toplevel(parent)
    window.title("來源健康")
    typography = getattr(parent, "news_scraper_typography", None)
    if typography is not None:
        typography.register_window(window, width=1180, height=720, min_width=900, min_height=600)
    container = ttk.Frame(window, padding=14)
    container.pack(fill="both", expand=True)
    if not model["report_count"]:
        ttk.Label(container, text="目前沒有執行紀錄。完成一次新聞整理後即可查看來源趨勢。").pack(
            anchor="w", pady=20
        )
        return window

    summary_text = (
        "最近狀態：{latest_status}　健康：{healthy_count}　失敗：{failed_count}　"
        "不穩定：{unstable_count}　備援：{fallback_source_count}　SSL 降級：{ssl_fallback_host_count}"
    ).format(**model)
    ttk.Label(container, text=summary_text, style="Subtitle.TLabel").pack(anchor="w", pady=(0, 10))

    body = ttk.Panedwindow(container, orient="vertical")
    body.pack(fill="both", expand=True)
    table_frame = ttk.Frame(body)
    detail_frame = ttk.Labelframe(body, text="最近 12 次執行", padding=8)
    body.add(table_frame, weight=3)
    body.add(detail_frame, weight=2)
    columns = (
        "source",
        "rate",
        "average",
        "latest",
        "failures",
        "unstable",
        "fallback",
        "zero",
        "failure",
        "failure_at",
        "route",
        "ssl",
    )
    tree = ttk.Treeview(table_frame, columns=columns, show="headings", selectmode="browse")
    labels = {
        "source": "來源",
        "rate": "成功率",
        "average": "平均秒數",
        "latest": "最近秒數",
        "failures": "失敗數",
        "unstable": "不穩定數",
        "fallback": "備援數",
        "zero": "連續零筆",
        "failure": "最近失敗分類",
        "failure_at": "最近失敗時間",
        "route": "最近入口",
        "ssl": "SSL 降級",
    }
    for column in columns:
        tree.heading(column, text=labels[column])
        tree.column(
            column,
            width=150 if column in {"source", "failure", "failure_at", "route"} else 84,
            anchor="center",
        )
    scrollbar = ttk.Scrollbar(table_frame, orient="vertical", command=tree.yview)
    horizontal = ttk.Scrollbar(table_frame, orient="horizontal", command=tree.xview)
    tree.configure(yscrollcommand=scrollbar.set, xscrollcommand=horizontal.set)
    tree.grid(row=0, column=0, sticky="nsew")
    scrollbar.grid(row=0, column=1, sticky="ns")
    horizontal.grid(row=1, column=0, sticky="ew")
    table_frame.columnconfigure(0, weight=1)
    table_frame.rowconfigure(0, weight=1)
    for row in model["source_rows"]:
        rate = "無資料" if row["success_rate"] is None else "{:.0%}".format(row["success_rate"])
        average = "無資料" if row["average_elapsed_seconds"] is None else "{:.1f}".format(
            row["average_elapsed_seconds"]
        )
        latest_elapsed = (
            "無資料"
            if row["latest_elapsed_seconds"] is None
            else "{:.1f}".format(row["latest_elapsed_seconds"])
        )
        tree.insert(
            "",
            "end",
            iid=row["source"],
            values=(
                row["source"],
                rate,
                average,
                latest_elapsed,
                row["failure_count"],
                row["unstable_runs"],
                row["fallback_uses"],
                row["consecutive_zero_items"],
                row["last_failure_class"] or "無",
                row["last_failure_at"][:19] or "無",
                row["last_route"] or "無資料",
                "是" if row["ssl_used"] else "否",
            ),
        )

    canvas = tk.Canvas(detail_frame, height=170, background="#FFFFFF", highlightthickness=1)
    canvas.pack(fill="both", expand=True)

    def draw_timeline(source):
        canvas.delete("all")
        timeline = model["timelines"].get(source, [])
        if not timeline:
            canvas.create_text(20, 20, anchor="nw", text="此來源沒有歷史資料", font="NewsScraperCJK")
            return
        width = max(700, canvas.winfo_width())
        step = max(70, (width - 40) // max(1, len(timeline)))
        for index, item in enumerate(timeline):
            x = 30 + index * step
            color = "#2E7D32" if item["status"] == "success" else "#C62828"
            canvas.create_oval(x, 18, x + 18, 36, fill=color, outline=color)
            evidence = item.get("failure_evidence") or {}
            evidence_label = "{}/{}".format(
                evidence.get("url_host") or "無主機",
                evidence.get("http_status") or "無狀態碼",
            )
            label = "{}\n{} 筆／{:.1f} 秒\n{}\n{}\n{}".format(
                item["finished_at"][:10] or "無日期",
                item["item_count"],
                item["elapsed_seconds"],
                item["route_id"] or "無入口資料",
                item["failure_class"] or "正常",
                evidence_label if item["status"] == "failed" else "",
            )
            canvas.create_text(x + 9, 44, anchor="n", text=label, font="NewsScraperCJK", justify="center")

    def on_select(_event=None):
        selected = tree.selection()
        if selected:
            draw_timeline(selected[0])

    tree.bind("<<TreeviewSelect>>", on_select)
    if model["source_rows"]:
        first = model["source_rows"][0]["source"]
        tree.selection_set(first)
        window.after(50, lambda: draw_timeline(first))
    return window
