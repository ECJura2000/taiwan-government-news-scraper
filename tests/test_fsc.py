from news_scraper.scrapers.ministry.finance.fsc import (
    extract_fsc_contact_units,
    resolve_fsc_department,
)


def test_extract_fsc_contact_units_from_rss_description():
    description = """
    <div>聯絡單位：證券期貨局證券發行組　楊科長　聯絡電話：(02)2774-7232<br />
    聯絡單位：證券期貨局投信投顧組 陳科長 聯絡電話：(02)2774-7445<br />
    聯絡單位：證券期貨局證券發行組 楊科長 聯絡電話：(02)2774-7232</div>
    """

    assert extract_fsc_contact_units(description) == [
        "證券期貨局證券發行組",
        "證券期貨局投信投顧組",
    ]


def test_resolve_fsc_department_prefers_contact_units():
    department = resolve_fsc_department(
        {
            "description": "聯絡單位：證券期貨局會計審計組 黃科長 聯絡電話：(02)2774-7124",
            "department_all_name": "金融監督管理委員會",
            "deptname": "金融監督管理委員會",
        }
    )

    assert department == "金管會／證券期貨局會計審計組"
