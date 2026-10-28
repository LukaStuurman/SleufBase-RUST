use serde_json::Value;
use sleufbase_rust::{
    project,
    template_order::{self, Row, State},
};
use std::path::{Path, PathBuf};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/template-order/reference.json")).unwrap()
}
#[test]
fn normalization_labels_variants_and_filenames_match_the_actual_source_dialog() {
    let f = fixture();
    for c in f["numbers"].as_array().unwrap() {
        assert_eq!(
            serde_json::to_value(template_order::number(c["input"].as_str().unwrap())).unwrap(),
            c["expected"],
            "{}",
            c["input"]
        );
    }
    for c in f["variants"].as_array().unwrap() {
        assert_eq!(
            template_order::variant(&c["input"]),
            c["expected"].as_str().unwrap()
        );
    }
    for c in f["filenames"].as_array().unwrap() {
        let p = Path::new(c["path"].as_str().unwrap());
        assert_eq!(
            template_order::default_number(p, c["index"].as_u64().unwrap() as usize),
            c["number"].as_str().unwrap(),
            "{}",
            p.display()
        );
        assert_eq!(
            template_order::default_variant(p),
            c["variant"].as_str().unwrap(),
            "{}",
            p.display()
        );
    }
    for c in f["suffixes"].as_array().unwrap() {
        assert_eq!(
            template_order::suffix(c["input"].as_u64().unwrap() as usize),
            c["expected"].as_str().unwrap()
        );
    }
    for c in f["cases"].as_array().unwrap() {
        let state = State {
            rows: serde_json::from_value(c["rows"].clone()).unwrap(),
            ..Default::default()
        };
        let (labels, invalid) = state.preview_labels();
        assert_eq!(serde_json::to_value(labels).unwrap(), c["labels"]);
        assert_eq!(serde_json::to_value(invalid.clone()).unwrap(), c["invalid"]);
        assert_eq!(state.labels().is_ok(), invalid.is_empty());
    }
}
#[test]
fn multi_selection_moves_whole_rows_without_reversing_blocks() {
    let rows = (0..7)
        .map(|i| Row {
            raster: Some(PathBuf::from(format!("PS{i}.tif"))),
            number: i.to_string(),
            comments: format!("comment {i}"),
            scale: Some(100 + i),
            ..Default::default()
        })
        .collect();
    let mut s = State {
        rows,
        ..Default::default()
    };
    let mut selected = vec![false, true, true, false, true, false, false];
    s.move_selected(&mut selected, false);
    assert_eq!(
        s.rows.iter().map(|r| r.number.as_str()).collect::<Vec<_>>(),
        ["1", "2", "0", "4", "3", "5", "6"]
    );
    s.move_selected(&mut selected, true);
    assert_eq!(
        s.rows.iter().map(|r| r.number.as_str()).collect::<Vec<_>>(),
        ["0", "1", "2", "3", "4", "5", "6"]
    );
    for (i, row) in s.rows.iter().enumerate() {
        assert_eq!(row.comments, format!("comment {i}"));
        assert_eq!(row.scale, Some(100 + i as i64));
    }
}
#[test]
fn natural_numeric_and_name_sorting_preserves_blank_sheet_positions_and_row_data() {
    use template_order::SortMode;
    let source = [
        ("PS10.tif", "10", "", "notes 10"),
        ("PS2B.tif", "2", "B", "notes 2B"),
        ("PS2.tif", "2", "", "notes 2"),
        ("PS1.tif", "1", "", "notes 1"),
        ("Unknown.tif", "invalid", "", "notes unknown"),
    ];
    let mut state=State {
        rows:source.iter().map(|(file,n,v,c)|Row {
            raster:Some(PathBuf::from(file)),number:(*n).into(),variant:(*v).into(),
            comments:(*c).into(),..Default::default()
        }).collect(),
        ..Default::default()
    };
    state.rows.insert(2,Row::default());
    let order=state.sort_rows(SortMode::NumberAsc);
    assert_eq!(order,[4,3,2,1,0,5]);
    assert_eq!(state.rows.iter().map(|r|r.number.as_str()).collect::<Vec<_>>(),
               ["1","2","","2","10","invalid"]);
    assert_eq!(state.rows[3].variant,"B");
    assert_eq!(state.rows[3].comments,"notes 2B");
    assert!(state.rows[2].raster.is_none());
    state.sort_rows(SortMode::NumberDesc);
    assert_eq!(state.rows.iter().map(|r|r.number.as_str()).collect::<Vec<_>>(),
               ["10","2","","2","1","invalid"]);
    assert_eq!(state.rows[1].variant,"B");
    assert_eq!(state.rows[1].comments,"notes 2B");

    let mut names=State {
        rows:["Sheet10.tif","Sheet2.tif","Alpha.tif"].into_iter().enumerate()
            .map(|(i,name)|Row{raster:Some(name.into()),number:(i+1).to_string(),..Default::default()})
            .collect(), ..Default::default()
    };
    names.sort_rows(SortMode::NameAsc);
    assert_eq!(names.rows.iter().map(|r|r.raster.as_ref().unwrap().as_path()).collect::<Vec<_>>(),
               ["Alpha.tif","Sheet2.tif","Sheet10.tif"].map(Path::new));
    names.sort_rows(SortMode::NameDesc);
    assert_eq!(names.rows.iter().map(|r|r.raster.as_ref().unwrap().as_path()).collect::<Vec<_>>(),
               ["Sheet10.tif","Sheet2.tif","Alpha.tif"].map(Path::new));

    assert!(SortMode::NumberAsc.compare("999999999999999999999","", "A","1000000000000000000000","","B").is_lt());
    assert!(SortMode::NumberDesc.compare("2","","A","invalid","","B").is_lt());
}

#[test]
fn project_v5_roundtrips_order_metadata_and_reads_v1_through_v4_without_rewriting_sources() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.json");
    let state = State {
        project: "Project Ø 漢".into(),
        rows: vec![
            Row {
                raster: Some("PS1.tiff".into()),
                number: "1".into(),
                metadata: serde_json::json!({"template_reference_point":{"x":10,"y":20},"custom":{"keep":true}}),
                ..Default::default()
            },
            Row::default(),
        ],
        ..Default::default()
    };
    let p = project::NativeProject {
        template_export: Some(state.clone()),
        ..Default::default()
    };
    project::save_project(&path, &p).unwrap();
    let loaded = project::read_project(&path).unwrap();
    assert_eq!(loaded.version, project::FORMAT_VERSION);
    assert_eq!(loaded.template_export, Some(state));
    for version in [1, 2, 3, 4] {
        let mut v = serde_json::to_value(&p).unwrap();
        v["version"] = version.into();
        v.as_object_mut().unwrap().remove("template_export");
        let bytes = serde_json::to_vec(&v).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let loaded = project::read_project(&path).unwrap();
        assert_eq!(loaded.version, project::FORMAT_VERSION);
        assert!(loaded.template_export.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
#[test]
fn original_multi_selection_actions_and_nearby_comment_text_match_source() {
    let fixture = fixture();
    for c in fixture["moves"].as_array().unwrap() {
        let count = c["count"].as_u64().unwrap() as usize;
        let mut state = State {
            rows: (0..count)
                .map(|i| Row {
                    raster: Some(format!("PS{i}.tif").into()),
                    number: i.to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut selected = vec![false; count];
        for i in c["selection"].as_array().unwrap() {
            selected[i.as_u64().unwrap() as usize] = true;
        }
        state.move_selected(&mut selected, c["offset"].as_i64().unwrap() > 0);
        let order = state
            .rows
            .iter()
            .map(|r| r.number.parse::<usize>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(serde_json::to_value(order).unwrap(), c["order"]);
        let actual = selected
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.then_some(i))
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            c["expected_selection"]
        );
    }
    for c in fixture["comments"].as_array().unwrap() {
        let bounds = serde_json::from_value(c["bounds"].clone()).unwrap();
        let comments: Vec<sleufbase_rust::project::Comment> =
            serde_json::from_value(c["comments"].clone()).unwrap();
        assert_eq!(
            sleufbase_rust::template_workflow::comments(bounds, &comments, ""),
            c["expected"].as_str().unwrap()
        );
    }
}
