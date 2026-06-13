from collections.abc import Callable, Iterator, Mapping
from importlib import import_module
from typing import TypeAlias, cast

from ..models import NewsItem
from ..source_catalog import SourceSpec, build_source_catalog

Scraper: TypeAlias = Callable[[], list[NewsItem]]
ScraperSpec: TypeAlias = tuple[str, str]

SCRAPER_SPECS: dict[str, ScraperSpec] = {
    "行政院": ("news_scraper.scrapers.ministry.executive.ey", "scrape_ey_this_week"),
    "監察院": ("news_scraper.scrapers.ministry.oversight.cy", "scrape_cy_this_week"),
    "司法院": ("news_scraper.scrapers.ministry.judicial.judicial_yuan", "scrape_judicial_yuan_this_week"),
    "內政部": ("news_scraper.scrapers.ministry.interior.moi", "scrape_moi_this_week"),
    "國土管理署": ("news_scraper.scrapers.ministry.interior.nlma", "scrape_nlma_this_week"),
    "國家公園署": ("news_scraper.scrapers.ministry.interior.nps", "scrape_nps_this_week"),
    "國土測繪中心": ("news_scraper.scrapers.ministry.interior.nlsc", "scrape_nlsc_this_week"),
    "警政署": ("news_scraper.scrapers.ministry.interior.npa", "scrape_npa_this_week"),
    "消防署": ("news_scraper.scrapers.ministry.interior.nfa", "scrape_nfa_this_week"),
    "外交部": ("news_scraper.scrapers.ministry.foreign.mofa", "scrape_mofa_this_week"),
    "僑委會": ("news_scraper.scrapers.ministry.foreign.ocac", "scrape_ocac_this_week"),
    "陸委會": ("news_scraper.scrapers.ministry.foreign.mac", "scrape_mac_this_week"),
    "國防部": ("news_scraper.scrapers.ministry.defense.mnd", "scrape_mnd_this_week"),
    "中科院": ("news_scraper.scrapers.ministry.defense.ncsist", "scrape_ncsist_this_week"),
    "國防院": ("news_scraper.scrapers.ministry.defense.indsr", "scrape_indsr_this_week"),
    "財政部": ("news_scraper.scrapers.ministry.finance.mof", "scrape_mof_this_week"),
    "金管會": ("news_scraper.scrapers.ministry.finance.fsc", "scrape_fsc_this_week"),
    "公平會": ("news_scraper.scrapers.ministry.finance.ftc", "scrape_ftc_this_week"),
    "中央銀行": ("news_scraper.scrapers.ministry.finance.cbc", "scrape_cbc_this_week"),
    "主計總處": ("news_scraper.scrapers.ministry.finance.dgbas", "scrape_dgbas_this_week"),
    "教育部": ("news_scraper.scrapers.ministry.education.moe", "scrape_moe_this_week"),
    "國教院": ("news_scraper.scrapers.ministry.education.naer", "scrape_naer_this_week"),
    "運動部": ("news_scraper.scrapers.ministry.sports.sports", "scrape_sports_this_week"),
    "法務部": ("news_scraper.scrapers.ministry.justice.moj", "scrape_moj_this_week"),
    "矯正署": ("news_scraper.scrapers.ministry.justice.mjac", "scrape_mjac_this_week"),
    "最高檢察署": ("news_scraper.scrapers.ministry.justice.tps", "scrape_tps_this_week"),
    "經濟部": ("news_scraper.scrapers.ministry.economy.moea", "scrape_moea_this_week"),
    "交通部": ("news_scraper.scrapers.ministry.transport.motc", "scrape_motc_this_week"),
    "觀光署": ("news_scraper.scrapers.ministry.transport.tourism", "scrape_tourism_this_week"),
    "公路局": ("news_scraper.scrapers.ministry.transport.thb", "scrape_thb_this_week"),
    "高速公路局": ("news_scraper.scrapers.ministry.transport.freeway", "scrape_freeway_this_week"),
    "航港局": ("news_scraper.scrapers.ministry.transport.motcmpb", "scrape_motcmpb_this_week"),
    "中央氣象署": ("news_scraper.scrapers.ministry.transport.cwa", "scrape_cwa_this_week"),
    "農業部": ("news_scraper.scrapers.ministry.agriculture.moa", "scrape_moa_this_week"),
    "農業金融署": ("news_scraper.scrapers.ministry.agriculture.afna", "scrape_afna_this_week"),
    "農糧署": ("news_scraper.scrapers.ministry.agriculture.afa", "scrape_afa_this_week"),
    "漁業署": ("news_scraper.scrapers.ministry.agriculture.fa", "scrape_fa_this_week"),
    "農村發展及水土保持署": ("news_scraper.scrapers.ministry.agriculture.ardswc", "scrape_ardswc_this_week"),
    "防檢署": ("news_scraper.scrapers.ministry.agriculture.aphia", "scrape_aphia_this_week"),
    "農科園區": ("news_scraper.scrapers.ministry.agriculture.atp", "scrape_atp_this_week"),
    "衛生福利部": ("news_scraper.scrapers.ministry.health.mohw", "scrape_mohw_this_week"),
    "食藥署": ("news_scraper.scrapers.ministry.health.fda", "scrape_fda_this_week"),
    "疾管署": ("news_scraper.scrapers.ministry.health.cdc", "scrape_cdc_this_week"),
    "國健署": ("news_scraper.scrapers.ministry.health.hpa", "scrape_hpa_this_week"),
    "社家署": ("news_scraper.scrapers.ministry.health.sfaa", "scrape_sfaa_this_week"),
    "勞動部": ("news_scraper.scrapers.ministry.labor.mol", "scrape_mol_this_week"),
    "勞動力發展署": ("news_scraper.scrapers.ministry.labor.wda", "scrape_wda_this_week"),
    "職業安全衛生署": ("news_scraper.scrapers.ministry.labor.osha", "scrape_osha_this_week"),
    "勞動基金運用局": ("news_scraper.scrapers.ministry.labor.blf", "scrape_blf_this_week"),
    "文化部": ("news_scraper.scrapers.ministry.culture.moc", "scrape_moc_this_week"),
    "故宮": ("news_scraper.scrapers.ministry.culture.npm", "scrape_npm_this_week"),
    "數位發展部": ("news_scraper.scrapers.ministry.digital.moda", "scrape_moda_this_week"),
    "數位產業署": ("news_scraper.scrapers.ministry.digital.moda", "scrape_adi_this_week"),
    "資通安全署": ("news_scraper.scrapers.ministry.digital.moda", "scrape_acs_this_week"),
    "國家資通安全研究院": ("news_scraper.scrapers.ministry.digital.moda", "scrape_nics_this_week"),
    "環境部": ("news_scraper.scrapers.ministry.environment.moenv", "scrape_moenv_this_week"),
    "國發會": ("news_scraper.scrapers.ministry.development.ndc", "scrape_ndc_this_week"),
    "國科會": ("news_scraper.scrapers.ministry.development.nstc", "scrape_nstc_this_week"),
    "國家實驗研究院": ("news_scraper.scrapers.ministry.development.niar", "scrape_niar_this_week"),
    "國家太空中心": ("news_scraper.scrapers.ministry.development.tasa", "scrape_tasa_this_week"),
    "原民會": ("news_scraper.scrapers.ministry.communities.cip", "scrape_cip_this_week"),
    "客委會": ("news_scraper.scrapers.ministry.communities.hakka", "scrape_hakka_this_week"),
    "海委會": ("news_scraper.scrapers.ministry.oceans.oac", "scrape_oac_this_week"),
    "海巡署": ("news_scraper.scrapers.ministry.oceans.cga", "scrape_cga_this_week"),
    "艦隊分署": ("news_scraper.scrapers.ministry.oceans.fleet", "scrape_cga_fleet_this_week"),
    "偵防分署": ("news_scraper.scrapers.ministry.oceans.investigation", "scrape_cga_investigation_this_week"),
    "退輔會": ("news_scraper.scrapers.ministry.veterans.vac", "scrape_vac_this_week"),
    "榮總": ("news_scraper.scrapers.ministry.veterans.vghtpe", "scrape_vghtpe_this_week"),
    "通傳會": ("news_scraper.scrapers.ministry.regulators.ncc", "scrape_ncc_this_week"),
    "工程會": ("news_scraper.scrapers.ministry.regulators.pcc", "scrape_pcc_this_week"),
    "中選會": ("news_scraper.scrapers.ministry.regulators.cec", "scrape_cec_this_week"),
}


SOURCE_SPECS = build_source_catalog(SCRAPER_SPECS)


class LazyScraperRegistry(Mapping[str, Scraper]):
    def __init__(self, source_specs: Mapping[str, SourceSpec]) -> None:
        self._source_specs = dict(source_specs)
        self._cache: dict[str, Scraper] = {}

    def __getitem__(self, source_name: str) -> Scraper:
        if source_name not in self._source_specs:
            raise KeyError(source_name)
        cached_scraper = self._cache.get(source_name)
        if cached_scraper is not None:
            return cached_scraper

        spec = self._source_specs[source_name]
        module = import_module(spec.module)
        scraper_func = cast(Scraper, getattr(module, spec.function))
        self._cache[source_name] = scraper_func
        return scraper_func

    def __iter__(self) -> Iterator[str]:
        return iter(self._source_specs)

    def __len__(self) -> int:
        return len(self._source_specs)


SCRAPER_REGISTRY = LazyScraperRegistry(SOURCE_SPECS)
