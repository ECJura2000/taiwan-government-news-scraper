import copy
from pathlib import Path
from typing import Any

from .relevance import (
    ExclusionRule,
    KeywordRule,
    RelevanceProfile,
    TopicRule,
    build_default_relevance_profile,
    classify_relevance,
    compare_relevance_profiles,
    delete_default_aware,
    get_relevance_profile_summary,
    load_relevance_profile,
    merge_new_default_rules,
    new_custom_id,
    save_relevance_profile,
    validate_relevance_profile,
)


def _topic_label(topic: TopicRule) -> str:
    return "{}{}".format("" if topic.enabled else "（停用）", topic.name)


class RelevanceProfileEditor:
    def __init__(
        self,
        parent,
        *,
        profile: RelevanceProfile,
        profile_path: Path,
        available_sources: list[str],
        on_saved,
    ):
        import tkinter as tk
        from tkinter import filedialog, messagebox, simpledialog, ttk

        self.tk = tk
        self.ttk = ttk
        self.filedialog = filedialog
        self.messagebox = messagebox
        self.simpledialog = simpledialog
        self.profile = profile.clone()
        self.profile_path = Path(profile_path)
        self.available_sources = list(available_sources)
        self.on_saved = on_saved
        self.deleted_stack: list[tuple] = []
        self.imported_profile = False
        self.dragged_topic_id = ""

        self.window = tk.Toplevel(parent)
        self.window.title("主題與關鍵字")
        typography = getattr(parent, "news_scraper_typography", None)
        if typography is not None:
            typography.register_window(
                self.window,
                width=1120,
                height=760,
                min_width=940,
                min_height=650,
            )
        else:
            self.window.geometry("1120x760")
            self.window.minsize(940, 650)
        self.window.transient(parent)
        self.window.protocol("WM_DELETE_WINDOW", self._cancel)

        self.name_var = tk.StringVar(value=self.profile.name)
        self.fallback_var = tk.BooleanVar(
            value=self.profile.include_unassigned_context_matches
        )
        self.summary_var = tk.StringVar()
        self.keyword_search_var = tk.StringVar()
        self.keyword_topic_filter_var = tk.StringVar(value="全部主題")
        self.keyword_type_filter_var = tk.StringVar(value="全部類型")
        self.keyword_status_filter_var = tk.StringVar(value="全部狀態")
        self.exclusion_search_var = tk.StringVar()
        self.exclusion_topic_filter_var = tk.StringVar(value="全部範圍")
        self.exclusion_status_filter_var = tk.StringVar(value="全部狀態")
        self.test_title_var = tk.StringVar()
        self.test_source_var = tk.StringVar()
        self.test_result_var = tk.StringVar(value="尚未測試")

        self._build_ui()
        self._refresh_all()
        self.keyword_search_var.trace_add("write", lambda *_args: self._refresh_keywords())
        self.exclusion_search_var.trace_add(
            "write",
            lambda *_args: self._refresh_exclusions(),
        )
        self.window.grab_set()
        self.window.focus_set()

    def _build_ui(self):
        container = self.ttk.Frame(self.window, padding=16)
        container.pack(fill="both", expand=True)

        header = self.ttk.Frame(container)
        header.pack(fill="x")
        self.ttk.Label(header, text="設定名稱").pack(side="left")
        self.ttk.Entry(
            header,
            textvariable=self.name_var,
            width=38,
            style="Latin.TEntry",
        ).pack(
            side="left",
            padx=(8, 18),
        )
        self.ttk.Checkbutton(
            header,
            text="只有全域脈絡詞時列為待人工判讀",
            variable=self.fallback_var,
        ).pack(side="left")
        self.ttk.Label(header, textvariable=self.summary_var).pack(side="right")

        self.notebook = self.ttk.Notebook(container)
        self.notebook.pack(fill="both", expand=True, pady=(12, 10))
        self.topic_tab = self.ttk.Frame(self.notebook, padding=10)
        self.keyword_tab = self.ttk.Frame(self.notebook, padding=10)
        self.exclusion_tab = self.ttk.Frame(self.notebook, padding=10)
        self.test_tab = self.ttk.Frame(self.notebook, padding=10)
        self.notebook.add(self.topic_tab, text="主題")
        self.notebook.add(self.keyword_tab, text="納入關鍵字")
        self.notebook.add(self.exclusion_tab, text="排除關鍵字")
        self.notebook.add(self.test_tab, text="測試判定")

        self._build_topic_tab()
        self._build_keyword_tab()
        self._build_exclusion_tab()
        self._build_test_tab()

        utility_row = self.ttk.Frame(container)
        utility_row.pack(fill="x")
        self.ttk.Button(
            utility_row,
            text="匯入設定",
            command=self._import_profile,
        ).pack(side="left")
        self.ttk.Button(
            utility_row,
            text="匯出設定",
            command=self._export_profile,
        ).pack(side="left", padx=6)
        self.ttk.Button(
            utility_row,
            text="加入新版範本",
            command=self._merge_defaults,
        ).pack(side="left")
        self.ttk.Button(
            utility_row,
            text="恢復 AI 十大建設範本",
            command=self._restore_defaults,
        ).pack(side="left", padx=6)

        self.ttk.Button(
            utility_row,
            text="取消",
            command=self._cancel,
        ).pack(side="right")
        self.ttk.Button(
            utility_row,
            text="儲存設定",
            style="Primary.TButton",
            command=self._save,
        ).pack(side="right", padx=8)

    def _make_tree(self, parent, columns):
        frame = self.ttk.Frame(parent)
        frame.pack(fill="both", expand=True)
        tree = self.ttk.Treeview(
            frame,
            columns=[column[0] for column in columns],
            show="headings",
            selectmode="browse",
        )
        for key, title, width in columns:
            tree.heading(key, text=title)
            tree.column(key, width=width, minwidth=70, stretch=True)
        scrollbar = self.ttk.Scrollbar(frame, orient="vertical", command=tree.yview)
        tree.configure(yscrollcommand=scrollbar.set)
        tree.pack(side="left", fill="both", expand=True)
        scrollbar.pack(side="right", fill="y")
        return tree

    def _build_topic_tab(self):
        self.topic_tree = self._make_tree(
            self.topic_tab,
            [
                ("enabled", "狀態", 80),
                ("name", "主題名稱", 250),
                ("sources", "優先關聯機關", 260),
                ("match_name", "比對名稱", 100),
                ("color", "顯示顏色", 100),
            ],
        )
        self.topic_tree.bind("<Double-1>", lambda _event: self._edit_topic())
        self.topic_tree.bind("<ButtonPress-1>", self._topic_drag_start)
        self.topic_tree.bind("<ButtonRelease-1>", self._topic_drag_end)

        actions = self.ttk.Frame(self.topic_tab)
        actions.pack(fill="x", pady=(8, 0))
        for label, command in (
            ("新增主題", self._add_topic),
            ("複製主題", self._duplicate_topic),
            ("編輯", self._edit_topic),
            ("啟用／停用", self._toggle_topic),
            ("刪除", self._delete_topic),
            ("復原刪除", self._undo_delete),
            ("上移", lambda: self._move_topic(-1)),
            ("下移", lambda: self._move_topic(1)),
        ):
            self.ttk.Button(actions, text=label, command=command).pack(
                side="left",
                padx=(0, 6),
            )

    def _build_keyword_tab(self):
        filters = self.ttk.Frame(self.keyword_tab)
        filters.pack(fill="x", pady=(0, 8))
        self.ttk.Label(filters, text="搜尋").pack(side="left")
        self.ttk.Entry(
            filters,
            textvariable=self.keyword_search_var,
            width=24,
            style="Latin.TEntry",
        ).pack(
            side="left",
            padx=(6, 14),
        )
        self.ttk.Label(filters, text="主題").pack(side="left")
        self.keyword_topic_filter = self.ttk.Combobox(
            filters,
            textvariable=self.keyword_topic_filter_var,
            state="readonly",
            width=28,
        )
        self.keyword_topic_filter.pack(side="left", padx=(6, 14))
        self.keyword_topic_filter.bind(
            "<<ComboboxSelected>>",
            lambda _event: self._refresh_keywords(),
        )
        self.ttk.Label(filters, text="類型").pack(side="left")
        keyword_type_filter = self.ttk.Combobox(
            filters,
            textvariable=self.keyword_type_filter_var,
            values=["全部類型", "核心詞", "輔助詞", "脈絡詞"],
            state="readonly",
            width=14,
        )
        keyword_type_filter.pack(side="left", padx=(6, 0))
        keyword_type_filter.bind(
            "<<ComboboxSelected>>",
            lambda _event: self._refresh_keywords(),
        )
        self.ttk.Label(filters, text="狀態").pack(side="left", padx=(14, 0))
        keyword_status_filter = self.ttk.Combobox(
            filters,
            textvariable=self.keyword_status_filter_var,
            values=["全部狀態", "啟用", "停用"],
            state="readonly",
            width=10,
        )
        keyword_status_filter.pack(side="left", padx=(6, 0))
        keyword_status_filter.bind(
            "<<ComboboxSelected>>",
            lambda _event: self._refresh_keywords(),
        )

        self.keyword_tree = self._make_tree(
            self.keyword_tab,
            [
                ("enabled", "狀態", 80),
                ("topic", "主題", 240),
                ("kind", "類型", 100),
                ("text", "關鍵字", 310),
                ("origin", "來源", 90),
            ],
        )
        self.keyword_tree.bind("<Double-1>", lambda _event: self._edit_keyword())
        actions = self.ttk.Frame(self.keyword_tab)
        actions.pack(fill="x", pady=(8, 0))
        for label, command in (
            ("新增關鍵字", self._add_keyword),
            ("編輯", self._edit_keyword),
            ("啟用／停用", self._toggle_keyword),
            ("刪除", self._delete_keyword),
            ("復原刪除", self._undo_delete),
        ):
            self.ttk.Button(actions, text=label, command=command).pack(
                side="left",
                padx=(0, 6),
            )

    def _build_exclusion_tab(self):
        filters = self.ttk.Frame(self.exclusion_tab)
        filters.pack(fill="x", pady=(0, 8))
        self.ttk.Label(filters, text="搜尋").pack(side="left")
        self.ttk.Entry(
            filters,
            textvariable=self.exclusion_search_var,
            width=24,
            style="Latin.TEntry",
        ).pack(side="left", padx=(6, 14))
        self.ttk.Label(filters, text="範圍").pack(side="left")
        self.exclusion_topic_filter = self.ttk.Combobox(
            filters,
            textvariable=self.exclusion_topic_filter_var,
            state="readonly",
            width=28,
        )
        self.exclusion_topic_filter.pack(side="left", padx=(6, 14))
        self.exclusion_topic_filter.bind(
            "<<ComboboxSelected>>",
            lambda _event: self._refresh_exclusions(),
        )
        self.ttk.Label(filters, text="狀態").pack(side="left")
        exclusion_status_filter = self.ttk.Combobox(
            filters,
            textvariable=self.exclusion_status_filter_var,
            values=["全部狀態", "啟用", "停用"],
            state="readonly",
            width=10,
        )
        exclusion_status_filter.pack(side="left", padx=(6, 0))
        exclusion_status_filter.bind(
            "<<ComboboxSelected>>",
            lambda _event: self._refresh_exclusions(),
        )

        self.exclusion_tree = self._make_tree(
            self.exclusion_tab,
            [
                ("enabled", "狀態", 80),
                ("scope", "套用範圍", 240),
                ("text", "排除關鍵字", 310),
                ("fields", "比對欄位", 140),
                ("origin", "來源", 90),
            ],
        )
        self.exclusion_tree.bind("<Double-1>", lambda _event: self._edit_exclusion())
        actions = self.ttk.Frame(self.exclusion_tab)
        actions.pack(fill="x", pady=(8, 0))
        for label, command in (
            ("新增排除詞", self._add_exclusion),
            ("編輯", self._edit_exclusion),
            ("啟用／停用", self._toggle_exclusion),
            ("刪除", self._delete_exclusion),
            ("復原刪除", self._undo_delete),
        ):
            self.ttk.Button(actions, text=label, command=command).pack(
                side="left",
                padx=(0, 6),
            )

    def _build_test_tab(self):
        form = self.ttk.Frame(self.test_tab)
        form.pack(fill="x")
        self.ttk.Label(form, text="新聞標題").grid(row=0, column=0, sticky="w")
        self.ttk.Entry(
            form,
            textvariable=self.test_title_var,
            style="Latin.TEntry",
        ).grid(
            row=1,
            column=0,
            columnspan=2,
            sticky="ew",
            pady=(4, 10),
        )
        self.ttk.Label(form, text="發布機關").grid(row=2, column=0, sticky="w")
        self.test_source = self.ttk.Combobox(
            form,
            textvariable=self.test_source_var,
            values=self.available_sources,
            state="readonly",
        )
        self.test_source.grid(row=3, column=0, sticky="ew", pady=(4, 10))
        self.ttk.Label(form, text="新聞摘要").grid(row=4, column=0, sticky="w")
        self.test_summary = self.tk.Text(form, height=7, wrap="word")
        self.test_summary.grid(
            row=5,
            column=0,
            columnspan=2,
            sticky="nsew",
            pady=(4, 10),
        )
        self.ttk.Button(
            form,
            text="測試判定",
            style="Primary.TButton",
            command=self._test_profile,
        ).grid(row=6, column=0, sticky="w")
        self.ttk.Label(
            form,
            textvariable=self.test_result_var,
            justify="left",
            wraplength=850,
        ).grid(row=7, column=0, columnspan=2, sticky="nw", pady=(14, 0))
        form.columnconfigure(0, weight=1)
        form.rowconfigure(5, weight=1)

    def _selected_id(self, tree):
        selection = tree.selection()
        return selection[0] if selection else ""

    def _topic_by_id(self, topic_id):
        return next((topic for topic in self.profile.topics if topic.id == topic_id), None)

    def _refresh_all(self):
        self.profile.name = self.name_var.get().strip() or self.profile.name
        self.profile.include_unassigned_context_matches = bool(self.fallback_var.get())
        self._refresh_topics()
        self._refresh_keywords()
        self._refresh_exclusions()
        summary = get_relevance_profile_summary(self.profile)
        self.summary_var.set(
            "{} 個啟用主題／{} 個啟用關鍵字／{} 個啟用排除詞／{} 個自訂項目".format(
                summary["enabled_topic_count"],
                summary["enabled_keyword_count"],
                summary["enabled_exclusion_count"],
                summary["custom_item_count"],
            )
        )

    def _refresh_topics(self):
        selected = self._selected_id(self.topic_tree)
        self.topic_tree.delete(*self.topic_tree.get_children())
        for topic in self.profile.topics:
            self.topic_tree.insert(
                "",
                "end",
                iid=topic.id,
                values=(
                    "啟用" if topic.enabled else "停用",
                    topic.name,
                    "、".join(topic.priority_sources),
                    "是" if topic.match_name else "否",
                    topic.display_color,
                ),
            )
        if selected and self.topic_tree.exists(selected):
            self.topic_tree.selection_set(selected)

    def _keyword_rows(self):
        for rule in self.profile.global_context_keywords:
            yield rule.id, "全域", "脈絡詞", rule
        for topic in self.profile.topics:
            for kind, rules in (
                ("核心詞", topic.core_keywords),
                ("輔助詞", topic.supporting_keywords),
                ("脈絡詞", topic.context_keywords),
            ):
                for rule in rules:
                    yield rule.id, topic.name, kind, rule

    def _refresh_keywords(self):
        selected = self._selected_id(self.keyword_tree)
        self.keyword_tree.delete(*self.keyword_tree.get_children())
        topic_values = ["全部主題", "全域"] + [topic.name for topic in self.profile.topics]
        self.keyword_topic_filter.configure(values=topic_values)
        if self.keyword_topic_filter_var.get() not in topic_values:
            self.keyword_topic_filter_var.set("全部主題")

        query = self.keyword_search_var.get().strip().casefold()
        topic_filter = self.keyword_topic_filter_var.get()
        type_filter = self.keyword_type_filter_var.get()
        status_filter = self.keyword_status_filter_var.get()
        for rule_id, topic_name, kind, rule in self._keyword_rows():
            if query and query not in rule.text.casefold():
                continue
            if topic_filter != "全部主題" and topic_filter != topic_name:
                continue
            if type_filter != "全部類型" and type_filter != kind:
                continue
            if status_filter != "全部狀態" and status_filter != (
                "啟用" if rule.enabled else "停用"
            ):
                continue
            self.keyword_tree.insert(
                "",
                "end",
                iid=rule_id,
                values=(
                    "啟用" if rule.enabled else "停用",
                    topic_name,
                    kind,
                    rule.text,
                    "預設" if rule.origin == "default" else "自訂",
                ),
            )
        if selected and self.keyword_tree.exists(selected):
            self.keyword_tree.selection_set(selected)

    def _refresh_exclusions(self):
        selected = self._selected_id(self.exclusion_tree)
        self.exclusion_tree.delete(*self.exclusion_tree.get_children())
        topic_names = {topic.id: topic.name for topic in self.profile.topics}
        scope_values = ["全部範圍", "全域"] + [
            topic.name for topic in self.profile.topics
        ]
        self.exclusion_topic_filter.configure(values=scope_values)
        if self.exclusion_topic_filter_var.get() not in scope_values:
            self.exclusion_topic_filter_var.set("全部範圍")
        query = self.exclusion_search_var.get().strip().casefold()
        scope_filter = self.exclusion_topic_filter_var.get()
        status_filter = self.exclusion_status_filter_var.get()
        for rule in self.profile.exclusions:
            scope = topic_names.get(rule.topic_id, "全域")
            if query and query not in rule.text.casefold():
                continue
            if scope_filter != "全部範圍" and scope_filter != scope:
                continue
            if status_filter != "全部狀態" and status_filter != (
                "啟用" if rule.enabled else "停用"
            ):
                continue
            fields = {
                ("title",): "標題",
                ("summary",): "摘要",
                ("title", "summary"): "標題與摘要",
                ("summary", "title"): "標題與摘要",
            }.get(tuple(rule.match_fields), "、".join(rule.match_fields))
            self.exclusion_tree.insert(
                "",
                "end",
                iid=rule.id,
                values=(
                    "啟用" if rule.enabled else "停用",
                    scope,
                    rule.text,
                    fields,
                    "預設" if rule.origin == "default" else "自訂",
                ),
            )
        if selected and self.exclusion_tree.exists(selected):
            self.exclusion_tree.selection_set(selected)

    def _ask_topic(self, topic=None):
        topic = copy.deepcopy(topic) if topic else TopicRule(id=new_custom_id("topic"), name="")
        dialog = self.tk.Toplevel(self.window)
        dialog.title("編輯主題" if topic.name else "新增主題")
        dialog.transient(self.window)
        dialog.grab_set()
        name_var = self.tk.StringVar(value=topic.name)
        description_var = self.tk.StringVar(value=topic.description)
        color_var = self.tk.StringVar(value=topic.display_color)
        enabled_var = self.tk.BooleanVar(value=topic.enabled)
        match_name_var = self.tk.BooleanVar(value=topic.match_name)
        result_value: list[TopicRule | None] = [None]

        frame = self.ttk.Frame(dialog, padding=14)
        frame.pack(fill="both", expand=True)
        for row, (label, variable) in enumerate(
            (("主題名稱", name_var), ("說明", description_var), ("顯示顏色", color_var))
        ):
            self.ttk.Label(frame, text=label).grid(row=row * 2, column=0, sticky="w")
            self.ttk.Entry(
                frame,
                textvariable=variable,
                width=54,
                style="Latin.TEntry",
            ).grid(
                row=row * 2 + 1,
                column=0,
                sticky="ew",
                pady=(3, 8),
            )
        self.ttk.Checkbutton(frame, text="啟用主題", variable=enabled_var).grid(
            row=6,
            column=0,
            sticky="w",
        )
        self.ttk.Checkbutton(
            frame,
            text="將主題名稱作為最高關聯性比對詞",
            variable=match_name_var,
        ).grid(row=7, column=0, sticky="w", pady=(4, 8))
        self.ttk.Label(frame, text="優先關聯機關（可複選）").grid(
            row=8,
            column=0,
            sticky="w",
        )
        source_list = self.tk.Listbox(
            frame,
            selectmode="multiple",
            exportselection=False,
            height=9,
        )
        source_list.grid(row=9, column=0, sticky="nsew", pady=(3, 8))
        for index, source in enumerate(self.available_sources):
            source_list.insert("end", source)
            if source in topic.priority_sources:
                source_list.selection_set(index)

        actions = self.ttk.Frame(frame)
        actions.grid(row=10, column=0, sticky="e")

        def accept():
            topic.name = name_var.get().strip()
            topic.description = description_var.get().strip()
            topic.display_color = color_var.get().strip().upper()
            topic.enabled = bool(enabled_var.get())
            topic.match_name = bool(match_name_var.get())
            topic.priority_sources = [
                self.available_sources[index] for index in source_list.curselection()
            ]
            if not topic.name:
                self.messagebox.showerror("設定錯誤", "主題名稱不可空白。", parent=dialog)
                return
            result_value[0] = topic
            dialog.destroy()

        self.ttk.Button(actions, text="取消", command=dialog.destroy).pack(side="right")
        self.ttk.Button(actions, text="確定", command=accept).pack(
            side="right",
            padx=6,
        )
        frame.columnconfigure(0, weight=1)
        frame.rowconfigure(9, weight=1)
        dialog.geometry("560x590")
        dialog.wait_window()
        return result_value[0]

    def _add_topic(self):
        topic = self._ask_topic()
        if topic:
            self.profile.topics.append(topic)
            self._refresh_all()
            self.topic_tree.selection_set(topic.id)

    def _edit_topic(self):
        topic_id = self._selected_id(self.topic_tree)
        topic = self._topic_by_id(topic_id)
        if not topic:
            return
        updated = self._ask_topic(topic)
        if updated:
            index = self.profile.topics.index(topic)
            self.profile.topics[index] = updated
            self._refresh_all()
            self.topic_tree.selection_set(updated.id)

    def _duplicate_topic(self):
        topic = self._topic_by_id(self._selected_id(self.topic_tree))
        if not topic:
            return
        duplicated = copy.deepcopy(topic)
        duplicated.id = new_custom_id("topic")
        duplicated.name = "{}（副本）".format(topic.name)
        duplicated.origin = "custom"
        for _kind, rule in self._topic_keyword_rows(duplicated):
            rule.id = new_custom_id("keyword")
            rule.origin = "custom"
        duplicated_exclusions = []
        for exclusion in self.profile.exclusions:
            if exclusion.topic_id != topic.id:
                continue
            duplicated_exclusion = copy.deepcopy(exclusion)
            duplicated_exclusion.id = new_custom_id("exclusion")
            duplicated_exclusion.topic_id = duplicated.id
            duplicated_exclusion.origin = "custom"
            duplicated_exclusions.append(duplicated_exclusion)
        self.profile.exclusions.extend(duplicated_exclusions)
        self.profile.topics.insert(self.profile.topics.index(topic) + 1, duplicated)
        self._refresh_all()
        self.topic_tree.selection_set(duplicated.id)

    def _topic_keyword_rows(self, topic):
        for kind, rules in (
            ("核心詞", topic.core_keywords),
            ("輔助詞", topic.supporting_keywords),
            ("脈絡詞", topic.context_keywords),
        ):
            for rule in rules:
                yield kind, rule

    def _toggle_topic(self):
        topic = self._topic_by_id(self._selected_id(self.topic_tree))
        if topic:
            topic.enabled = not topic.enabled
            self._refresh_all()

    def _delete_topic(self):
        topic = self._topic_by_id(self._selected_id(self.topic_tree))
        if not topic:
            return
        if not self.messagebox.askyesno(
            "刪除主題",
            "確定刪除「{}」及其全部關鍵字嗎？".format(topic.name),
            parent=self.window,
        ):
            return
        index = self.profile.topics.index(topic)
        related_exclusions = [
            rule for rule in self.profile.exclusions if rule.topic_id == topic.id
        ]
        self.profile.exclusions = [
            rule for rule in self.profile.exclusions if rule.topic_id != topic.id
        ]
        delete_default_aware(self.profile, topic.id)
        for _kind, rule in self._topic_keyword_rows(topic):
            delete_default_aware(self.profile, rule.id)
        for rule in related_exclusions:
            delete_default_aware(self.profile, rule.id)
        self.profile.topics.remove(topic)
        self.deleted_stack.append(("topic", index, topic, related_exclusions))
        self._refresh_all()

    def _move_topic(self, offset):
        topic = self._topic_by_id(self._selected_id(self.topic_tree))
        if not topic:
            return
        index = self.profile.topics.index(topic)
        target = max(0, min(len(self.profile.topics) - 1, index + offset))
        if target == index:
            return
        self.profile.topics.pop(index)
        self.profile.topics.insert(target, topic)
        self._refresh_all()
        self.topic_tree.selection_set(topic.id)

    def _topic_drag_start(self, event):
        self.dragged_topic_id = self.topic_tree.identify_row(event.y)

    def _topic_drag_end(self, event):
        target_id = self.topic_tree.identify_row(event.y)
        topic = self._topic_by_id(self.dragged_topic_id)
        target = self._topic_by_id(target_id)
        self.dragged_topic_id = ""
        if not topic or not target or topic is target:
            return
        self.profile.topics.remove(topic)
        self.profile.topics.insert(self.profile.topics.index(target), topic)
        self._refresh_all()
        self.topic_tree.selection_set(topic.id)

    def _locate_keyword(self, rule_id):
        for rule in self.profile.global_context_keywords:
            if rule.id == rule_id:
                return "global", None, "脈絡詞", self.profile.global_context_keywords, rule
        for topic in self.profile.topics:
            for kind, rules in (
                ("核心詞", topic.core_keywords),
                ("輔助詞", topic.supporting_keywords),
                ("脈絡詞", topic.context_keywords),
            ):
                for rule in rules:
                    if rule.id == rule_id:
                        return "topic", topic, kind, rules, rule
        return None

    def _ask_keyword(self, located=None):
        existing = located[-1] if located else None
        dialog = self.tk.Toplevel(self.window)
        dialog.title("編輯關鍵字" if existing else "新增關鍵字")
        dialog.transient(self.window)
        dialog.grab_set()
        topic_values = ["全域"] + [topic.name for topic in self.profile.topics]
        initial_topic = (
            "全域"
            if located and located[0] == "global"
            else (located[1].name if located else topic_values[1] if len(topic_values) > 1 else "全域")
        )
        topic_var = self.tk.StringVar(value=initial_topic)
        kind_var = self.tk.StringVar(value=located[2] if located else "核心詞")
        text_var = self.tk.StringVar(value=existing.text if existing else "")
        enabled_var = self.tk.BooleanVar(value=existing.enabled if existing else True)
        result_value: list[dict | None] = [None]
        frame = self.ttk.Frame(dialog, padding=14)
        frame.pack(fill="both", expand=True)
        self.ttk.Label(frame, text="套用主題").grid(row=0, column=0, sticky="w")
        topic_combo = self.ttk.Combobox(
            frame,
            textvariable=topic_var,
            values=topic_values,
            state="readonly",
            width=38,
        )
        topic_combo.grid(row=1, column=0, sticky="ew", pady=(3, 8))
        self.ttk.Label(frame, text="關鍵字類型").grid(row=2, column=0, sticky="w")
        kind_combo = self.ttk.Combobox(
            frame,
            textvariable=kind_var,
            values=["核心詞", "輔助詞", "脈絡詞"],
            state="readonly",
        )
        kind_combo.grid(row=3, column=0, sticky="ew", pady=(3, 8))

        def sync_kind(*_args):
            if topic_var.get() == "全域":
                kind_var.set("脈絡詞")
                kind_combo.configure(state="disabled")
            else:
                kind_combo.configure(state="readonly")

        topic_var.trace_add("write", sync_kind)
        sync_kind()
        self.ttk.Label(frame, text="關鍵字").grid(row=4, column=0, sticky="w")
        self.ttk.Entry(
            frame,
            textvariable=text_var,
            style="Latin.TEntry",
        ).grid(
            row=5,
            column=0,
            sticky="ew",
            pady=(3, 8),
        )
        self.ttk.Checkbutton(frame, text="啟用", variable=enabled_var).grid(
            row=6,
            column=0,
            sticky="w",
        )
        actions = self.ttk.Frame(frame)
        actions.grid(row=7, column=0, sticky="e", pady=(10, 0))

        def accept():
            text = text_var.get().strip()
            if not text:
                self.messagebox.showerror("設定錯誤", "關鍵字不可空白。", parent=dialog)
                return
            result_value[0] = {
                "topic": topic_var.get(),
                "kind": kind_var.get(),
                "rule": KeywordRule(
                    id=existing.id if existing else new_custom_id("keyword"),
                    text=text,
                    enabled=bool(enabled_var.get()),
                    origin=existing.origin if existing else "custom",
                ),
            }
            dialog.destroy()

        self.ttk.Button(actions, text="取消", command=dialog.destroy).pack(side="right")
        self.ttk.Button(actions, text="確定", command=accept).pack(
            side="right",
            padx=6,
        )
        frame.columnconfigure(0, weight=1)
        dialog.geometry("470x330")
        dialog.wait_window()
        return result_value[0]

    def _keyword_target(self, topic_name, kind):
        if topic_name == "全域":
            return self.profile.global_context_keywords
        topic = next(topic for topic in self.profile.topics if topic.name == topic_name)
        return {
            "核心詞": topic.core_keywords,
            "輔助詞": topic.supporting_keywords,
            "脈絡詞": topic.context_keywords,
        }[kind]

    def _add_keyword(self):
        result = self._ask_keyword()
        if result:
            self._keyword_target(result["topic"], result["kind"]).append(result["rule"])
            self._refresh_all()

    def _edit_keyword(self):
        located = self._locate_keyword(self._selected_id(self.keyword_tree))
        if not located:
            return
        result = self._ask_keyword(located)
        if result:
            located[3].remove(located[4])
            self._keyword_target(result["topic"], result["kind"]).append(result["rule"])
            self._refresh_all()
            if self.keyword_tree.exists(result["rule"].id):
                self.keyword_tree.selection_set(result["rule"].id)

    def _toggle_keyword(self):
        located = self._locate_keyword(self._selected_id(self.keyword_tree))
        if located:
            located[4].enabled = not located[4].enabled
            self._refresh_all()

    def _delete_keyword(self):
        located = self._locate_keyword(self._selected_id(self.keyword_tree))
        if not located:
            return
        container, rule = located[3], located[4]
        index = container.index(rule)
        delete_default_aware(self.profile, rule.id)
        container.remove(rule)
        self.deleted_stack.append(("keyword", located[0], located[1], located[2], index, rule))
        self._refresh_all()

    def _ask_exclusion(self, rule=None):
        rule = copy.deepcopy(rule) if rule else ExclusionRule(
            id=new_custom_id("exclusion"),
            text="",
        )
        dialog = self.tk.Toplevel(self.window)
        dialog.title("編輯排除詞" if rule.text else "新增排除詞")
        dialog.transient(self.window)
        dialog.grab_set()
        topic_values = ["全域"] + [topic.name for topic in self.profile.topics]
        topic_names = {topic.id: topic.name for topic in self.profile.topics}
        scope_var = self.tk.StringVar(value=topic_names.get(rule.topic_id, "全域"))
        text_var = self.tk.StringVar(value=rule.text)
        field_lookup = {
            ("title",): "標題",
            ("summary",): "摘要",
            ("title", "summary"): "標題與摘要",
            ("summary", "title"): "標題與摘要",
        }
        fields_var = self.tk.StringVar(
            value=field_lookup.get(tuple(rule.match_fields), "標題")
        )
        enabled_var = self.tk.BooleanVar(value=rule.enabled)
        result_value: list[ExclusionRule | None] = [None]
        frame = self.ttk.Frame(dialog, padding=14)
        frame.pack(fill="both", expand=True)
        self.ttk.Label(frame, text="套用範圍").grid(row=0, column=0, sticky="w")
        self.ttk.Combobox(
            frame,
            textvariable=scope_var,
            values=topic_values,
            state="readonly",
        ).grid(row=1, column=0, sticky="ew", pady=(3, 8))
        self.ttk.Label(frame, text="排除關鍵字").grid(row=2, column=0, sticky="w")
        self.ttk.Entry(
            frame,
            textvariable=text_var,
            style="Latin.TEntry",
        ).grid(
            row=3,
            column=0,
            sticky="ew",
            pady=(3, 8),
        )
        self.ttk.Label(frame, text="比對欄位").grid(row=4, column=0, sticky="w")
        self.ttk.Combobox(
            frame,
            textvariable=fields_var,
            values=["標題", "摘要", "標題與摘要"],
            state="readonly",
        ).grid(row=5, column=0, sticky="ew", pady=(3, 8))
        self.ttk.Checkbutton(frame, text="啟用", variable=enabled_var).grid(
            row=6,
            column=0,
            sticky="w",
        )
        actions = self.ttk.Frame(frame)
        actions.grid(row=7, column=0, sticky="e", pady=(10, 0))

        def accept():
            text = text_var.get().strip()
            if not text:
                self.messagebox.showerror("設定錯誤", "排除關鍵字不可空白。", parent=dialog)
                return
            topic_id = ""
            if scope_var.get() != "全域":
                topic_id = next(
                    topic.id for topic in self.profile.topics if topic.name == scope_var.get()
                )
            fields = {
                "標題": ["title"],
                "摘要": ["summary"],
                "標題與摘要": ["title", "summary"],
            }[fields_var.get()]
            result_value[0] = ExclusionRule(
                id=rule.id,
                text=text,
                topic_id=topic_id,
                match_fields=fields,
                enabled=bool(enabled_var.get()),
                origin=rule.origin,
            )
            dialog.destroy()

        self.ttk.Button(actions, text="取消", command=dialog.destroy).pack(side="right")
        self.ttk.Button(actions, text="確定", command=accept).pack(
            side="right",
            padx=6,
        )
        frame.columnconfigure(0, weight=1)
        dialog.geometry("470x350")
        dialog.wait_window()
        return result_value[0]

    def _exclusion_by_id(self, rule_id):
        return next((rule for rule in self.profile.exclusions if rule.id == rule_id), None)

    def _add_exclusion(self):
        rule = self._ask_exclusion()
        if rule:
            self.profile.exclusions.append(rule)
            self._refresh_all()

    def _edit_exclusion(self):
        rule = self._exclusion_by_id(self._selected_id(self.exclusion_tree))
        if not rule:
            return
        updated = self._ask_exclusion(rule)
        if updated:
            self.profile.exclusions[self.profile.exclusions.index(rule)] = updated
            self._refresh_all()
            self.exclusion_tree.selection_set(updated.id)

    def _toggle_exclusion(self):
        rule = self._exclusion_by_id(self._selected_id(self.exclusion_tree))
        if rule:
            rule.enabled = not rule.enabled
            self._refresh_all()

    def _delete_exclusion(self):
        rule = self._exclusion_by_id(self._selected_id(self.exclusion_tree))
        if not rule:
            return
        index = self.profile.exclusions.index(rule)
        delete_default_aware(self.profile, rule.id)
        self.profile.exclusions.remove(rule)
        self.deleted_stack.append(("exclusion", index, rule))
        self._refresh_all()

    def _undo_delete(self):
        if not self.deleted_stack:
            self.messagebox.showinfo("復原刪除", "目前沒有可以復原的刪除動作。", parent=self.window)
            return
        action = self.deleted_stack.pop()
        kind = action[0]
        restored_ids = []
        if kind == "topic":
            _kind, index, topic, exclusions = action
            self.profile.topics.insert(index, topic)
            self.profile.exclusions.extend(exclusions)
            restored_ids.append(topic.id)
            restored_ids.extend(rule.id for _label, rule in self._topic_keyword_rows(topic))
            restored_ids.extend(rule.id for rule in exclusions)
        elif kind == "keyword":
            _kind, scope, topic, label, index, rule = action
            container = (
                self.profile.global_context_keywords
                if scope == "global"
                else {
                    "核心詞": topic.core_keywords,
                    "輔助詞": topic.supporting_keywords,
                    "脈絡詞": topic.context_keywords,
                }[label]
            )
            container.insert(index, rule)
            restored_ids.append(rule.id)
        else:
            _kind, index, rule = action
            self.profile.exclusions.insert(index, rule)
            restored_ids.append(rule.id)
        self.profile.deleted_default_ids = [
            item_id
            for item_id in self.profile.deleted_default_ids
            if item_id not in restored_ids
        ]
        self._refresh_all()

    def _test_profile(self):
        self.profile.name = self.name_var.get().strip() or self.profile.name
        self.profile.include_unassigned_context_matches = bool(self.fallback_var.get())
        try:
            validate_relevance_profile(
                self.profile,
                available_sources=self.available_sources,
            )
            result = classify_relevance(
                self.test_title_var.get(),
                source=self.test_source_var.get(),
                summary=self.test_summary.get("1.0", "end").strip(),
                profile=self.profile,
            )
        except ValueError as exc:
            self.messagebox.showerror("設定錯誤", str(exc), parent=self.window)
            return
        self.test_result_var.set(
            "關聯性：{}　分數：{}\n主題：{}\n命中：{}\n排除：{}\n理由：{}".format(
                result["relevance"] or "不相關",
                result["score"],
                "、".join(result["topics"]) or "無",
                "、".join(result["matched_keywords"]) or "無",
                "、".join(result["excluded_keywords"]) or "無",
                "；".join(result["reasons"]) or "未命中任何有效規則",
            )
        )

    def _import_profile(self):
        selected = self.filedialog.askopenfilename(
            parent=self.window,
            title="匯入主題關聯性設定",
            filetypes=[("JSON 設定檔", "*.json"), ("所有檔案", "*")],
        )
        if not selected:
            return
        try:
            loaded = load_relevance_profile(
                selected,
                available_sources=self.available_sources,
                merge_defaults=False,
            )
            summary = get_relevance_profile_summary(loaded.profile)
            difference = compare_relevance_profiles(
                self.profile,
                loaded.profile,
            )
        except ValueError as exc:
            self.messagebox.showerror("匯入失敗", str(exc), parent=self.window)
            return

        def preview(items):
            values = list(items)
            if not values:
                return "無"
            visible = "、".join(values[:5])
            if len(values) > 5:
                visible += "，另有 {} 項".format(len(values) - 5)
            return visible

        if not self.messagebox.askyesno(
            "確認匯入",
            (
                "設定：{}\n主題：{}\n關鍵字：{}\n排除詞：{}\n\n"
                "新增 {} 項：{}\n修改 {} 項：{}\n刪除 {} 項：{}\n\n"
                "匯入後將取代目前尚未儲存的設定；儲存時會備份現有設定。"
            ).format(
                summary["name"],
                summary["topic_count"],
                summary["keyword_count"],
                summary["exclusion_count"],
                difference["added_count"],
                preview(difference["added"]),
                difference["modified_count"],
                preview(difference["modified"]),
                difference["removed_count"],
                preview(difference["removed"]),
            ),
            parent=self.window,
        ):
            return
        self.profile = loaded.profile.clone()
        self.name_var.set(self.profile.name)
        self.fallback_var.set(self.profile.include_unassigned_context_matches)
        self.deleted_stack.clear()
        self.imported_profile = True
        self._refresh_all()

    def _export_profile(self):
        self.profile.name = self.name_var.get().strip() or self.profile.name
        self.profile.include_unassigned_context_matches = bool(self.fallback_var.get())
        selected = self.filedialog.asksaveasfilename(
            parent=self.window,
            title="匯出主題關聯性設定",
            defaultextension=".json",
            filetypes=[("JSON 設定檔", "*.json")],
            initialfile="relevance-profile.json",
        )
        if not selected:
            return
        try:
            save_relevance_profile(
                selected,
                self.profile,
                available_sources=self.available_sources,
            )
        except ValueError as exc:
            self.messagebox.showerror("匯出失敗", str(exc), parent=self.window)
            return
        self.messagebox.showinfo("匯出完成", "主題關聯性設定已匯出。", parent=self.window)

    def _merge_defaults(self):
        self.profile, changed = merge_new_default_rules(self.profile)
        self.name_var.set(self.profile.name)
        self._refresh_all()
        self.messagebox.showinfo(
            "加入新版範本",
            "已加入新版預設項目。" if changed else "目前已包含所有新版預設項目。",
            parent=self.window,
        )

    def _restore_defaults(self):
        if not self.messagebox.askyesno(
            "恢復預設範本",
            "這會清除目前所有主題與自訂關鍵字，確定繼續嗎？",
            parent=self.window,
        ):
            return
        self.profile = build_default_relevance_profile()
        self.name_var.set(self.profile.name)
        self.fallback_var.set(self.profile.include_unassigned_context_matches)
        self.deleted_stack.clear()
        self._refresh_all()

    def _save(self):
        self.profile.name = self.name_var.get().strip()
        self.profile.include_unassigned_context_matches = bool(self.fallback_var.get())
        try:
            save_relevance_profile(
                self.profile_path,
                self.profile,
                available_sources=self.available_sources,
                backup=self.imported_profile,
            )
        except ValueError as exc:
            self.messagebox.showerror("設定錯誤", str(exc), parent=self.window)
            return
        self.on_saved(self.profile.clone())
        self.window.grab_release()
        self.window.destroy()

    def _cancel(self):
        self.window.grab_release()
        self.window.destroy()

    def validate_smoke_test(self) -> bool:
        self.window.update_idletasks()
        visible_text = set()
        pending: list[Any] = [self.window]
        while pending:
            widget = pending.pop()
            pending.extend(widget.winfo_children())
            try:
                text = str(widget.cget("text")).strip()
            except self.tk.TclError:
                continue
            if text:
                visible_text.add(text)
        required_text = {
            "新增主題",
            "新增關鍵字",
            "新增排除詞",
            "匯入設定",
            "匯出設定",
            "加入新版範本",
            "恢復 AI 十大建設範本",
            "儲存設定",
        }
        required_tabs = {"主題", "納入關鍵字", "排除關鍵字", "測試判定"}
        visible_tabs = {
            str(self.notebook.tab(tab_id, "text"))
            for tab_id in self.notebook.tabs()
        }
        return required_text <= visible_text and required_tabs == visible_tabs and (
            self.window.winfo_reqwidth() <= self.window.winfo_screenwidth()
        )


def open_relevance_profile_editor(
    parent,
    *,
    profile,
    profile_path,
    available_sources,
    on_saved,
):
    return RelevanceProfileEditor(
        parent,
        profile=profile,
        profile_path=Path(profile_path),
        available_sources=list(available_sources),
        on_saved=on_saved,
    )
