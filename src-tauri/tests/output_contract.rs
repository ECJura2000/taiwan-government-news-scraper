use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

#[test]
fn cli_preserves_failures_and_publishes_matching_excel_json_pairs() {
    let temp = tempfile::tempdir().unwrap();
    let output_dir = temp.path().join("output");
    let reports = temp.path().join("reports");
    let collect = |strict: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_news-scraper"));
        command
            .args([
                "collect",
                "--sources",
                "未註冊測試來源",
                "--date",
                "2026-09-28",
                "--output-dir",
            ])
            .arg(&output_dir)
            .arg("--report-dir")
            .arg(&reports);
        if strict {
            command.arg("--fail-on-source-error");
        }
        let result = command.output().unwrap();
        assert_eq!(result.status.code(), Some(if strict { 1 } else { 0 }));
        let summary: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(summary["status"], "partial_failure");
        assert_eq!(
            summary["failed_sources"],
            serde_json::json!(["未註冊測試來源"])
        );
        let report_file = Path::new(summary["report_file"].as_str().unwrap());
        let report: Value = serde_json::from_slice(&std::fs::read(report_file).unwrap()).unwrap();
        assert_eq!(report["report_schema_version"], 4);
        assert_eq!(report["output_file"], summary["output_file"]);
        assert_eq!(report["week_start"], "2026-09-28");
        assert_eq!(report["week_end"], "2026-10-04");
        let marker: Value =
            serde_json::from_slice(&std::fs::read(report_file.with_extension("complete")).unwrap())
                .unwrap();
        assert_eq!(marker["run_id"], report["run_id"]);
        for (file_field, hash_field) in [
            ("output_file", "excel_sha256"),
            ("report_file", "report_sha256"),
        ] {
            let file = summary[file_field].as_str().unwrap();
            assert_eq!(
                marker[hash_field],
                format!("{:x}", Sha256::digest(std::fs::read(file).unwrap()))
            );
        }
        let mut workbook = zip::ZipArchive::new(
            std::fs::File::open(summary["output_file"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        use std::io::Read;
        let mut sheet = String::new();
        workbook
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut sheet)
            .unwrap();
        assert!(
            sheet.contains("Q1"),
            "17-column contract must survive an empty/failed run"
        );
        summary
    };
    let first = collect(false);
    let second = collect(true);
    assert_ne!(first["output_file"], second["output_file"]);
    assert!(Path::new(first["output_file"].as_str().unwrap()).is_file());
    let latest: Value =
        serde_json::from_slice(&std::fs::read(reports.join("latest_run.json")).unwrap()).unwrap();
    assert_eq!(latest["report_file"], second["report_file"]);
}
