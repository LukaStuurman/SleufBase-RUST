use sleufbase_rust::{Bounds,cadastral,template_server};
use std::sync::atomic::AtomicBool;
#[test]
fn active_context_and_wfs_grid_match_original_startup_helpers(){
    let oracle:serde_json::Value=serde_json::from_str(include_str!("fixtures/template-context/reference.json")).unwrap();
    for case in oracle["context"].as_array().unwrap(){let bounds:Vec<Bounds>=serde_json::from_value(case["bounds"].clone()).unwrap();assert_eq!(serde_json::to_value(template_server::fetch_bounds(&bounds).unwrap()).unwrap(),case["expected"],"{}",case["name"]);}
    for case in oracle["queries"].as_array().unwrap(){let bounds:Bounds=serde_json::from_value(case["bounds"].clone()).unwrap();let actual=cadastral::template_tiles(bounds,&AtomicBool::new(false));if case["count"].as_u64().unwrap()>4096{assert!(actual.unwrap_err().contains("tegels"));}else{assert_eq!(serde_json::to_value(actual.unwrap()).unwrap(),case["expected"],"{}",case["name"]);}}
}
#[test]
fn viewport_limit_and_pre_cancelled_context_remain_distinct(){
    let bounds=Bounds{min_x:155000.,min_y:463000.,max_x:162800.,max_y:464510.};
    assert!(cadastral::tiles(bounds).unwrap_err().contains("3,5"));assert!(cadastral::template_tiles(bounds,&AtomicBool::new(false)).is_ok());assert!(cadastral::template_tiles(bounds,&AtomicBool::new(true)).unwrap_err().contains("geannuleerd"));
}
