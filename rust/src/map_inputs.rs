//! Explorer/startup input grouping. Projects take priority over loose sources,
//! as in the original open_input_paths; native JSON projects are also accepted.
use std::{collections::BTreeSet, path::PathBuf};
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub project: Option<PathBuf>,
    pub maps: Vec<PathBuf>,
    pub ignored: Vec<PathBuf>,
}
pub fn plan(paths: Vec<PathBuf>) -> Result<Plan, String> {
    if paths.len() > 4096 {
        return Err("Selecteer maximaal 4096 bestanden tegelijk".into());
    }
    let mut seen = BTreeSet::new();
    let mut out = Plan::default();
    let mut tiffs = Vec::new();
    let mut dxfs = Vec::new();
    for path in paths {
        if path.as_os_str().is_empty() || !seen.insert(path.clone()) {
            continue;
        }
        match path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase()
            .as_str()
        {
            "klicproject" | "json" => {
                if out.project.is_none() {
                    out.project = Some(path)
                } else {
                    out.ignored.push(path)
                }
            }
            "tif" | "tiff" => tiffs.push(path),
            "dxf" => dxfs.push(path),
            _ => out.ignored.push(path),
        }
    }
    tiffs.extend(dxfs);
    if out.project.is_some() {
        out.ignored.extend(tiffs);
    } else {
        out.maps = tiffs;
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_project_priority_and_tiff_first_order() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/map-inputs/reference.json"))
                .unwrap();
        for case in data["cases"].as_array().unwrap() {
            let paths = case["input"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| PathBuf::from(v.as_str().unwrap()))
                .collect();
            let got = plan(paths).unwrap();
            assert_eq!(
                got.project
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                case["project"].as_str().map(str::to_owned)
            );
            assert_eq!(
                got.maps
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
                case["maps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            );
            // The original groups unknowns before other projects; ordering of
            // warnings has no effect on the chosen project or loaded sources.
            let mut ignored = got.ignored;
            ignored.sort();
            let mut expected = case["ignored"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| PathBuf::from(v.as_str().unwrap()))
                .collect::<Vec<_>>();
            expected.sort();
            assert_eq!(ignored, expected);
        }
    }
    #[test]
    fn native_json_and_duplicate_paths_are_handled_explicitly() {
        let p = plan(vec!["a.dxf".into(), "a.dxf".into(), "project.json".into()]).unwrap();
        assert_eq!(p.project, Some("project.json".into()));
        assert!(p.maps.is_empty());
        assert_eq!(p.ignored, vec![PathBuf::from("a.dxf")]);
        assert!(plan(vec!["x".into(); 4097]).is_err());
    }
}
