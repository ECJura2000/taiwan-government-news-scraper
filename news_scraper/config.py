import hashlib
import json
import re
from dataclasses import asdict, dataclass
from pathlib import Path

from .runtime_config import (  # noqa: F401
    AFNA_LIST_TIMEOUT,
    APHIA_RSS_TIMEOUT,
    ASYNC_MAX_CONNECTIONS,
    ASYNC_MAX_PER_HOST,
    ASYNC_PAGE_BATCH_WORKERS,
    ASYNC_PAGE_TIMEOUT,
    ATP_RSS_TIMEOUT,
    CDC_RSS_TIMEOUT,
    CWA_JS_TIMEOUT,
    FAILED_SOURCE_RETRY_TIMEOUT_EXTRA_SECONDS,
    FA_LIST_TIMEOUT,
    FDA_RSS_TIMEOUT,
    HPA_RSS_TIMEOUT,
    INDSR_LIST_TIMEOUT,
    JUDICIAL_LIST_TIMEOUT,
    JUDICIAL_MAX_PAGES,
    LIST_PAGE_TIMEOUT,
    MAC_RSS_TIMEOUT,
    MAX_WORKERS,
    MJAC_LIST_TIMEOUT,
    MOA_RSS_TIMEOUT,
    MOEA_RSS_TIMEOUT,
    MOF_PAGE_SIZE,
    MOF_RSS_TIMEOUT,
    MOHW_LIST_TIMEOUT,
    MOI_MAX_PAGES,
    MOJ_DETAIL_CHECK_LIMIT_PER_PAGE,
    MOJ_DETAIL_WORKERS,
    MOJ_MAX_PAGES,
    MOJ_OLD_NEWS_STREAK_LIMIT,
    MOJ_PAGE_WORKERS,
    MOTC_LIST_TIMEOUT,
    NAER_LIST_TIMEOUT,
    NAER_MAX_PAGES,
    NCSIST_LIST_TIMEOUT,
    NFA_LIST_TIMEOUT,
    NIAR_LIST_TIMEOUT,
    NLMA_LIST_TIMEOUT,
    NPA_TIMEOUT,
    NPS_JSON_TIMEOUT,
    PAGED_SITE_WORKERS,
    PARSER,
    PCC_TIMEOUT,
    REQUEST_TIMEOUT,
    RETRY_BACKOFF_FACTOR,
    RETRY_TOTAL,
    RSS_FEED_TIMEOUT,
    SFAA_LIST_TIMEOUT,
    SPORTS_RSS_TIMEOUT,
    TASA_GRAPHQL_TIMEOUT,
    THB_LIST_TIMEOUT,
    TOURISM_LIST_TIMEOUT,
    TPS_TIMEOUT,
    VGHTPE_LIST_TIMEOUT,
    WDA_RSS_TIMEOUT,
)

URLS: dict[str, str] = {
    "數位發展部": "https://moda.gov.tw/press/press-releases/372",
    "數位產業署": "https://moda.gov.tw/ADI/news/latest-news/766",
    "資通安全署": "https://moda.gov.tw/ACS/press/news/press/820",
    "國家資通安全研究院": "https://www.nics.nat.gov.tw/latest_news/announcements/Latest_Announcement/",
    "行政院": "https://www.ey.gov.tw/Page/6485009ABEC1CB9C?PS=50&page=1",
    "監察院": "https://www.cy.gov.tw/OpenData.aspx?SN=ADE5198E06414AF5",
    "司法院": "https://www.judicial.gov.tw/tw/lp-1790-1-1-40.html",
    "文化部": "https://www.moc.gov.tw/OpenData.aspx?SN=C4E4E3A8E687AD91",
    "財政部": "https://www.mof.gov.tw/Rss/384fb3077bb349ea973e7fc6f13b6974",
    "外交部": "https://www.mofa.gov.tw/News.aspx?n=95&sms=73",
    "僑委會": "https://www.ocac.gov.tw/OCAC/Pages/List.aspx?nodeid=3018",
    "退輔會": "https://www.vac.gov.tw/lp-1788-1.html",
    "榮總": "https://www.vghtpe.gov.tw/News.action?gcode=A05",
    "中央銀行": "https://www.cbc.gov.tw/tw/lp-302-1.html",
    "主計總處": "https://www.dgbas.gov.tw/OpenData.aspx?SN=0DA0FD2F5416554E",
    "人事總處": "https://www.dgpa.gov.tw/rsscon?uid=82",
    "客委會": "https://www.hakka.gov.tw/chhakka/app/data/list?id=25",
    "原民會": "https://www.cip.gov.tw/zh-tw/rss/35AE118732EB6BAF/news.html",
    "陸委會": "https://www.mac.gov.tw/OpenData.aspx?SN=D33B55D537402BAA",
    "海委會": "https://www.oac.gov.tw/News?language=chinese&websitedn=ch",
    "海巡署": "https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=650&mp=999",
    "艦隊分署": "https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=2116&mp=9997",
    "偵防分署": "https://www.cga.gov.tw/GipOpen/wSite/rss?ctNode=10619&mp=9998",
    "金管會": "https://www.fsc.gov.tw/RSS/Messages?serno=201202290009&language=chinese",
    "工程會": "https://www.pcc.gov.tw/content/News.aspx?n=C61062639C0CD29F&sms=21EF9CF82726C1BB",
    "故宮": "https://www.npm.gov.tw/News-List.aspx?sno=01000001&l=1&q=&s_date=&e_date=&type=03000096",
    "中選會": "https://web.cec.gov.tw/central",
    "環境部": "https://www.moenv.gov.tw/press/press-releases/2626.html?p=1&dc=50",
    "國發會": "https://www.ndc.gov.tw/Rss_News.aspx?n=114AAE178CD95D4C",
    "國科會": "https://www.nstc.gov.tw/folksonomy/list/9aa56881-8df0-4eb6-a5a7-32a2f72826ff?l=ch&pageSize=%EF%BC%93%EF%BC%95&pageNum=1",
    "國家實驗研究院": "https://www.niar.org.tw/xmdoc?xsmsid=0I148622737263495777",
    "國家太空中心": "https://www.tasa.org.tw/zh-TW/announcements/news",
    "經濟部": "https://www.moea.gov.tw/Mns/cord/news/News.aspx?kind=1&menu_id=5987",
    "農業部": "https://www.moa.gov.tw/open_data.php?format=rss&func=news_agri",
    "農業金融署": "https://www.afna.gov.tw/list.php?theme=news&subtheme=news",
    "農糧署": "https://www.afa.gov.tw/cht/index.php?act=rss&ids=309",
    "交通部": "https://www.motc.gov.tw/ch/app/news_list/query?module=news&id=14",
    "觀光署": "https://www.taiwan.net.tw/m1.aspx?sNo=0001001",
    "公路局": "https://www.thb.gov.tw/News.aspx?n=12181&sms=14672",
    "高速公路局": "https://www.freeway.gov.tw/Rss/freewayrss2.xml",
    "航港局": "https://www.motcmpb.gov.tw/Information/RSS?SiteId=1&NodeId=15",
    "中央氣象署": "https://www.cwa.gov.tw/V8/C/S/news_data.html",
    "法務部": "https://www.moj.gov.tw/2204/2795/2796/rss",
    "矯正署": "https://www.mjac.moj.gov.tw/4786/4963/4965/",
    "內政部": "https://www.moi.gov.tw/news.aspx?n=4&sms=9009",
    "國土管理署": "https://www.nlma.gov.tw/ch/titlelist/news",
    "國家公園署": "https://www.nps.gov.tw/ch/titlelist/parknews",
    "國土測繪中心": "https://www.nlsc.gov.tw/OpenData.aspx?SN=6880137375636088",
    "警政署": "https://www.npa.gov.tw/ch/app/news/list?module=news&id=2139",
    "衛生福利部": "https://www.mohw.gov.tw/lp-16-1.html",
    "勞動部": "https://www.mol.gov.tw/1607/1632/1633/",
    "勞動力發展署": "https://www.wda.gov.tw/OpenData.aspx?SN=8C4FEB29449A1601",
    "職業安全衛生署": "https://www.osha.gov.tw/48110/48417/48419/RssList",
    "勞動基金運用局": "https://www.blf.gov.tw/49200/49245/49247/",
    "國防部": "https://www.mnd.gov.tw/news/pressreleaselist",
    "中科院": "https://www.ncsist.org.tw/csistdup/news/NewsPublish.aspx",
    "國防院": "https://indsr.org.tw/informationlist?uid=7",
    "消防署": "https://www.nfa.gov.tw/pro/index.php?code=list&ids=1470",
    "教育部": "https://www.edu.tw/News.aspx?n=9E7AC85F1954DDA8&sms=169B8E91BB75571F",
    "國教院": "https://www.naer.edu.tw/PageDoc?fid=15",
    "漁業署": "https://www.fa.gov.tw/list.php?theme=Press_release",
    "農村發展及水土保持署": "https://www.ardswc.gov.tw/Home/Content/RSS/press.xml",
    "最高檢察署": "https://www.tps.moj.gov.tw/16314/1140948/",
    "運動部": "https://www.sports.gov.tw/News/309",
    "公平會": "https://www.ftc.gov.tw/internet/main/rss/rss_1.xml",
    "通傳會": "https://api.ncc.gov.tw/chncc/rss/News?id=50",
    "防檢署": "https://www.aphia.gov.tw/theme_list.php?theme=NewInfoListWS",
    "食藥署": "https://www.fda.gov.tw/tc/rssNews.ashx",
    "農科園區": "https://www.atp.gov.tw/CHT/Rss.aspx",
    "疾管署": "https://www.cdc.gov.tw/Bulletin/List/MmgtpeidAR5Ooai4-fgHzQ",
    "國健署": "https://www.hpa.gov.tw/Pages/ashx/rsspage.ashx?nodeid=124",
    "社家署": "https://www.sfaa.gov.tw/sfaa/list/5cX",
}

SOURCE_URL_SETS: dict[str, tuple[str, ...]] = {
    "財政部": (
        "https://www.mof.gov.tw/Rss/384fb3077bb349ea973e7fc6f13b6974",
        "https://www.etax.nat.gov.tw/etwmain/rss/news",
    ),
}


def get_source_url(source: str) -> str:
    return URLS[source]


def get_source_urls(source: str) -> tuple[str, ...]:
    return SOURCE_URL_SETS.get(source, (get_source_url(source),))

ORDERED_SOURCE_NAMES = [
    "行政院",
    "監察院",
    "司法院",
    "內政部",
    "國土管理署",
    "國家公園署",
    "國土測繪中心",
    "警政署",
    "消防署",
    "外交部",
    "僑委會",
    "陸委會",
    "國防部",
    "中科院",
    "國防院",
    "財政部",
    "金管會",
    "公平會",
    "中央銀行",
    "主計總處",
    "人事總處",
    "教育部",
    "國教院",
    "運動部",
    "法務部",
    "矯正署",
    "最高檢察署",
    "經濟部",
    "交通部",
    "觀光署",
    "公路局",
    "高速公路局",
    "航港局",
    "中央氣象署",
    "農業部",
    "農業金融署",
    "農糧署",
    "漁業署",
    "農村發展及水土保持署",
    "防檢署",
    "農科園區",
    "衛生福利部",
    "食藥署",
    "疾管署",
    "國健署",
    "社家署",
    "勞動部",
    "勞動力發展署",
    "職業安全衛生署",
    "勞動基金運用局",
    "文化部",
    "故宮",
    "數位發展部",
    "數位產業署",
    "資通安全署",
    "國家資通安全研究院",
    "環境部",
    "國發會",
    "國科會",
    "國家實驗研究院",
    "國家太空中心",
    "原民會",
    "客委會",
    "海委會",
    "海巡署",
    "艦隊分署",
    "偵防分署",
    "退輔會",
    "榮總",
    "通傳會",
    "工程會",
    "中選會",
]

SOURCE_ORDER = {source_name: idx for idx, source_name in enumerate(ORDERED_SOURCE_NAMES, 1)}

SCRAPE_DIFFICULTY_ORDER = {
    "監察院": 10,
    "文化部": 10,
    "財政部": 10,
    "金管會": 10,
    "公平會": 10,
    "原民會": 10,
    "海巡署": 10,
    "艦隊分署": 10,
    "偵防分署": 10,
    "高速公路局": 10,
    "航港局": 10,
    "法務部": 10,
    "職業安全衛生署": 10,
    "農糧署": 10,
    "農村發展及水土保持署": 10,
    "食藥署": 10,
    "通傳會": 10,
    "農科園區": 10,
    "行政院": 20,
    "國家公園署": 20,
    "陸委會": 20,
    "中央氣象署": 20,
    "國家資通安全研究院": 20,
    "國家太空中心": 20,
    "中選會": 20,
    "國土測繪中心": 30,
    "內政部": 30,
    "外交部": 30,
    "僑委會": 30,
    "國防部": 30,
    "中科院": 30,
    "國防院": 30,
    "中央銀行": 30,
    "主計總處": 30,
    "人事總處": 10,
    "教育部": 30,
    "警政署": 30,
    "環境部": 30,
    "國發會": 30,
    "國科會": 30,
    "客委會": 30,
    "退輔會": 30,
    "故宮": 30,
    "數位發展部": 30,
    "數位產業署": 30,
    "資通安全署": 30,
    "農業部": 30,
    "農業金融署": 30,
    "漁業署": 30,
    "防檢署": 30,
    "疾管署": 30,
    "勞動基金運用局": 30,
    "觀光署": 40,
    "交通部": 40,
    "衛生福利部": 40,
    "勞動部": 40,
    "國教院": 40,
    "消防署": 50,
    "矯正署": 50,
    "最高檢察署": 50,
    "海委會": 50,
    "工程會": 60,
    "榮總": 60,
    "國家實驗研究院": 60,
    "國健署": 60,
    "社家署": 60,
    "勞動力發展署": 60,
    "司法院": 70,
    "運動部": 80,
    "經濟部": 80,
    "公路局": 90,
    "國土管理署": 100,
}

AFFILIATED_SOURCE_PATHS = {
    "國土管理署": ("內政部", "國土管理署"),
    "國家公園署": ("內政部", "國家公園署"),
    "國土測繪中心": ("內政部", "國土測繪中心"),
    "警政署": ("內政部", "警政署"),
    "消防署": ("內政部", "消防署"),
    "中科院": ("國防部", "中科院"),
    "國防院": ("國防部", "國防院"),
    "國教院": ("教育部", "國教院"),
    "矯正署": ("法務部", "矯正署"),
    "最高檢察署": ("法務部", "最高檢察署"),
    "觀光署": ("交通部", "觀光署"),
    "公路局": ("交通部", "公路局"),
    "高速公路局": ("交通部", "高速公路局"),
    "航港局": ("交通部", "航港局"),
    "中央氣象署": ("交通部", "中央氣象署"),
    "農業金融署": ("農業部", "農業金融署"),
    "農糧署": ("農業部", "農糧署"),
    "漁業署": ("農業部", "漁業署"),
    "農村發展及水土保持署": ("農業部", "農村發展及水土保持署"),
    "防檢署": ("農業部", "防檢署"),
    "農科園區": ("農業部", "農科園區"),
    "食藥署": ("衛生福利部", "食藥署"),
    "疾管署": ("衛生福利部", "疾管署"),
    "國健署": ("衛生福利部", "國健署"),
    "社家署": ("衛生福利部", "社家署"),
    "勞動力發展署": ("勞動部", "勞動力發展署"),
    "職業安全衛生署": ("勞動部", "職業安全衛生署"),
    "勞動基金運用局": ("勞動部", "勞動基金運用局"),
    "數位產業署": ("數位發展部", "數位產業署"),
    "資通安全署": ("數位發展部", "資通安全署"),
    "國家資通安全研究院": ("數位發展部", "國家資通安全研究院"),
    "國家實驗研究院": ("國科會", "國家實驗研究院"),
    "國家太空中心": ("國科會", "國家太空中心"),
    "海巡署": ("海委會", "海巡署"),
    "艦隊分署": ("海委會", "海巡署", "艦隊分署"),
    "偵防分署": ("海委會", "海巡署", "偵防分署"),
    "榮總": ("退輔會", "榮總"),
}


def build_affiliated_groups(source_paths):
    grouped_members: dict[str, set[str]] = {}
    for source_name, path in source_paths.items():
        if not path:
            continue
        parent_source = path[0]
        grouped_members.setdefault(parent_source, {parent_source})
        grouped_members[parent_source].add(source_name)

    return {
        parent_source: {
            "members": [source_name for source_name in ORDERED_SOURCE_NAMES if source_name in members],
            "priority": {
                source_name: 1 if source_name == parent_source else 0
                for source_name in members
            },
        }
        for parent_source, members in grouped_members.items()
    }


AFFILIATED_GROUPS = build_affiliated_groups(AFFILIATED_SOURCE_PATHS)

HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/135.0.0.0 Safari/537.36"
    ),
    "Accept": (
        "text/html,application/xhtml+xml,application/xml;"
        "q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8"
    ),
    "Accept-Language": "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7",
    "Cache-Control": "no-cache",
    "Pragma": "no-cache",
    "Connection": "keep-alive",
    "Upgrade-Insecure-Requests": "1",
}

BASE_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = BASE_DIR.parent
DEFAULT_OUTPUT_DIR = PROJECT_ROOT / "新聞搜集區"

SOURCE_ALIASES = {
    "數發部": "數位發展部",
    "原住民族委員會": "原民會",
    "客家委員會": "客委會",
    "國家發展委員會": "國發會",
    "行政院公共工程委員會": "工程會",
    "國家科學及技術委員會": "國科會",
    "行政院主計總處": "主計總處",
    "行政院人事行政總處": "人事總處",
    "大陸委員會": "陸委會",
    "國立故宮博物院": "故宮",
    "金融監督管理委員會": "金管會",
    "中央選舉委員會": "中選會",
    "海洋委員會": "海委會",
    "公平交易委員會": "公平會",
    "僑務委員會": "僑委會",
    "國家通訊傳播委員會": "通傳會",
    "國軍退除役官兵輔導委員會": "退輔會",
}

AI_POLICY_GENERAL_KEYWORDS = (
    "AI",
    "人工智慧",
    "生成式AI",
    "生成式人工智慧",
    "大語言模型",
    "大型語言模型",
    "LLM",
)

AI_POLICY_NEGATIVE_KEYWORDS = (
    "徵才",
    "招募",
    "職缺",
    "採購",
    "招標",
    "決標",
    "轉知",
)
AI_POLICY_HIGH_SCORE = 80
AI_POLICY_POSSIBLE_SCORE = 40
AI_POLICY_NEGATIVE_PENALTY = 50
AI_POLICY_RULESET_VERSION = "2.1.0"


@dataclass(frozen=True, slots=True)
class AIPolicyInitiative:
    name: str
    lead_agency: str
    lead_source: str
    exact_phrases: tuple[str, ...]
    strong_keywords: tuple[str, ...]
    context_keywords: tuple[str, ...]


def validate_ai_policy_initiatives(
    initiatives: tuple[AIPolicyInitiative, ...],
) -> tuple[AIPolicyInitiative, ...]:
    errors: list[str] = []
    seen_names: set[str] = set()
    for initiative in initiatives:
        if not initiative.name or initiative.name in seen_names:
            errors.append("建設名稱空白或重複：{}".format(initiative.name))
        seen_names.add(initiative.name)
        if initiative.lead_source not in SOURCE_ORDER:
            errors.append("{} 的主政來源不存在：{}".format(initiative.name, initiative.lead_source))

        normalized_keywords: list[str] = []
        for field_name in ("exact_phrases", "strong_keywords", "context_keywords"):
            values = getattr(initiative, field_name)
            if not values:
                errors.append("{} 缺少 {}".format(initiative.name, field_name))
            normalized_keywords.extend(
                re.sub(r"\s+", "", str(keyword)).lower()
                for keyword in values
                if str(keyword).strip()
            )
        if len(normalized_keywords) != len(set(normalized_keywords)):
            errors.append("{} 具有重複關鍵字".format(initiative.name))

    if errors:
        raise RuntimeError("AI 新十大建設規則設定錯誤：{}".format("；".join(errors)))
    return tuple(initiatives)

# 精準詞可單獨判定高度相關；輔助詞必須搭配 AI 語境或主政部會才納入。
AI_POLICY_INITIATIVES = validate_ai_policy_initiatives((
    AIPolicyInitiative(
        name="全民智慧生活圈",
        lead_agency="國科會",
        lead_source="國科會",
        exact_phrases=("全民智慧生活圈",),
        strong_keywords=("智慧生活圈",),
        context_keywords=(
            "智慧交通", "智慧醫療", "AI醫療", "智慧健康", "AI健康", "智慧建築", "智慧電網",
            "智慧照護", "AI照護", "智慧防災", "韌性防災", "防災救難", "災防",
        ),
    ),
    AIPolicyInitiative(
        name="百工百業智慧應用",
        lead_agency="經濟部",
        lead_source="經濟部",
        exact_phrases=("百工百業智慧應用",),
        strong_keywords=("產業AI化", "產業AI轉型"),
        context_keywords=(
            "百工百業", "中小微型企業", "中小企業", "智慧製造", "智慧製程",
            "產業轉型", "產業升級",
        ),
    ),
    AIPolicyInitiative(
        name="AI數位產業登峰",
        lead_agency="數發部",
        lead_source="數位發展部",
        exact_phrases=("AI數位產業登峰",),
        strong_keywords=("AI數位產業", "利基型AI平台", "利基型AI平臺", "AI服務輸出"),
        context_keywords=(
            "Team Taiwan", "AI服務", "AI軟體", "AI模型", "資服業", "軟體服務",
            "開源", "國際輸出",
        ),
    ),
    AIPolicyInitiative(
        name="矽光子技術全球領先",
        lead_agency="經濟部",
        lead_source="經濟部",
        exact_phrases=("矽光子技術全球領先",),
        strong_keywords=("矽光子", "矽光子共同封裝"),
        context_keywords=("CPO", "共同封裝", "光電整合", "高速傳輸", "低功耗傳輸"),
    ),
    AIPolicyInitiative(
        name="全球量子能力登頂",
        lead_agency="國科會",
        lead_source="國科會",
        exact_phrases=("全球量子能力登頂",),
        strong_keywords=("量子位元", "量子晶片", "量子運算", "量子科技", "後量子密碼"),
        context_keywords=("量子", "量子研發實驗室", "量子產業鏈"),
    ),
    AIPolicyInitiative(
        name="全球AI機器人供應鏈樞紐",
        lead_agency="國科會",
        lead_source="國科會",
        exact_phrases=("全球AI機器人供應鏈樞紐",),
        strong_keywords=("AI機器人", "人形機器人", "智慧機器人", "機器人供應鏈"),
        context_keywords=(
            "機器人研發", "機器人產業", "機器人零組件", "自主機器人",
            "服務型機器人", "機器人樞紐",
        ),
    ),
    AIPolicyInitiative(
        name="主權AI及算力建設",
        lead_agency="國科會",
        lead_source="國科會",
        exact_phrases=("主權AI及算力建設",),
        strong_keywords=("主權AI", "AI算力", "算力建設", "國家算力", "AI超級電腦"),
        context_keywords=("GPU", "先進運算系統", "雲端開發環境", "訓練語料庫", "語料庫"),
    ),
    AIPolicyInitiative(
        name="智慧政府與資料治理",
        lead_agency="數發部",
        lead_source="數位發展部",
        exact_phrases=("智慧政府與資料治理",),
        strong_keywords=("智慧政府", "資料治理", "數據治理", "政府AI治理"),
        context_keywords=(
            "AI治理", "跨域資料整合", "資料匯流", "政府資料", "資料標準", "開放資料",
        ),
    ),
    AIPolicyInitiative(
        name="AI人才生態系引領國際",
        lead_agency="國發會",
        lead_source="國發會",
        exact_phrases=("AI人才生態系引領國際",),
        strong_keywords=(
            "AI人才生態系", "AI人才", "人工智慧人才", "AI教育", "AI素養",
            "AI公務人才", "AI共通核心能力",
        ),
        context_keywords=("跨域學習", "產業實作人才", "海外專業人才", "人才培育"),
    ),
    AIPolicyInitiative(
        name="千億資金驅動創新",
        lead_agency="國發會",
        lead_source="國發會",
        exact_phrases=("千億資金驅動創新",),
        strong_keywords=("AI創新基金", "AI投資", "創新資金", "資金點火"),
        context_keywords=("千億資金", "國發基金", "創業投資", "民間投資", "創新投資"),
    ),
))


def get_ai_policy_ruleset_hash() -> str:
    payload = {
        "version": AI_POLICY_RULESET_VERSION,
        "initiatives": [asdict(initiative) for initiative in AI_POLICY_INITIATIVES],
        "general_keywords": AI_POLICY_GENERAL_KEYWORDS,
        "negative_keywords": AI_POLICY_NEGATIVE_KEYWORDS,
        "thresholds": {
            "high": AI_POLICY_HIGH_SCORE,
            "possible": AI_POLICY_POSSIBLE_SCORE,
            "negative_penalty": AI_POLICY_NEGATIVE_PENALTY,
        },
    }
    encoded = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()[:16]

# 保留既有布林篩選 API；實際 Excel 分級由結構化建設規則判定。
AI_POLICY_KEYWORDS = tuple(
    dict.fromkeys(
        AI_POLICY_GENERAL_KEYWORDS
        + tuple(
            keyword
            for initiative in AI_POLICY_INITIATIVES
            for field in (initiative.exact_phrases, initiative.strong_keywords, initiative.context_keywords)
            for keyword in field
        )
    )
)

SSL_FALLBACK_HOSTS = {
    "moda.gov.tw",
    "www.ey.gov.tw",
    "www.mof.gov.tw",
    "www.etax.nat.gov.tw",
    "www.mofa.gov.tw",
    "www.ocac.gov.tw",
    "www.vac.gov.tw",
    "www.cbc.gov.tw",
    "www.dgbas.gov.tw",
    "www.dgpa.gov.tw",
    "www.hakka.gov.tw",
    "www.cip.gov.tw",
    "www.mac.gov.tw",
    "www.oac.gov.tw",
    "www.cga.gov.tw",
    "www.fsc.gov.tw",
    "www.pcc.gov.tw",
    "www.npm.gov.tw",
    "web.cec.gov.tw",
    "www.moenv.gov.tw",
    "www.ndc.gov.tw",
    "www.nstc.gov.tw",
    "www.niar.org.tw",
    "www.tasa.org.tw",
    "www.nics.nat.gov.tw",
    "www.moea.gov.tw",
    "www.moa.gov.tw",
    "www.afna.gov.tw",
    "www.afa.gov.tw",
    "www.motc.gov.tw",
    "www.taiwan.net.tw",
    "www.thb.gov.tw",
    "www.freeway.gov.tw",
    "www.motcmpb.gov.tw",
    "www.cwa.gov.tw",
    "www.moj.gov.tw",
    "www.mjac.moj.gov.tw",
    "www.moi.gov.tw",
    "www.nlma.gov.tw",
    "www.nps.gov.tw",
    "www.nlsc.gov.tw",
    "www.npa.gov.tw",
    "www.mohw.gov.tw",
    "www.mol.gov.tw",
    "www.wda.gov.tw",
    "www.osha.gov.tw",
    "www.blf.gov.tw",
    "www.mnd.gov.tw",
    "www.fa.gov.tw",
    "www.ardswc.gov.tw",
    "www.ncsist.org.tw",
    "indsr.org.tw",
    "www.nfa.gov.tw",
    "www.edu.tw",
    "www.naer.edu.tw",
    "www.tps.moj.gov.tw",
    "www.ftc.gov.tw",
    "api.ncc.gov.tw",
    "www.aphia.gov.tw",
    "www.fda.gov.tw",
    "www.atp.gov.tw",
    "www.cdc.gov.tw",
    "www.hpa.gov.tw",
    "www.sfaa.gov.tw",
    "www.judicial.gov.tw",
    "www.niar.org.tw",
}
