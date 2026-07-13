import importlib.util
import sys

CORE_DEPENDENCIES = {
    "requests": "requests",
    "bs4": "beautifulsoup4",
}

SCRAPE_DEPENDENCIES = {
    "lxml": "lxml",
}

EXPORT_DEPENDENCIES = {
    "pandas": "pandas",
    "openpyxl": "openpyxl",
}

FEEDPARSER_SOURCE_NAMES = {
    "農業部",
    "農村發展及水土保持署",
    "高速公路局",
    "航港局",
}

OPTIONAL_SOURCE_DEPENDENCIES = {
    "feedparser": {
        "package": "feedparser",
        "sources": FEEDPARSER_SOURCE_NAMES,
    },
    "selenium": {
        "package": "selenium",
        "sources": {"國土管理署"},
    },
}


def normalize_selected_source_names(selected_sources):
    return {
        str(source_name).strip()
        for source_name in (selected_sources or [])
        if str(source_name).strip()
    }


def detect_missing_dependencies(module_package_map, spec_finder=None):
    if spec_finder is None:
        spec_finder = importlib.util.find_spec

    missing = []
    for module_name, package_name in module_package_map.items():
        try:
            module_spec = spec_finder(module_name)
        except (ImportError, ModuleNotFoundError, ValueError):
            module_spec = None
        if module_spec is None:
            missing.append((module_name, package_name))
    return missing


def build_missing_dependency_message(missing_dependencies):
    packages = []
    for module_name, package_name in missing_dependencies:
        if module_name == package_name:
            packages.append(package_name)
        else:
            packages.append("{} (`{}`)".format(package_name, module_name))

    unique_packages = list(dict.fromkeys(packages))
    exact_packages = list(dict.fromkeys(package_name for _, package_name in missing_dependencies))

    if getattr(sys, "frozen", False):
        return (
            "封裝執行檔缺少必要模組：{}\n"
            "這是封裝問題，無法透過對 .exe 執行 pip 修正。\n"
            "請改用最新版本的正式 Release。"
        ).format("、".join(unique_packages))

    pip_command = "{} -m pip install {}".format(sys.executable, " ".join(exact_packages))
    return (
        "缺少執行所需模組：{}\n"
        "請先安裝後再執行。\n"
        "建議指令：{}\n"
        "或在專案目錄使用：{} -m pip install -r requirements.txt"
    ).format(
        "、".join(unique_packages),
        pip_command,
        sys.executable,
    )


def collect_required_dependencies(selected_sources=None, needs_excel_export=True, list_sources_only=False):
    required = dict(CORE_DEPENDENCIES)

    if not list_sources_only:
        required.update(SCRAPE_DEPENDENCIES)
        if needs_excel_export:
            required.update(EXPORT_DEPENDENCIES)

    if not list_sources_only:
        selected_source_names = normalize_selected_source_names(selected_sources)
        for module_name, dependency_info in OPTIONAL_SOURCE_DEPENDENCIES.items():
            source_requires_dependency = (
                not selected_source_names
                or selected_source_names & dependency_info["sources"]
            )
            if source_requires_dependency:
                required[module_name] = str(dependency_info["package"])

    return required


def validate_runtime_environment(selected_sources=None, needs_excel_export=True, list_sources_only=False, spec_finder=None):
    required_dependencies = collect_required_dependencies(
        selected_sources=selected_sources,
        needs_excel_export=needs_excel_export,
        list_sources_only=list_sources_only,
    )
    missing_dependencies = detect_missing_dependencies(required_dependencies, spec_finder=spec_finder)
    if missing_dependencies:
        raise RuntimeError(build_missing_dependency_message(missing_dependencies))
    return True
