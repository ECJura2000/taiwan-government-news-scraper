# 外交部擴充來源公開樣本

2026-10-12 從官方來源實際 GET（TFD 以 system Chrome headless 正常官方前端）取得，僅保留解析所需的真實日期、標題、原文連結與外框。移除圖片、導覽、報名附件及 RSS description；不含帳號、cookie、瀏覽器 profile 或個資。

路由與樣本对应見 `docs/audits/four-ministry-mofa-20261012.json` 的 `entries[].fixtures`。

- BOCA：RSS 原始20項，保留前5；HTML包含2個徵才負例及1個行動領務活動正例。兩種格式應共用徵才／採購排除規則。
- ICDF：RSS 原始2項，HTML15項；保留前2比較。志工招募負例與綠色產業新聞正例。相同xItem在RSS和HTML的ctNode不同，應以文章ID去重，且RSS必補HTML。
- IDIA：最新消息及重要活動兩個表格各保留前2，發布日期不同於標題提到的活動日期時應採發布欄位。
- AEAR：官方最新消息前2。
- TAEF：首頁/doc活動新聞2026-08-06晚於/event列表2026-06-29，需聚合兩路由；保留官方英語序數日期，不能從文章內文臆測發布日。
- TFD：官方焦點快訊及本會活動各保留前3。`/tw/news/general` 是補助公告，不當作唯一新聞來源。Vue頁必等 `li.news-item` 出現才擷取，10筆顯示上限不等於整個分類總項數。
