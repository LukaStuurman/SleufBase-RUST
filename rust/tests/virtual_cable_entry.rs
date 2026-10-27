use sleufbase_rust::{
    cross_section,
    dxf_document::Document,
    model::Vertex,
    object_dxf,
    virtual_trench::{TrenchPoint, VirtualTrench},
};
#[test]
fn oblique_cable_is_saved_as_a_clipped_line_in_a_real_dxf() {
    let mut trench = VirtualTrench::new(
        "PS12".into(),
        Vertex {
            x: 155000.,
            y: 463000.,
            z: Some(2.),
        },
        Vertex {
            x: 155010.,
            y: 463000.,
            z: Some(2.),
        },
        0.6,
    )
    .unwrap();
    trench.objects.push(TrenchPoint {
        metadata: serde_json::json!({"crossing_angle_degrees":45.,"quantity":3}),
        role: "leiding".into(),
        object_name: "Waterleiding".into(),
        source_name: "water".into(),
        position: Vertex {
            x: 155005.,
            y: 463000.,
            z: Some(1.3),
        },
        attributes: ["PE".into(), "110".into(), "3".into()],
    });
    let serialized = serde_json::to_string(&trench).unwrap();
    let restored: VirtualTrench = serde_json::from_str(&serialized).unwrap();
    let expected = cross_section::markers(&restored).remove(0);
    let folder = tempfile::tempdir().unwrap();
    let output = folder.path().join("schuine-kabel.dxf");
    object_dxf::write_with_layers(
        &output,
        &[],
        &[restored],
        None,
        &[],
        sleufbase_rust::disciplines::RULES,
        object_dxf::Options::default(),
    )
    .unwrap();
    let doc = Document::load(&output, &std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert!(
        doc.records().any(|r| {
            if r.kind() != "LINE" {
                return false;
            }
            let value = |c| r.first(c).and_then(|v| v.parse::<f64>().ok());
            match (value(10), value(20), value(11), value(21)) {
                (Some(x1), Some(y1), Some(x2), Some(y2)) => {
                    ((x1 - expected.start.x).abs() < 1e-6
                        && (y1 - expected.start.y).abs() < 1e-6
                        && (x2 - expected.end.x).abs() < 1e-6
                        && (y2 - expected.end.y).abs() < 1e-6)
                        || ((x2 - expected.start.x).abs() < 1e-6
                            && (y2 - expected.start.y).abs() < 1e-6
                            && (x1 - expected.end.x).abs() < 1e-6
                            && (y1 - expected.end.y).abs() < 1e-6)
                }
                _ => false,
            }
        }),
        "export must contain the oblique cable segment, not a perpendicular replacement"
    );
    assert!((expected.start.x - expected.end.x).abs() > 0.5);
}
