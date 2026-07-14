import json
import logging
import os
import queue
import subprocess
import sys
import threading
from dataclasses import asdict, dataclass, field
from pathlib import Path

from .application import ProgressEvent, RunOptions, RunResult, run_news_scraper
from .io_utils import atomic_write_text
from .paths import get_default_output_dir, prepare_workspace
from .run_lock import RunAlreadyActiveError


@dataclass
class GuiSettings:
    schema_version: int = 1
    sources: list[str] = field(default_factory=list)
    output_dir: str = ""
    max_workers: int = 8
    dedupe_affiliated: bool = False
    fail_on_source_error: bool = False
    report_retention_days: int = 180


def load_settings(path: str | Path, available_sources: list[str]) -> GuiSettings:
    settings_path = Path(path)
    try:
        data = json.loads(settings_path.read_text(encoding="utf-8"))
        settings = GuiSettings(**{key: data[key] for key in asdict(GuiSettings()) if key in data})
    except (OSError, TypeError, ValueError):
        settings = GuiSettings()
    settings.sources = [source for source in settings.sources if source in available_sources]
    if not settings.sources:
        settings.sources = list(available_sources)
    if not settings.output_dir:
        settings.output_dir = str(get_default_output_dir())
    try:
        settings.max_workers = max(1, min(32, int(settings.max_workers)))
        settings.report_retention_days = max(1, min(3650, int(settings.report_retention_days)))
    except (TypeError, ValueError):
        settings.max_workers = 8
        settings.report_retention_days = 180
    return settings


def save_settings(path: str | Path, settings: GuiSettings) -> Path:
    return atomic_write_text(
        path,
        json.dumps(asdict(settings), ensure_ascii=False, indent=2),
    )


def open_path(path: str | Path) -> None:
    target = str(Path(path).resolve())
    if sys.platform == "win32":
        os.startfile(target)  # type: ignore[attr-defined]
    elif sys.platform == "darwin":
        subprocess.Popen(["open", target])
    else:
        subprocess.Popen(["xdg-open", target])


def hide_windows_console() -> None:
    if sys.platform != "win32" or not getattr(sys, "frozen", False):
        return
    try:
        import ctypes

        console_window = ctypes.windll.kernel32.GetConsoleWindow()
        if console_window:
            ctypes.windll.user32.ShowWindow(console_window, 0)
    except (AttributeError, OSError):
        return


class QueueLogHandler(logging.Handler):
    def __init__(self, events: queue.Queue):
        super().__init__()
        self.events = events
        self.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(name)s: %(message)s", "%H:%M:%S"))

    def emit(self, record):
        try:
            self.events.put(("log", self.format(record)))
        except Exception:
            self.handleError(record)


class NewsScraperApp:
    def __init__(self, root):
        import tkinter as tk
        from tkinter import ttk

        from .config import MAX_WORKERS
        from .scrapers.registry import SCRAPER_REGISTRY

        self.tk = tk
        self.ttk = ttk
        self.root = root
        self.root.title("各機關新聞整理")
        self.root.geometry("1040x720")
        self.root.minsize(860, 620)

        self.workspace = prepare_workspace()
        self.settings_path = self.workspace.program_data / "settings.json"
        self.all_sources = list(SCRAPER_REGISTRY)
        self.settings = load_settings(self.settings_path, self.all_sources)
        if self.settings.max_workers == 8:
            self.settings.max_workers = MAX_WORKERS

        self.selected_sources = set(self.settings.sources)
        self.visible_sources: list[str] = []
        self.events: queue.Queue = queue.Queue()
        self.cancel_event = threading.Event()
        self.worker: threading.Thread | None = None
        self.result: RunResult | None = None
        self.close_requested = False

        self.search_var = tk.StringVar()
        self.output_var = tk.StringVar(value=self.settings.output_dir)
        self.workers_var = tk.IntVar(value=self.settings.max_workers)
        self.retention_var = tk.IntVar(value=self.settings.report_retention_days)
        self.dedupe_var = tk.BooleanVar(value=self.settings.dedupe_affiliated)
        self.strict_var = tk.BooleanVar(value=self.settings.fail_on_source_error)
        self.status_var = tk.StringVar(value="準備完成")
        self.progress_var = tk.DoubleVar(value=0)
        self.selected_count_var = tk.StringVar()

        self._configure_style()
        self._build_ui()
        self._rebuild_source_list()
        self.search_var.trace_add("write", self._on_search_changed)
        self.root.protocol("WM_DELETE_WINDOW", self._on_close)
        self.root.after(100, self._poll_events)

    def _configure_style(self):
        style = self.ttk.Style(self.root)
        for theme in ("vista", "aqua", "clam"):
            if theme in style.theme_names():
                try:
                    style.theme_use(theme)
                    break
                except self.tk.TclError:
                    continue
        style.configure("Title.TLabel", font=("TkDefaultFont", 20, "bold"))
        style.configure("Subtitle.TLabel", foreground="#455a64")
        style.configure("Primary.TButton", font=("TkDefaultFont", 11, "bold"), padding=(18, 10))
        style.configure("Danger.TButton", padding=(14, 10))

    def _build_ui(self):
        from tkinter import filedialog

        self.filedialog = filedialog
        container = self.ttk.Frame(self.root, padding=20)
        container.pack(fill="both", expand=True)

        self.ttk.Label(container, text="各機關新聞整理", style="Title.TLabel").pack(anchor="w")
        self.ttk.Label(
            container,
            text="選擇來源後按一次開始；Excel 與 JSON 報告會分開保存。",
            style="Subtitle.TLabel",
        ).pack(anchor="w", pady=(2, 16))

        body = self.ttk.Panedwindow(container, orient="horizontal")
        body.pack(fill="both", expand=True)
        source_frame = self.ttk.Labelframe(body, text="新聞來源", padding=10)
        options_frame = self.ttk.Labelframe(body, text="執行設定", padding=12)
        body.add(source_frame, weight=2)
        body.add(options_frame, weight=3)

        search = self.ttk.Entry(source_frame, textvariable=self.search_var)
        search.pack(fill="x", pady=(0, 8))
        search.bind("<Escape>", lambda _event: self.search_var.set(""))

        list_frame = self.ttk.Frame(source_frame)
        list_frame.pack(fill="both", expand=True)
        self.source_list = self.tk.Listbox(
            list_frame,
            selectmode="extended",
            exportselection=False,
            activestyle="dotbox",
        )
        scrollbar = self.ttk.Scrollbar(list_frame, orient="vertical", command=self.source_list.yview)
        self.source_list.configure(yscrollcommand=scrollbar.set)
        self.source_list.pack(side="left", fill="both", expand=True)
        scrollbar.pack(side="right", fill="y")
        self.source_list.bind("<<ListboxSelect>>", self._on_source_selection)

        source_actions = self.ttk.Frame(source_frame)
        source_actions.pack(fill="x", pady=(8, 0))
        self.ttk.Button(source_actions, text="全選", command=self._select_all).pack(side="left")
        self.ttk.Button(source_actions, text="清除", command=self._clear_all).pack(side="left", padx=6)
        self.ttk.Label(source_actions, textvariable=self.selected_count_var).pack(side="right")

        self.ttk.Label(options_frame, text="輸出資料夾").grid(row=0, column=0, sticky="w")
        output_row = self.ttk.Frame(options_frame)
        output_row.grid(row=1, column=0, columnspan=2, sticky="ew", pady=(4, 14))
        self.ttk.Entry(output_row, textvariable=self.output_var).pack(side="left", fill="x", expand=True)
        self.ttk.Button(output_row, text="選擇...", command=self._choose_output).pack(side="left", padx=(8, 0))

        self.ttk.Label(options_frame, text="同時抓取數量").grid(row=2, column=0, sticky="w", pady=5)
        self.ttk.Spinbox(options_frame, from_=1, to=32, textvariable=self.workers_var, width=8).grid(
            row=2, column=1, sticky="e", pady=5
        )
        self.ttk.Label(options_frame, text="報告保留天數").grid(row=3, column=0, sticky="w", pady=5)
        self.ttk.Spinbox(options_frame, from_=1, to=3650, textvariable=self.retention_var, width=8).grid(
            row=3, column=1, sticky="e", pady=5
        )
        self.ttk.Checkbutton(
            options_frame,
            text="合併部會與所屬機關的同標題新聞",
            variable=self.dedupe_var,
        ).grid(row=4, column=0, columnspan=2, sticky="w", pady=(12, 4))
        self.ttk.Checkbutton(
            options_frame,
            text="任何來源失敗時使用嚴格失敗狀態",
            variable=self.strict_var,
        ).grid(row=5, column=0, columnspan=2, sticky="w", pady=4)

        self.progress = self.ttk.Progressbar(options_frame, variable=self.progress_var, maximum=100)
        self.progress.grid(row=6, column=0, columnspan=2, sticky="ew", pady=(18, 5))
        self.ttk.Label(options_frame, textvariable=self.status_var).grid(row=7, column=0, columnspan=2, sticky="w")

        log_frame = self.ttk.Labelframe(options_frame, text="執行紀錄", padding=6)
        log_frame.grid(row=8, column=0, columnspan=2, sticky="nsew", pady=(12, 10))
        self.log_text = self.tk.Text(log_frame, height=12, wrap="word", state="disabled")
        self.log_text.pack(fill="both", expand=True)

        action_row = self.ttk.Frame(options_frame)
        action_row.grid(row=9, column=0, columnspan=2, sticky="ew")
        self.start_button = self.ttk.Button(
            action_row,
            text="開始整理新聞",
            style="Primary.TButton",
            command=self._start,
        )
        self.start_button.pack(side="left")
        self.cancel_button = self.ttk.Button(
            action_row,
            text="取消",
            style="Danger.TButton",
            command=self._cancel,
            state="disabled",
        )
        self.cancel_button.pack(side="left", padx=8)
        self.open_excel_button = self.ttk.Button(action_row, text="開啟 Excel", command=self._open_excel, state="disabled")
        self.open_excel_button.pack(side="right")
        self.open_folder_button = self.ttk.Button(
            action_row,
            text="開啟資料夾",
            command=self._open_folder,
            state="disabled",
        )
        self.open_folder_button.pack(side="right", padx=8)

        options_frame.columnconfigure(0, weight=1)
        options_frame.rowconfigure(8, weight=1)

    def _on_search_changed(self, *_args):
        self._sync_selected_from_view()
        self._rebuild_source_list()

    def _sync_selected_from_view(self):
        selected_indexes = set(self.source_list.curselection()) if hasattr(self, "source_list") else set()
        for index, source in enumerate(self.visible_sources):
            if index in selected_indexes:
                self.selected_sources.add(source)
            else:
                self.selected_sources.discard(source)
        self._update_selected_count()

    def _on_source_selection(self, _event=None):
        self._sync_selected_from_view()

    def _rebuild_source_list(self):
        query = self.search_var.get().strip().casefold()
        self.visible_sources = [source for source in self.all_sources if query in source.casefold()]
        self.source_list.delete(0, "end")
        for index, source in enumerate(self.visible_sources):
            self.source_list.insert("end", source)
            if source in self.selected_sources:
                self.source_list.selection_set(index)
        self._update_selected_count()

    def _update_selected_count(self):
        self.selected_count_var.set("已選 {} / {}".format(len(self.selected_sources), len(self.all_sources)))

    def _select_all(self):
        self.selected_sources.update(self.all_sources)
        self._rebuild_source_list()

    def _clear_all(self):
        self.selected_sources.clear()
        self._rebuild_source_list()

    def _choose_output(self):
        selected = self.filedialog.askdirectory(initialdir=self.output_var.get() or str(Path.home()))
        if selected:
            self.output_var.set(selected)

    def _append_log(self, message: str):
        self.log_text.configure(state="normal")
        self.log_text.insert("end", message.rstrip() + "\n")
        self.log_text.see("end")
        self.log_text.configure(state="disabled")

    def _validated_settings(self) -> GuiSettings:
        from tkinter import messagebox

        self._sync_selected_from_view()
        if not self.selected_sources:
            messagebox.showerror("缺少來源", "請至少選擇一個新聞來源。")
            raise ValueError("沒有選擇來源")
        output_dir = Path(self.output_var.get()).expanduser()
        try:
            output_dir.mkdir(parents=True, exist_ok=True)
        except OSError as exc:
            messagebox.showerror("輸出位置無法使用", str(exc))
            raise ValueError("輸出位置無法使用") from exc
        try:
            workers = max(1, min(32, int(self.workers_var.get())))
            retention = max(1, min(3650, int(self.retention_var.get())))
        except (TypeError, ValueError, self.tk.TclError) as exc:
            messagebox.showerror("設定錯誤", "同時抓取數量與保留天數必須是整數。")
            raise ValueError("數值設定錯誤") from exc
        return GuiSettings(
            sources=[source for source in self.all_sources if source in self.selected_sources],
            output_dir=str(output_dir.resolve()),
            max_workers=workers,
            dedupe_affiliated=bool(self.dedupe_var.get()),
            fail_on_source_error=bool(self.strict_var.get()),
            report_retention_days=retention,
        )

    def _start(self):
        if self.worker and self.worker.is_alive():
            return
        try:
            settings = self._validated_settings()
        except ValueError:
            return
        save_settings(self.settings_path, settings)
        self.settings = settings
        self.cancel_event = threading.Event()
        self.result = None
        self.progress_var.set(0)
        self.status_var.set("正在啟動...")
        self.start_button.configure(state="disabled")
        self.cancel_button.configure(state="normal")
        self.open_excel_button.configure(state="disabled")
        self.open_folder_button.configure(state="disabled")
        self._append_log("開始整理 {} 個來源。".format(len(settings.sources)))
        self.worker = threading.Thread(target=self._run_worker, args=(settings,), daemon=True)
        self.worker.start()

    def _run_worker(self, settings: GuiSettings):
        handler = QueueLogHandler(self.events)
        root_logger = logging.getLogger()
        root_logger.addHandler(handler)
        try:
            result = run_news_scraper(
                RunOptions(
                    sources=tuple(settings.sources),
                    output_dir=Path(settings.output_dir),
                    max_workers=settings.max_workers,
                    dedupe_affiliated=settings.dedupe_affiliated,
                    report_retention_days=settings.report_retention_days,
                    fail_on_source_error=settings.fail_on_source_error,
                    mode="gui",
                ),
                cancel_event=self.cancel_event,
                progress_callback=lambda event: self.events.put(("progress", event)),
            )
        except RunAlreadyActiveError as exc:
            self.events.put(("error", str(exc)))
        except Exception as exc:
            logging.getLogger(__name__).exception("GUI 新聞整理失敗")
            self.events.put(("error", "{}: {}".format(type(exc).__name__, exc)))
        else:
            self.events.put(("result", result))
        finally:
            root_logger.removeHandler(handler)
            self.events.put(("worker_done", None))

    def _handle_progress(self, event: ProgressEvent):
        self.status_var.set(event.message)
        self._append_log(event.message)
        if event.total:
            self.progress_var.set(min(100, event.completed / event.total * 100))

    def _poll_events(self):
        from tkinter import messagebox

        try:
            while True:
                kind, payload = self.events.get_nowait()
                if kind == "log":
                    self._append_log(payload)
                elif kind == "progress":
                    self._handle_progress(payload)
                elif kind == "result":
                    self.result = payload
                    if payload.cancelled:
                        self.status_var.set("已取消")
                    elif payload.status == "success":
                        self.status_var.set("完成：共 {} 筆".format(payload.news_count))
                    else:
                        self.status_var.set("完成，但有需要注意的項目")
                    if payload.output_path:
                        self.open_excel_button.configure(state="normal")
                    self.open_folder_button.configure(state="normal")
                elif kind == "error":
                    self.status_var.set("執行失敗")
                    self._append_log(payload)
                    messagebox.showerror("新聞整理失敗", payload)
                elif kind == "worker_done":
                    self.start_button.configure(state="normal")
                    self.cancel_button.configure(state="disabled")
                    if self.close_requested:
                        self.root.destroy()
                        return
        except queue.Empty:
            pass
        self.root.after(100, self._poll_events)

    def _cancel(self):
        self.cancel_event.set()
        self.cancel_button.configure(state="disabled")
        self.status_var.set("正在取消；等待目前的網路請求結束...")
        self._append_log("已要求取消，不再派發新來源。")

    def _open_excel(self):
        if self.result and self.result.output_path:
            open_path(self.result.output_path)

    def _open_folder(self):
        if self.result and self.result.output_path:
            open_path(self.result.output_path.parent)
        else:
            open_path(self.output_var.get())

    def _on_close(self):
        from tkinter import messagebox

        if self.worker and self.worker.is_alive():
            if not messagebox.askyesno("仍在執行", "要取消目前工作並在安全停止後關閉嗎？"):
                return
            self.close_requested = True
            self._cancel()
            return
        self.root.destroy()


def main(smoke_test: bool = False) -> int:
    hide_windows_console()
    try:
        import tkinter as tk
        from tkinter import messagebox
    except ImportError:
        print("此 Python 環境缺少 tkinter，無法開啟圖形介面。", file=sys.stderr)
        return 1

    try:
        root = tk.Tk()
    except tk.TclError as exc:
        print("無法開啟圖形介面：{}".format(exc), file=sys.stderr)
        return 1
    try:
        NewsScraperApp(root)
        if smoke_test:
            root.after(250, root.destroy)
        root.mainloop()
    except Exception as exc:
        messagebox.showerror("程式錯誤", "{}: {}".format(type(exc).__name__, exc))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
