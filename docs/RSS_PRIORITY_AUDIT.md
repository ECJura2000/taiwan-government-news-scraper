# 88 來源 RSS 優先查核

查核日期：2026-10-12。維持 88 個來源，其中 49 個來源採官方 RSS 優先。這是設定數量；網站暫時無法連線不代表沒有 RSS。沒有核實符合原新聞欄目的 RSS 時，保留原網頁／JSON／Chrome 路徑。

查核包含原新聞頁、官網首頁及其提供的訂閱頁，並驗證實際 RSS 回應。未採用留言頻道、其他機關的共用頻道或涵蓋範圍較大的全站頻道。主計總處透過正常驗證憑證的 Chrome 成功讀取使用者提供的 RSS；不增加憑證驗證例外。司法院官方公告說明 RSS 已於 2019 年新版網站上線時停用。

行政院、客委會與交通部經同週比較確認 RSS 未包含原列表的部分新聞，固定先讀 RSS 再補查原列表。RSS 最早日期與週起始日相同時，也補查原路徑，避免當日文章被篇數限制截斷。

RSS 日期涵蓋不足時，接續原路徑並保留兩邊新聞；最後使用既有去重程序。維護頁／一般 XML 不會當成空新聞成功。

完整可核對資料：[查核 JSON](audits/rss-priority-20261012.json)。

| 來源 | RSS 優先入口／查核狀態 |
| --- | --- |
| 行政院 | [官方 RSS](https://www.ey.gov.tw/RSS_Content.aspx?ModuleType=3) |
| 監察院 | [官方 RSS](https://www.cy.gov.tw/OpenData.aspx?SN=ADE5198E06414AF5) |
| 司法院 | 未核實符合原欄目的 RSS，保留原入口 |
| 內政部 | [官方 RSS](https://www.moi.gov.tw/OpenData.aspx?SN=76F358C679FAD4CF) |
| 國土管理署 | 未核實符合原欄目的 RSS，保留原入口 |
| 國家公園署 | 未核實符合原欄目的 RSS，保留原入口 |
| 國土測繪中心 | [官方 RSS](https://www.nlsc.gov.tw/OpenData.aspx?SN=6880137375636088) |
| 警政署 | 未核實符合原欄目的 RSS，保留原入口 |
| 消防署 | 未核實符合原欄目的 RSS，保留原入口 |
| 外交部 | [官方 RSS](https://www.mofa.gov.tw/OpenData.aspx?SN=8ABA444B9AC1360F) |
| 僑委會 | 未核實符合原欄目的 RSS，保留原入口 |
| 陸委會 | [官方 RSS](https://www.mac.gov.tw/OpenData.aspx?SN=D33B55D537402BAA) |
| 國防部 | 未核實符合原欄目的 RSS，保留原入口 |
| 文策院 | 本次連線無法核實，保留原入口 |
| 中科院 | 未核實符合原欄目的 RSS，保留原入口 |
| 國防院 | 未核實符合原欄目的 RSS，保留原入口 |
| 財政部 | [官方 RSS](https://www.mof.gov.tw/Rss/384fb3077bb349ea973e7fc6f13b6974)、[官方 RSS](https://www.etax.nat.gov.tw/etwmain/rss/news) |
| 金管會 | [官方 RSS](https://www.fsc.gov.tw/RSS/Messages?serno=201202290009&language=chinese) |
| 公平會 | [官方 RSS](https://www.ftc.gov.tw/internet/main/rss/rss_1.xml) |
| 中央銀行 | [官方 RSS](https://www.cbc.gov.tw/tw/rss-302-1.xml) |
| 主計總處 | [官方 RSS](https://www.dgbas.gov.tw/OpenData.aspx?SN=C741343EFC5BCC6A&isRssFeed=1)、[官方 RSS](https://www.dgbas.gov.tw/OpenData.aspx?SN=0DA0FD2F5416554E) |
| 人事總處 | [官方 RSS](https://www.dgpa.gov.tw/rsscon?uid=82) |
| 教育部 | [官方 RSS](https://www.edu.tw/Rss_News.aspx?n=9E7AC85F1954DDA8) |
| 國教院 | 未核實符合原欄目的 RSS，保留原入口 |
| 運動部 | 本次連線無法核實，保留原入口 |
| 法務部 | [官方 RSS](https://www.moj.gov.tw/2204/2795/2796/rss) |
| 矯正署 | [官方 RSS](https://www.mjac.moj.gov.tw/4786/4963/4965/rss) |
| 最高檢察署 | 未核實符合原欄目的 RSS，保留原入口 |
| 經濟部 | [官方 RSS](https://www.moea.gov.tw/Mns/populace/news/NewsRSSdetail.aspx?Kind=1) |
| 交通部 | [官方 RSS](https://www.motc.gov.tw/ch/app/rss/News) |
| 觀光署 | [官方 RSS](https://www.taiwan.net.tw/rss.aspx?sno=0001001) |
| 公路局 | 未核實符合原欄目的 RSS，保留原入口 |
| 高速公路局 | [官方 RSS](https://www.freeway.gov.tw/Rss/freewayrss2.xml) |
| 航港局 | [官方 RSS](https://www.motcmpb.gov.tw/Information/RSS?SiteId=1&NodeId=15) |
| 中央氣象署 | 未核實符合原欄目的 RSS，保留原入口 |
| 農業部 | [官方 RSS](https://www.moa.gov.tw/open_data.php?format=rss&func=news_agri) |
| 農業金融署 | [官方 RSS](https://www.afna.gov.tw/wm_DATA.php?data=news) |
| 農糧署 | [官方 RSS](https://www.afa.gov.tw/cht/index.php?act=rss&ids=309) |
| 漁業署 | [官方 RSS](https://www.fa.gov.tw/wm_DATA.php?data=news) |
| 農村發展及水土保持署 | [官方 RSS](https://www.ardswc.gov.tw/Home/Content/RSS/press.xml) |
| 防檢署 | [官方 RSS](https://www.aphia.gov.tw/api.php?func=news&format=rss) |
| 農科園區 | [官方 RSS](https://www.atp.gov.tw/CHT/Rss.aspx) |
| 衛生福利部 | [官方 RSS](https://www.mohw.gov.tw/rss-16-1.html) |
| 食藥署 | [官方 RSS](https://www.fda.gov.tw/tc/rssNews.ashx) |
| 疾管署 | [官方 RSS](https://www.cdc.gov.tw/RSS/RssXml/Hh094B49-DRwe2RR4eFfrQ?type=1) |
| 國健署 | [官方 RSS](https://www.hpa.gov.tw/Pages/ashx/rsspage.ashx?nodeid=124) |
| 社家署 | 未核實符合原欄目的 RSS，保留原入口 |
| 勞動部 | [官方 RSS](https://www.mol.gov.tw/1607/1632/1633/RssList) |
| 勞動力發展署 | [官方 RSS](https://www.wda.gov.tw/OpenData.aspx?SN=8C4FEB29449A1601) |
| 職業安全衛生署 | [官方 RSS](https://www.osha.gov.tw/48110/48417/48419/RssList) |
| 勞動基金運用局 | 未核實符合原欄目的 RSS，保留原入口 |
| 文化部 | [官方 RSS](https://www.moc.gov.tw/OpenData.aspx?SN=C4E4E3A8E687AD91) |
| 故宮 | 未核實符合原欄目的 RSS，保留原入口 |
| 數位發展部 | 未核實符合原欄目的 RSS，保留原入口 |
| 數位產業署 | 未核實符合原欄目的 RSS，保留原入口 |
| 資通安全署 | 未核實符合原欄目的 RSS，保留原入口 |
| 國家資通安全研究院 | 未核實符合原欄目的 RSS，保留原入口 |
| 環境部 | 本次連線無法核實，保留原入口 |
| 國發會 | [官方 RSS](https://www.ndc.gov.tw/Rss_News.aspx?n=114AAE178CD95D4C) |
| 國科會 | [官方 RSS](https://www.nstc.gov.tw/nstc/rss/news) |
| 國家實驗研究院 | 未核實符合原欄目的 RSS，保留原入口 |
| 國家太空中心 | 未核實符合原欄目的 RSS，保留原入口 |
| 原民會 | [官方 RSS](https://www.cip.gov.tw/zh-tw/rss/35AE118732EB6BAF/news.html) |
| 客委會 | [官方 RSS](https://www.hakka.gov.tw/chhakka/app/rss/News) |
| 海委會 | [官方 RSS](https://www.oac.gov.tw/News?language=chinese&websitedn=ch) |
| 海巡署 | [官方 RSS](https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=650&mp=999) |
| 艦隊分署 | [官方 RSS](https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=2116&mp=9997) |
| 偵防分署 | [官方 RSS](https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=10619&mp=9998) |
| 退輔會 | [官方 RSS](https://www.vac.gov.tw/rss-1788-1.html) |
| 榮總 | 本次連線無法核實，保留原入口 |
| 通傳會 | [官方 RSS](https://api.ncc.gov.tw/chncc/rss/News?id=50) |
| 工程會 | [官方 RSS](https://www.pcc.gov.tw/content/opendata?item=news&n=CF5DF99964D9DB8E) |
| 中選會 | 未核實符合原欄目的 RSS，保留原入口 |
| 國家文化藝術基金會 | 未核實符合原欄目的 RSS，保留原入口 |
| 金門酒廠胡璉文化藝術基金會 | 未核實符合原欄目的 RSS，保留原入口 |
| 文化臺灣基金會 | 未核實符合原欄目的 RSS，保留原入口 |
| 臺灣美術基金會 | 未核實符合原欄目的 RSS，保留原入口 |
| 臺灣博物館文教基金會 | 未核實符合原欄目的 RSS，保留原入口 |
| 二二八事件紀念基金會 | 未核實符合原欄目的 RSS，保留原入口 |
| 威權統治時期國家不法行為被害者權利回復基金會 | [官方 RSS](https://www.rrf.org.tw/presses/rss) |
| 台灣建築中心 | 未核實符合原欄目的 RSS，保留原入口 |
| 藥害救濟基金會 | [官方 RSS](https://www.tdrf.org.tw/category/news/feed/) |
| 國家衛生研究院 | 未核實符合原欄目的 RSS，保留原入口 |
| 賑災基金會 | 未核實符合原欄目的 RSS，保留原入口 |
| 醫藥品查驗中心 | 未核實符合原欄目的 RSS，保留原入口 |
| 器官捐贈移植登錄及病人自主推廣中心 | 未核實符合原欄目的 RSS，保留原入口 |
| 婦女權益促進發展基金會 | 未核實符合原欄目的 RSS，保留原入口 |
| 醫院評鑑暨醫療品質策進會 | [官方 RSS](https://www.jct.org.tw/rss-5-1.xml) |

國家標準 NTP 校時的實作及限制見 [本版更新](v2.1.30-release-notes.md)。
