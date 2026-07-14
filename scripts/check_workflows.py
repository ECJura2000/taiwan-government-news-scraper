from __future__ import annotations

import re
from pathlib import Path

import yaml


REMOTE_ACTION = re.compile(r"^\s*-?\s*uses:\s*([^\s#]+)", re.MULTILINE)
PINNED_ACTION = re.compile(r"^[^@]+@[0-9a-f]{40}$")


def main() -> int:
    workflow_dir = Path(__file__).resolve().parents[1] / ".github" / "workflows"
    failures = []
    for workflow_path in sorted(workflow_dir.glob("*.yml")):
        text = workflow_path.read_text(encoding="utf-8")
        try:
            document = yaml.safe_load(text)
        except yaml.YAMLError as exc:
            failures.append("{}: YAML 語法錯誤：{}".format(workflow_path.name, exc))
            continue
        if not isinstance(document, dict):
            failures.append("{}: workflow 根節點必須是 mapping".format(workflow_path.name))
            continue
        if "\t" in text:
            failures.append("{}: YAML 不得包含 tab".format(workflow_path.name))
        for action in REMOTE_ACTION.findall(text):
            if action.startswith("./") or action.startswith("docker://"):
                continue
            if not PINNED_ACTION.fullmatch(action):
                failures.append("{}: action 未固定完整 SHA：{}".format(workflow_path.name, action))
    if failures:
        raise SystemExit("\n".join(failures))
    print("Workflow action SHA 檢查通過。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
