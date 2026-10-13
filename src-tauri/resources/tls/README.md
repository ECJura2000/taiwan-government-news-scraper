此處只有公開官方中繼憑證，不含私鑰。

`twca-secure-2023.der` 是 TWCA Secure SSL Certification Authority 官方 AIA 憑證：
https://sslserver.twca.com.tw/cacert/secure_sha2_2023G3.crt

SHA-256：`1a2c75fd096e0499e9ff6ac74e526f61eaae3edfc8c2ea4436fee0c24d8b7d0e`。

2026-10-13 查核消防署、農科園區及國健署的伺服器鏈缺少此中繼。Linux 原始驗證拒絕連線，補中繼後可驗證到既有系統信任根。使用者明確核准僅為 `www.nfa.gov.tw`、`www.atp.gov.tw`、`www.hpa.gov.tw` 啟用；主機限制在來源傳輸政策，HTTP 用戶端接受已解析政策，macOS／Windows 不套用。

沒有關閉有效期限、主機名稱或信任根驗證。補鏈用戶端不得跨主機或降為 HTTP 跳轉。Linux `tls_intermediate.rs` 實際握手證明缺中繼、缺信任根、主機不符都遭拒絕，完整鏈才通過；另確認過期鏈遭拒絕。國防部既有受限 invalid-certificate fallback 維持原設定。
