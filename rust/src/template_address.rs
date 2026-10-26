//! Active original template address formatting, independent of HTTP and GUI.
//! Explicit prepared addresses take priority; reverse lookup is Fresh-only and
//! best effort. No metadata address field is used by the original callback.
use crate::{Bounds,export_control::Control,location_search::ReverseLocation};
pub fn format(label:&str)->Option<String>{
    let label=crate::python_values::trim(label);
    if label.is_empty()||label.len()>4096||label.chars().any(char::is_control){return None;}
    Some(if let Some((head,tail))=label.split_once(','){std::format!("{},\\P{}",crate::python_values::trim(head),crate::python_values::trim(tail))}else{label.into()})
}
pub fn lookup(bounds:Bounds,control:&Control,mut reverse:impl FnMut(f64,f64,&Control)->Result<Vec<ReverseLocation>,String>)->Result<Option<String>,String>{
    control.check()?;
    let result=reverse(bounds.min_x*0.5+bounds.max_x*0.5,bounds.min_y*0.5+bounds.max_y*0.5,control);
    control.check()?;
    Ok(result.ok().and_then(|rows|rows.into_iter().next()).and_then(|row|format(&row.label)))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn isolated_original_callback_fixtures(){let cases:serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/template-address/reference.json")).unwrap();for case in cases.as_array().unwrap(){let result=case["label"].as_str().and_then(format);assert_eq!(serde_json::to_value(result).unwrap(),case["expected"]);}}
    #[test]
    fn original_first_comma_and_python_trim(){
        assert_eq!(format("  Straat 2, 1234 AA Stad, provincie  ").as_deref(),Some("Straat 2,\\P1234 AA Stad, provincie"));
        assert_eq!(format("  Alleen straat  ").as_deref(),Some("Alleen straat"));assert_eq!(format("  "),None);
        assert_eq!(format("Straat\nInjectie"),None);
    }
    #[test]
    fn original_lookup_center_first_result_and_best_effort(){
        let control=Control::default();let bounds=Bounds{min_x:1.,max_x:5.,min_y:2.,max_y:8.};
        let output=lookup(bounds,&control,|x,y,_|{assert_eq!((x,y),(3.,5.));Ok(vec![ReverseLocation{label:"First, City".into(),distance_m:0.,location_type:String::new()},ReverseLocation{label:"Second".into(),distance_m:1.,location_type:String::new()}])}).unwrap();
        assert_eq!(output.as_deref(),Some("First,\\PCity"));assert_eq!(lookup(bounds,&control,|_,_,_|Err("offline server".into())).unwrap(),None);
        assert!(lookup(bounds,&control,|_,_,c|{c.cancel();Err("cancelled".into())}).is_err());
    }
}
