use pdf_extract::{dictionary, Document, Object, Stream};
use std::path::Path;
use std::time::Duration;
use taiwan_government_news_lib::scraper::pdf;

fn document(text: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let mut mappings = String::new();
    let mut encoded = String::new();
    for (index, character) in text.chars().enumerate() {
        let code = index + 1;
        let unicode = character
            .encode_utf16(&mut [0; 2])
            .iter()
            .map(|unit| format!("{unit:04X}"))
            .collect::<String>();
        mappings.push_str(&format!("<{code:04X}> <{unicode}>\n"));
        encoded.push_str(&format!("{code:04X}"));
    }
    let cmap = format!("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Test def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange {} beginbfchar\n{mappings}endbfchar endcmap CMapName currentdict /CMap defineresource pop end end", text.chars().count());
    let unicode = doc.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    let cid = doc.add_object(dictionary! {"Type"=>"Font", "Subtype"=>"CIDFontType2", "BaseFont"=>"TestFont", "DW"=>1000,
        "FontDescriptor"=>dictionary!{"Type"=>"FontDescriptor", "FontName"=>"TestFont", "Flags"=>4, "FontBBox"=>vec![0.into(),0.into(),1000.into(),1000.into()], "ItalicAngle"=>0, "Ascent"=>1000, "Descent"=>0, "CapHeight"=>1000, "StemV"=>80}, "CIDSystemInfo"=>dictionary!{"Registry"=>Object::string_literal("Adobe"), "Ordering"=>Object::string_literal("Identity"), "Supplement"=>0}});
    let font = doc.add_object(dictionary! {"Type"=>"Font", "Subtype"=>"Type0", "BaseFont"=>"TestFont", "Encoding"=>"Identity-H", "DescendantFonts"=>vec![cid.into()], "ToUnicode"=>unicode});
    let content = doc.add_object(Stream::new(
        dictionary! {},
        format!("BT /F1 12 Tf 30 700 Td <{encoded}> Tj ET").into_bytes(),
    ));
    let page = doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "MediaBox"=>vec![0.into(),0.into(),600.into(),800.into()], "Contents"=>content,
        "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}}});
    doc.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages", "Kids"=>vec![page.into()], "Count"=>1}.into(),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[tokio::test]
async fn cli_worker_preserves_chinese_publication_body() {
    let text = "最高檢察署新聞稿：檢警調廉共同推動反賄選宣導，維護公平選舉與法治精神。";
    let bytes = document(text);
    for executable in [
        env!("CARGO_BIN_EXE_news-scraper"),
        env!("CARGO_BIN_EXE_taiwan-government-news"),
    ] {
        let recovered =
            pdf::extract_with_executable(&bytes, Path::new(executable), Duration::from_secs(10))
                .await
                .unwrap();
        assert_eq!(recovered, text);
    }
}

#[tokio::test]
async fn malformed_empty_and_oversized_pdfs_do_not_become_successful_text() {
    let executable = Path::new(env!("CARGO_BIN_EXE_news-scraper"));
    assert_eq!(
        pdf::extract_with_executable(b"%PDF-broken", executable, Duration::from_secs(5)).await,
        Err("pdf_invalid")
    );
    assert_eq!(
        pdf::extract_with_executable(&document(""), executable, Duration::from_secs(5)).await,
        Err("pdf_no_text")
    );
    assert_eq!(
        pdf::extract_with_executable(
            &vec![0; pdf::MAX_PDF_BYTES + 1],
            executable,
            Duration::from_secs(5)
        )
        .await,
        Err("pdf_size_limit")
    );
    assert!(!pdf::is_pdf(b"<html>verification page for news.pdf</html>"));
}

#[tokio::test]
async fn excessive_pages_are_rejected_before_text_extraction() {
    let mut doc = Document::load_mem(&document(
        "最高檢察署新聞正文，維護法治與公平選舉的完整說明內容。",
    ))
    .unwrap();
    let first = *doc.get_pages().values().next().unwrap();
    let page = doc.get_object(first).unwrap().as_dict().unwrap().clone();
    let pages = page.get(b"Parent").unwrap().as_reference().unwrap();
    let children = (0..101)
        .map(|_| Object::Reference(doc.add_object(page.clone())))
        .collect::<Vec<_>>();
    let root = doc.get_object_mut(pages).unwrap().as_dict_mut().unwrap();
    root.set("Kids", children);
    root.set("Count", 101);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert_eq!(
        pdf::extract_with_executable(
            &bytes,
            Path::new(env!("CARGO_BIN_EXE_news-scraper")),
            Duration::from_secs(5)
        )
        .await,
        Err("pdf_size_limit")
    );
}

#[tokio::test]
async fn oversized_text_is_not_returned_as_a_successful_truncated_article() {
    let mut doc = Document::load_mem(&document("中")).unwrap();
    let first = *doc.get_pages().values().next().unwrap();
    let content = doc
        .get_object(first)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap();
    let text = format!("BT /F1 12 Tf 30 700 Td <{}> Tj ET", "0001".repeat(710_000));
    doc.get_object_mut(content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(text.into_bytes());
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert!(bytes.len() < pdf::MAX_PDF_BYTES);
    assert_eq!(
        pdf::extract_with_executable(
            &bytes,
            Path::new(env!("CARGO_BIN_EXE_news-scraper")),
            Duration::from_secs(10)
        )
        .await,
        Err("pdf_size_limit")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn worker_failure_and_timeout_are_isolated_and_reaped() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("worker");
    std::fs::write(&executable, "#!/bin/sh\nexit 17\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        pdf::extract_with_executable(b"%PDF-1.5", &executable, Duration::from_secs(5)).await,
        Err("pdf_worker_failed")
    );
    let marker = temp.path().join("pid");
    std::fs::write(
        &executable,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec sleep 30\n",
            marker.display()
        ),
    )
    .unwrap();
    assert_eq!(
        pdf::extract_with_executable(b"%PDF-1.5", &executable, Duration::from_millis(200)).await,
        Err("pdf_timeout")
    );
    let pid = std::fs::read_to_string(marker).unwrap();
    for _ in 0..20 {
        if !std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed-out worker {pid} still exists");
}
