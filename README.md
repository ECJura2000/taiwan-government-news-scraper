# News Scraper

中華民國政府政府部會每週新聞整合爬蟲，已由原本單檔程式拆分為可維護的套件結構。

## 目錄

```text
news_scraper/
  main.py                 CLI 入口
  config.py               URL、排序、timeout、headers
  models.py               資料模型
  excel_exporter.py       Excel 匯出
  http/                   requests / aiohttp 共用抓取
  rss/                    RSS / feedparser 共用解析
  utils/                  日期、文字、去重工具
  scrapers/
    registry.py           全來源註冊表
    base.py               scraper 共用 helper
    ministry/
      executive/          行政院
      oversight/          監察院
      finance/            財政金融與統計相關機關
      regulators/         委員會型機關
      foreign/            外交、陸委、僑委相關機關
      culture/            文化部與故宮
      digital/            數位發展部
      economy/            經濟部
      environment/        環境部
      development/        國發會、國科會及相關行政法人
      communities/        原民會與客委會
      sports/             運動部
      interior/           內政部及所屬機關
      justice/            法務部及所屬機關
      education/          教育部及相關機關
      transport/          交通部及所屬機關
      agriculture/        農業部及所屬機關
      health/             衛福部及所屬機關
      labor/              勞動部及所屬機關
      defense/            國防部及相關機關
      oceans/             海委會及所屬機關
      veterans/           退輔會及相關機關
```

## 安裝

```bash
python3 -m pip install -r requirements.txt
```

或：

```bash
python3 -m pip install -e .
```

> 若你的系統沒有 `python` 指令，請把下列指令的 `python` 改成 `python3`。

## 使用方式

列出目前支援來源：

```bash
python -m news_scraper --list-sources
```

抓取全部來源：

```bash
python -m news_scraper
```

未另外指定時，Excel 會預設輸出到專案根目錄的 `新聞搜集區/`。

只抓指定來源：

```bash
python -m news_scraper --sources 財政部 法務部 衛生福利部
```

指定輸出資料夾：

```bash
python -m news_scraper --output-dir /path/to/output
```

合併部會與所屬機關重複發布的同標題新聞：

```bash
python -m news_scraper --dedupe-affiliated
```

國土管理署網站是 JavaScript 動態頁，且會出現一般 `curl`/HTTP2 不易直接讀取的防護頁；程式會改用 Selenium 開啟瀏覽器取得列表。若只抓國土管理署，建議確認本機已安裝 Chrome/Chromium，並已安裝 `requirements.txt` 內的 `selenium`：

```bash
python -m news_scraper --sources 國土管理署 --max-workers 1
```

## 輸出

程式會輸出：

- 終端機表格摘要
- `本週新聞整理（民國起迄日期）.xlsx`
- 全部新聞工作表
- AI 關鍵字初步篩選工作表
- 指定重點部會分頁

## 開發說明

- `SCRAPER_REGISTRY` 位於 `news_scraper/scrapers/registry.py`
- `SOURCE_ORDER` 位於 `news_scraper/config.py`
- 共用 RSS 規則優先放在 `rss/` 與 `scrapers/base.py`
- 可重用工具函式優先放在 `utils/`
- 下級機關 scraper 盡量放在對應部會子資料夾

## 驗證

目前可用的基本檢查：

```bash
python3 -m compileall news_scraper
python -m news_scraper --list-sources
```

如果本機有安裝 `pytest`，也可以執行：

```bash
pytest -q
```
