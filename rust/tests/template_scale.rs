use sleufbase_rust::template_scale;
#[test]
fn original_scale_controls_and_metadata_coercion() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-scale/reference.json")).unwrap();
    for case in reference["parse"].as_array().unwrap() {
        let actual = template_scale::parse(case["text"].as_str().unwrap());
        if case["error"] == true {
            assert!(actual.is_err(), "{case}");
        } else {
            assert_eq!(
                serde_json::to_value(actual.unwrap()).unwrap(),
                case["value"],
                "{case}"
            );
        }
    }
    for case in reference["coerce"].as_array().unwrap() {
        assert_eq!(
            serde_json::to_value(template_scale::coerce(Some(&case["value"]))).unwrap(),
            case["expected"],
            "{case}"
        );
    }
}
