use sleufbase_rust::{dxf_document::Document, template_pages};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn bytes(doc: &Document) -> Vec<u8> {
    let mut b = Vec::new();
    doc.write_to(&mut b, &AtomicBool::new(false)).unwrap();
    b
}
#[test]
fn dimension_copies_have_independent_geometry_blocks_and_owners() {
    let cancel = AtomicBool::new(false);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/template-dimensions/source.dxf");
    let mut doc = Document::load(&path, &cancel).unwrap();
    for (required, count) in [(9, 10), (17, 15), (33, 25)] {
        template_pages::extend(&mut doc, required, &cancel).unwrap();
        let dims = doc
            .records()
            .filter(|r| r.kind() == "DIMENSION")
            .collect::<Vec<_>>();
        assert_eq!(dims.len(), count);
        let mut names = std::collections::HashSet::new();
        for d in dims {
            assert!(names.insert(d.first(2).unwrap().to_owned()));
            let block = doc
                .records()
                .find(|r| r.kind() == "BLOCK_RECORD" && r.first(2) == d.first(2))
                .unwrap();
            assert!(doc.records().any(|r| r.kind() == "BLOCK"
                && r.first(330).and_then(|h| u64::from_str_radix(h, 16).ok()) == block.handle()));
        }
    }
}
#[test]
fn missing_dimension_geometry_and_cancel_leave_document_unchanged() {
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/template-dimensions/source.dxf"),
    )
    .unwrap();
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut active = false;
    for pair in lines.chunks_exact_mut(2) {
        if pair[0].trim() == "0" {
            active = pair[1] == "DIMENSION";
        }
        if active && pair[0].trim() == "2" {
            pair[1] = "MISSING_DIMENSION_BLOCK".into();
            break;
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.dxf");
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let mut doc = Document::load(&path, &AtomicBool::new(false)).unwrap();
    let before = bytes(&doc);
    assert!(template_pages::extend(&mut doc, 9, &AtomicBool::new(false))
        .unwrap_err()
        .contains("geometrieblok"));
    assert_eq!(bytes(&doc), before);
    assert!(template_pages::extend(&mut doc, 9, &AtomicBool::new(true)).is_err());
    assert_eq!(bytes(&doc), before);
}
#[test]
fn cyclic_dimension_geometry_is_rejected_without_publishing_any_copies() {
    let cancel = AtomicBool::new(false);
    let source = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/template-dimensions/source.dxf"),
    )
    .unwrap();
    let doc = Document::from_bytes(source.clone(), &cancel).unwrap();
    let name = doc
        .records()
        .find(|r| r.kind() == "DIMENSION")
        .unwrap()
        .first(2)
        .unwrap();
    let block = doc
        .records()
        .find(|r| r.kind() == "BLOCK_RECORD" && r.first(2) == Some(name))
        .unwrap();
    let owner = block.first(5).unwrap();
    let end = doc
        .records()
        .find(|r| r.kind() == "ENDBLK" && r.first(330) == Some(owner))
        .unwrap()
        .first(5)
        .unwrap();
    let fresh = doc.records().filter_map(|r| r.handle()).max().unwrap() + 1;
    let child=format!("0\nDIMENSION\n5\n{fresh:X}\n330\n{owner}\n100\nAcDbEntity\n8\n0\n100\nAcDbDimension\n2\n{name}\n10\n0\n20\n0\n30\n0\n70\n32\n3\nStandard\n100\nAcDbAlignedDimension\n13\n0\n23\n0\n33\n0\n14\n1\n24\n0\n34\n0\n100\nAcDbRotatedDimension\n50\n0\n");
    let source = String::from_utf8(source).unwrap();
    let source = source
        .lines()
        .enumerate()
        .map(|(i, line)| if i % 2 == 0 { line.trim() } else { line })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let needle = format!("0\nENDBLK\n5\n{end}\n");
    assert_eq!(source.matches(&needle).count(), 1);
    let mut doc = Document::from_bytes(
        source
            .replacen(&needle, &format!("{child}{needle}"), 1)
            .into_bytes(),
        &cancel,
    )
    .unwrap();
    let before = bytes(&doc);
    assert!(template_pages::extend(&mut doc, 9, &cancel)
        .unwrap_err()
        .contains("Cyclisch"));
    assert_eq!(bytes(&doc), before);
}
