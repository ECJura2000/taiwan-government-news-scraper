import re

from ....config import URLS
from ....utils.text import build_department_label, clean_text
from ...base import scrape_standard_rss_this_week

try:
    from bs4 import BeautifulSoup
except ImportError:
    BeautifulSoup = None

FSC_DEPARTMENT_ALIASES = {"金融監督管理委員會", "行政院金融監督管理委員會"}
FSC_CONTACT_PERSON_TITLE_RE = r"(?:副)?(?:科長|組長|主任|專員|秘書|視察|科員|技正|稽核|先生|小姐)$"


def html_to_text(html):
    if BeautifulSoup is None:
        return clean_text(html)
    return clean_text(BeautifulSoup(html or "", "html.parser").get_text(" ", strip=True))


def normalize_fsc_contact_unit(raw_unit):
    unit = clean_text(raw_unit).strip(" ，,、;；")
    if not unit:
        return ""

    parts = [part for part in unit.split(" ") if part]
    if len(parts) > 1 and re.search(FSC_CONTACT_PERSON_TITLE_RE, parts[-1]):
        unit = " ".join(parts[:-1])

    return clean_text(unit).strip(" ，,、;；")


def extract_fsc_contact_units(description):
    text = html_to_text(description)
    if not text:
        return []

    units = []
    seen = set()
    pattern = re.compile(r"聯絡單位\s*[:：]\s*(.*?)(?=\s*聯絡電話\s*[:：]|$)")
    for match in pattern.finditer(text):
        unit = normalize_fsc_contact_unit(match.group(1))
        if unit and unit not in seen:
            units.append(unit)
            seen.add(unit)
    return units


def resolve_fsc_department(fields):
    contact_units = extract_fsc_contact_units(fields.get("description", ""))
    if contact_units:
        return build_department_label("金管會", "、".join(contact_units), aliases=FSC_DEPARTMENT_ALIASES)

    return build_department_label(
        "金管會",
        fields.get("department_all_name", "") or fields["deptname"],
        aliases=FSC_DEPARTMENT_ALIASES,
    )


def scrape_fsc_this_week():
    return scrape_standard_rss_this_week(
        "金管會",
        URLS["金管會"],
        department_aliases=FSC_DEPARTMENT_ALIASES,
        department_resolver=resolve_fsc_department,
    )
