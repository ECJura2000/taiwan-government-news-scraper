# 資料來源與參考資料

本專案整合中華民國政府機關與所屬單位公開發布的新聞、新聞稿及公告。資料來源以各機關官方網站、官方 RSS／Atom、開放資料頁面或網站提供的公開 API 為主。

## 使用原則

- 本專案不代表任何政府機關，也不修改原始新聞內容。
- Excel 與執行報告屬於自動整理結果；正式內容、發布時間及後續修正均以原發布機關網站為準。
- 來源網址可能因網站改版而異動。程式實際使用的最新網址以 [`news_scraper/config.py`](news_scraper/config.py) 與各 scraper 模組為準。
- 新增或更換來源時，應優先採用官方 RSS、Atom、開放資料或公開 API；只有缺少結構化入口時才解析 HTML。
- 使用、再利用或散布資料前，應另行確認各原發布網站的使用條款、著作權聲明與開放資料授權。

## 官方資料來源

| 機關／單位 | 主要官方入口 |
| --- | --- |
| 行政院 | [www.ey.gov.tw](https://www.ey.gov.tw/Page/6485009ABEC1CB9C?PS=50&page=1) |
| 監察院 | [www.cy.gov.tw](https://www.cy.gov.tw/OpenData.aspx?SN=ADE5198E06414AF5) |
| 司法院 | [www.judicial.gov.tw](https://www.judicial.gov.tw/tw/lp-1790-1-1-40.html) |
| 內政部 | [www.moi.gov.tw](https://www.moi.gov.tw/news.aspx?n=4&sms=9009) |
| 國土管理署 | [www.nlma.gov.tw](https://www.nlma.gov.tw/ch/titlelist/news) |
| 國家公園署 | [www.nps.gov.tw](https://www.nps.gov.tw/ch/titlelist/parknews) |
| 國土測繪中心 | [www.nlsc.gov.tw](https://www.nlsc.gov.tw/OpenData.aspx?SN=6880137375636088) |
| 警政署 | [www.npa.gov.tw](https://www.npa.gov.tw/ch/app/news/list?module=news&id=2139) |
| 消防署 | [www.nfa.gov.tw](https://www.nfa.gov.tw/pro/index.php?code=list&ids=1470) |
| 外交部 | [www.mofa.gov.tw](https://www.mofa.gov.tw/News.aspx?n=95&sms=73) |
| 僑委會 | [www.ocac.gov.tw](https://www.ocac.gov.tw/OCAC/Pages/List.aspx?nodeid=3018) |
| 陸委會 | [www.mac.gov.tw](https://www.mac.gov.tw/OpenData.aspx?SN=D33B55D537402BAA) |
| 國防部 | [www.mnd.gov.tw](https://www.mnd.gov.tw/news/pressreleaselist) |
| 中科院 | [www.ncsist.org.tw](https://www.ncsist.org.tw/csistdup/news/NewsPublish.aspx) |
| 國防院 | [indsr.org.tw](https://indsr.org.tw/informationlist?uid=7) |
| 財政部 | [www.mof.gov.tw](https://www.mof.gov.tw/Rss/384fb3077bb349ea973e7fc6f13b6974)<br>[www.etax.nat.gov.tw](https://www.etax.nat.gov.tw/etwmain/rss/news) |
| 金管會 | [www.fsc.gov.tw](https://www.fsc.gov.tw/RSS/Messages?serno=201202290009&language=chinese) |
| 公平會 | [www.ftc.gov.tw](https://www.ftc.gov.tw/internet/main/rss/rss_1.xml) |
| 中央銀行 | [www.cbc.gov.tw](https://www.cbc.gov.tw/tw/lp-302-1.html) |
| 主計總處 | [www.dgbas.gov.tw](https://www.dgbas.gov.tw/OpenData.aspx?SN=0DA0FD2F5416554E) |
| 人事總處 | [www.dgpa.gov.tw](https://www.dgpa.gov.tw/rsscon?uid=82) |
| 教育部 | [www.edu.tw](https://www.edu.tw/News.aspx?n=9E7AC85F1954DDA8&sms=169B8E91BB75571F) |
| 國教院 | [www.naer.edu.tw](https://www.naer.edu.tw/PageDoc?fid=15) |
| 運動部 | [www.sports.gov.tw](https://www.sports.gov.tw/News/309) |
| 法務部 | [www.moj.gov.tw](https://www.moj.gov.tw/2204/2795/2796/rss) |
| 矯正署 | [www.mjac.moj.gov.tw](https://www.mjac.moj.gov.tw/4786/4963/4965/) |
| 最高檢察署 | [www.tps.moj.gov.tw](https://www.tps.moj.gov.tw/16314/1140948/) |
| 經濟部 | [www.moea.gov.tw](https://www.moea.gov.tw/Mns/cord/news/News.aspx?kind=1&menu_id=5987) |
| 交通部 | [www.motc.gov.tw](https://www.motc.gov.tw/ch/app/news_list/query?module=news&id=14) |
| 觀光署 | [www.taiwan.net.tw](https://www.taiwan.net.tw/m1.aspx?sNo=0001001) |
| 公路局 | [www.thb.gov.tw](https://www.thb.gov.tw/News.aspx?n=12181&sms=14672) |
| 高速公路局 | [www.freeway.gov.tw](https://www.freeway.gov.tw/Rss/freewayrss2.xml) |
| 航港局 | [www.motcmpb.gov.tw](https://www.motcmpb.gov.tw/Information/RSS?SiteId=1&NodeId=15) |
| 中央氣象署 | [www.cwa.gov.tw](https://www.cwa.gov.tw/V8/C/S/news_data.html) |
| 農業部 | [www.moa.gov.tw](https://www.moa.gov.tw/open_data.php?format=rss&func=news_agri) |
| 農業金融署 | [www.afna.gov.tw](https://www.afna.gov.tw/list.php?theme=news&subtheme=news) |
| 農糧署 | [www.afa.gov.tw](https://www.afa.gov.tw/cht/index.php?act=rss&ids=309) |
| 漁業署 | [www.fa.gov.tw](https://www.fa.gov.tw/list.php?theme=Press_release) |
| 農村發展及水土保持署 | [www.ardswc.gov.tw](https://www.ardswc.gov.tw/Home/Content/RSS/press.xml) |
| 防檢署 | [www.aphia.gov.tw](https://www.aphia.gov.tw/theme_list.php?theme=NewInfoListWS) |
| 農科園區 | [www.atp.gov.tw](https://www.atp.gov.tw/CHT/Rss.aspx) |
| 衛生福利部 | [www.mohw.gov.tw](https://www.mohw.gov.tw/lp-16-1.html) |
| 食藥署 | [www.fda.gov.tw](https://www.fda.gov.tw/tc/rssNews.ashx) |
| 疾管署 | [www.cdc.gov.tw](https://www.cdc.gov.tw/Bulletin/List/MmgtpeidAR5Ooai4-fgHzQ) |
| 國健署 | [www.hpa.gov.tw](https://www.hpa.gov.tw/Pages/ashx/rsspage.ashx?nodeid=124) |
| 社家署 | [www.sfaa.gov.tw](https://www.sfaa.gov.tw/sfaa/list/5cX) |
| 勞動部 | [www.mol.gov.tw](https://www.mol.gov.tw/1607/1632/1633/) |
| 勞動力發展署 | [www.wda.gov.tw](https://www.wda.gov.tw/OpenData.aspx?SN=8C4FEB29449A1601) |
| 職業安全衛生署 | [www.osha.gov.tw](https://www.osha.gov.tw/48110/48417/48419/RssList) |
| 勞動基金運用局 | [www.blf.gov.tw](https://www.blf.gov.tw/49200/49245/49247/) |
| 文化部 | [www.moc.gov.tw](https://www.moc.gov.tw/OpenData.aspx?SN=C4E4E3A8E687AD91) |
| 故宮 | [www.npm.gov.tw](https://www.npm.gov.tw/News-List.aspx?sno=01000001&l=1&q=&s_date=&e_date=&type=03000096) |
| 數位發展部 | [moda.gov.tw](https://moda.gov.tw/press/press-releases/372) |
| 數位產業署 | [moda.gov.tw](https://moda.gov.tw/ADI/news/latest-news/766) |
| 資通安全署 | [moda.gov.tw](https://moda.gov.tw/ACS/press/news/press/820) |
| 國家資通安全研究院 | [www.nics.nat.gov.tw](https://www.nics.nat.gov.tw/latest_news/announcements/Latest_Announcement/) |
| 環境部 | [www.moenv.gov.tw](https://www.moenv.gov.tw/press/press-releases/2626.html?p=1&dc=50) |
| 國發會 | [www.ndc.gov.tw](https://www.ndc.gov.tw/Rss_News.aspx?n=114AAE178CD95D4C) |
| 國科會 | [www.nstc.gov.tw](https://www.nstc.gov.tw/folksonomy/list/9aa56881-8df0-4eb6-a5a7-32a2f72826ff?l=ch&pageSize=%EF%BC%93%EF%BC%95&pageNum=1) |
| 國家實驗研究院 | [www.niar.org.tw](https://www.niar.org.tw/xmdoc?xsmsid=0I148622737263495777) |
| 國家太空中心 | [www.tasa.org.tw](https://www.tasa.org.tw/zh-TW/announcements/news) |
| 原民會 | [www.cip.gov.tw](https://www.cip.gov.tw/zh-tw/rss/35AE118732EB6BAF/news.html) |
| 客委會 | [www.hakka.gov.tw](https://www.hakka.gov.tw/chhakka/app/data/list?id=25) |
| 海委會 | [www.oac.gov.tw](https://www.oac.gov.tw/News?language=chinese&websitedn=ch) |
| 海巡署 | [www.cga.gov.tw](https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=650&mp=999) |
| 艦隊分署 | [www.cga.gov.tw](https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=2116&mp=9997) |
| 偵防分署 | [www.cga.gov.tw](https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=10619&mp=9998) |
| 退輔會 | [www.vac.gov.tw](https://www.vac.gov.tw/lp-1788-1.html) |
| 榮總 | [www.vghtpe.gov.tw](https://www.vghtpe.gov.tw/News.action?gcode=A05) |
| 通傳會 | [api.ncc.gov.tw](https://api.ncc.gov.tw/chncc/rss/News?id=50) |
| 工程會 | [www.pcc.gov.tw](https://www.pcc.gov.tw/content/News.aspx?n=C61062639C0CD29F&sms=21EF9CF82726C1BB) |
| 中選會 | [web.cec.gov.tw](https://web.cec.gov.tw/central) |

## 抓取方式

| 類型 | 用途 | 主要實作 |
| --- | --- | --- |
| RSS／Atom／開放資料 XML | 優先使用的結構化新聞來源 | [`news_scraper/rss/`](news_scraper/rss/) |
| HTML 新聞列表 | 官方網站未提供穩定結構化來源時使用 | [`news_scraper/scrapers/`](news_scraper/scrapers/) |
| JSON／公開 API | 讀取網站公開提供的結構化資料 | 各機關 scraper 模組 |
| GraphQL | 國家太空中心公開網站資料 | [`tasa.py`](news_scraper/scrapers/ministry/development/tasa.py) |
| Selenium | 國土管理署等 JavaScript 動態頁面 | [`nlma.py`](news_scraper/scrapers/ministry/interior/nlma.py) |

## 技術參考

- [Python](https://www.python.org/)：主要程式語言。
- [Requests](https://requests.readthedocs.io/) 與 [aiohttp](https://docs.aiohttp.org/)：HTTP 同步與非同步請求。
- [Beautiful Soup](https://www.crummy.com/software/BeautifulSoup/bs4/doc/) 與 [lxml](https://lxml.de/)：HTML／XML 解析。
- [feedparser](https://feedparser.readthedocs.io/)：RSS／Atom 解析。
- [Selenium](https://www.selenium.dev/documentation/)：JavaScript 動態頁面擷取。
- [pandas](https://pandas.pydata.org/docs/) 與 [openpyxl](https://openpyxl.readthedocs.io/)：Excel 資料整理與匯出。
- [pytest](https://docs.pytest.org/)、[Ruff](https://docs.astral.sh/ruff/) 與 [Mypy](https://mypy.readthedocs.io/)：測試、程式碼檢查與型別檢查。
- [GitHub Actions](https://docs.github.com/actions)：跨 Python 版本的持續整合驗證。

## 維護來源清單

新增、移除或修改資料來源時，請同步更新：

1. [`news_scraper/config.py`](news_scraper/config.py) 的 `URLS` 與 `ORDERED_SOURCE_NAMES`。
2. [`news_scraper/scrapers/registry.py`](news_scraper/scrapers/registry.py) 的 `SCRAPER_SPECS`。
3. 本文件的官方資料來源表格。
4. 對應解析器測試或 fixture。
