use sleufbase_rust::{dxf_document::Document, template_pages};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-xclip/source.dxf")
}
fn bytes(doc: &Document) -> Vec<u8> {
    let mut b = Vec::new();
    doc.write_to(&mut b, &AtomicBool::new(false)).unwrap();
    b
}
#[test]
fn original_xclip_filters_survive_repeated_layout_extension() {
    let cancel = AtomicBool::new(false);
    let mut doc = Document::load(&source(), &cancel).unwrap();
    for (required, count) in [(9, 6), (17, 9), (33, 15)] {
        template_pages::extend(&mut doc, required, &cancel).unwrap();
        assert_eq!(
            doc.records()
                .filter(|r| r.kind() == "SPATIAL_FILTER")
                .count(),
            count
        );
        let mut owners = std::collections::HashSet::new();
        for f in doc.records().filter(|r| r.kind() == "SPATIAL_FILTER") {
            assert!(owners.insert(f.first(330).unwrap().to_owned()));
            assert_eq!(f.tags().filter(|(c, _)| *c == 40).count(), 25);
        }
    }
}
#[test]
fn malformed_filter_ownership_or_cancellation_cannot_modify_the_source() {
    let source = std::fs::read_to_string(source()).unwrap();
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut filter = false;
    let mut depth=0usize;
    let mut changed=false;
    for pair in lines.chunks_exact_mut(2) {
        if pair[0].trim() == "0" {
            filter = pair[1] == "SPATIAL_FILTER";
            depth=0;
        }
        if filter&&pair[0].trim()=="102"{if pair[1].starts_with('{'){depth+=1;}else if pair[1]=="}"{depth=depth.saturating_sub(1);}}
        // A reactor also has code 330; corrupt the actual owner outside APPDATA.
        if filter && depth==0 && pair[0].trim() == "330" {
            pair[1] = "0".into();
            changed=true;
            break;
        }
    }
    assert!(changed);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid-filter.dxf");
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let mut doc = Document::load(&path, &AtomicBool::new(false)).unwrap();
    let before = bytes(&doc);
    assert!(template_pages::extend(&mut doc, 9, &AtomicBool::new(false))
        .unwrap_err()
        .contains("eigendom"));
    assert_eq!(bytes(&doc), before);
    assert!(template_pages::extend(&mut doc, 9, &AtomicBool::new(true)).is_err());
    assert_eq!(bytes(&doc), before);
}
