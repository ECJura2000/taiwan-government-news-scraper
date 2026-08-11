#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
app_path="$script_dir/各機關新聞整理.app"

if [[ ! -d "$app_path" ]]; then
  app_path="$(find "$script_dir" -maxdepth 3 -type d -name "各機關新聞整理.app" -print -quit)"
fi

if [[ -z "${app_path:-}" || ! -d "$app_path" ]]; then
  osascript -e 'display alert "找不到各機關新聞整理.app" message "請確認本檔案與各機關新聞整理.app 位於同一個解壓縮資料夾。"'
  exit 1
fi

xattr -cr "$app_path" 2>/dev/null || true
open "$app_path"
