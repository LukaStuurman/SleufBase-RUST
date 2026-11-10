//! Physical TIFF profile sources, matching the active original local-first
//! maaiveld loader and explicit fresh-template snapshot workflow.
use super::{Options, Source};
use crate::{export_control::Control, export_worker::Inputs, geotiff::GeoTiffInfo, Dataset};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
pub fn local_path(raster: &GeoTiffInfo) -> Option<PathBuf> {
    let raw = raster
        .metadata
        .get("kickthemap_job_features_path")?
        .as_str()?
        .trim();
    if raw.is_empty() {
        return None;
    }
    let p = PathBuf::from(raw);
    Some(
        if p.is_absolute() || raw.as_bytes().get(1) == Some(&b':') || raw.starts_with("\\\\") {
            p
        } else {
            raster.path.parent().unwrap_or(Path::new(".")).join(p)
        },
    )
}
/// A present local file is authoritative, including an explicit parse error;
/// a stale/missing local path can fall back to an unambiguous loaded dataset.
pub fn resolve_raster<'a>(
    raster: &GeoTiffInfo,
    loaded: &'a [Dataset],
    cancel: &AtomicBool,
) -> Result<Option<Cow<'a, Dataset>>, String> {
    crate::input::check(cancel)?;
    let id = raster.kickthemap_job_id.filter(|id| *id > 0);
    if let Some(path) = local_path(raster).filter(|p| p.exists()) {
        let id = id.ok_or("Geen KickTheMap-job gekoppeld aan deze proefsleuf")?;
        let bytes =
            crate::input::read(&path, 256 * 1024 * 1024, "Lokale TIFF-profielbron", cancel)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| "Lokale TIFF-profielbron bevat geen geldige UTF-8")?;
        let title = if raster.kickthemap_job_title.trim().is_empty() {
            raster
                .path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            raster.kickthemap_job_title.clone()
        };
        let mut dataset = crate::parse::parse_job_features_with_cancel(text, id, &title, cancel)?;
        dataset.source_path = path;
        return Ok(Some(Cow::Owned(dataset)));
    }
    let mut matches = loaded
        .iter()
        .filter(|d| id.is_some_and(|id| d.job_id == id));
    let found = matches.next();
    if matches.next().is_some() {
        return Err("De TIFF-profielkoppeling is niet eenduidig".into());
    }
    Ok(found.map(Cow::Borrowed))
}
#[derive(Clone, Debug)]
pub struct HydratedFile {
    pub path: PathBuf,
    pub title: String,
}
pub trait ProfileProvider {
    fn download(&mut self, job_id: u64, control: &Control) -> Result<HydratedFile, String>;
}
/// Stages every requested response before changing Inputs. Cancel/failure leaves
/// source metadata and datasets intact, and never replaces the original files.
pub fn hydrate_missing(
    opts: &Options,
    inputs: &mut Inputs,
    provider: &mut impl ProfileProvider,
    force_refresh: bool,
    control: &Control,
) -> Result<usize, String> {
    let mut wanted = Vec::new();
    let mut seen = HashSet::new();
    for choice in opts
        .choices
        .iter()
        .filter(|c| c.include && !matches!(c.source, Source::Empty(_) | Source::Virtual(_)))
    {
        if !choice.profile {
            continue;
        }
        let Some(index) = choice.raster else {
            if matches!(choice.source, Source::Raster(_)) {
                return Err("De fysieke profielbron mist een TIFF-koppeling".into());
            }
            continue;
        };
        let raster = inputs
            .rasters
            .get(index)
            .ok_or("De fysieke profielbron bestaat niet meer")?;
        if !force_refresh && resolve_raster(raster, &inputs.datasets, control.flag())?.is_some() {
            continue;
        }
        let Some(id) = raster.kickthemap_job_id.filter(|id| *id > 0) else {
            continue;
        };
        if seen.insert(index) {
            wanted.push((index, id));
        }
    }
    let mut staged = HashMap::new();
    for &(_, id) in &wanted {
        control.check()?;
        if staged.contains_key(&id) {
            continue;
        }
        let file = provider.download(id, control)?;
        let bytes = crate::input::read(
            &file.path,
            256 * 1024 * 1024,
            "Opgehaalde TIFF-profielbron",
            control.flag(),
        )?;
        // Validate one response at a time. Holding every parsed feature vector
        // until commit would duplicate the entire project just for validation.
        let _ = crate::parse::parse_job_features_with_cancel(
            std::str::from_utf8(&bytes)
                .map_err(|_| "Opgehaalde profielbron bevat geen geldige UTF-8")?,
            id,
            &file.title,
            control.flag(),
        )?;
        staged.insert(id, file);
    }
    control.check()?;
    for (index, id) in &wanted {
        let file = &staged[id];
        let raster = &mut inputs.rasters[*index];
        if !raster.metadata.is_object() {
            raster.metadata = serde_json::json!({});
        }
        raster.metadata["kickthemap_job_features_path"] = serde_json::json!(file.path);
        raster.metadata["kickthemap_job_title"] = serde_json::json!(file.title);
        raster.kickthemap_job_title = file.title.clone();
    }
    for (_, file) in staged {
        inputs.protected_paths.push(file.path);
        // The per-raster path is authoritative. Keep loaded source vectors and
        // their indices intact, including distinct sources with the same job ID.
    }
    Ok(wanted.len())
}
/// Whether all selected KickTheMap profiles already have a local source.
pub fn profiles_available(
    opts:&Options,rasters:&[GeoTiffInfo],datasets:&[Dataset],cancel:&AtomicBool,
)->Result<bool,String>{
    for choice in opts.choices.iter().filter(|choice|choice.include&&choice.profile){
        crate::input::check(cancel)?;
        let Some(raster)=choice.raster.and_then(|i|rasters.get(i)) else{continue;};
        let Some(id)=raster.kickthemap_job_id.filter(|id|*id>0) else{continue;};
        let available=match choice.source{
            Source::Job(i) if datasets.get(i).is_some_and(|dataset|dataset.job_id==id)
                &&local_path(raster).is_some()=>resolve_raster(raster,datasets,cancel)?.is_some_and(|dataset|!dataset.features.is_empty()),
            Source::Job(i)=>datasets.get(i).is_some_and(|dataset|!dataset.features.is_empty()),
            Source::Raster(_)=>resolve_raster(raster,datasets,cancel)?.is_some_and(|dataset|!dataset.features.is_empty()),
            _=>true,
        };
        if !available{return Ok(false);}
    }
    Ok(true)
}

/// A failed refresh may use complete local sources. Cancellation and missing
/// profiles remain errors, so the requested drawing is never silently degraded.
pub fn refresh_or_local(
    opts:&Options,inputs:&mut Inputs,provider:&mut impl ProfileProvider,control:&Control,
)->Result<bool,String>{
    match hydrate_missing(opts,inputs,provider,true,control){
        Ok(_)=>Ok(true),
        Err(error)=>{
            control.check()?;
            if profiles_available(opts,&inputs.rasters,&inputs.datasets,control.flag())?{
                Ok(false)
            }else{
                Err(format!("KickTheMap-profielbronnen niet beschikbaar: {error}. Laad de ontbrekende objectpunten of zet Profiel tekenen uit voor die vakken."))
            }
        }
    }
}

/// Caller transfers the exact owner-verified live session into its worker.
/// Each request gets a fresh output directory so existing valid files are never
/// mistaken for the original force_refresh=True template snapshot.
pub struct Live<'a> {
    pub client: &'a mut crate::kickthemap_live::KickTheMapLiveClient,
    pub root: PathBuf,
    jobs: Option<HashMap<u64, crate::jobs_cache::Job>>,
    browser:Option<crate::kickthemap_browser::BrowserChannel>,
}
impl<'a> Live<'a> {
    pub fn new(
        client: &'a mut crate::kickthemap_live::KickTheMapLiveClient,
        root: PathBuf,
        browser:Option<crate::kickthemap_browser::BrowserChannel>,
    ) -> Self {
        Self {
            client,
            root,
            jobs: None,
            browser,
        }
    }
}
impl ProfileProvider for Live<'_> {
    fn download(&mut self, job_id: u64, control: &Control) -> Result<HydratedFile, String> {
        if self.jobs.is_none() {
            let jobs = if let Some(browser)=&self.browser{
                self.client.fetch_jobs_from_browser(||browser.browser_job_payload_with_cancel(control.flag()))
                    .or_else(|browser_error|{
                        control.check()?;
                        self.client.fetch_jobs_with_cancel(control.flag())
                            .map_err(|http_error|format!("KickTheMap-jobs ophalen: {browser_error}; {http_error}"))
                    })?
            }else{self.client.fetch_jobs_with_cancel(control.flag())?};
            self.jobs = Some(jobs.into_iter().map(|job| (job.job_id, job)).collect());
        }
        let job = self
            .jobs
            .as_ref()
            .unwrap()
            .get(&job_id)
            .ok_or_else(|| format!("KickTheMap job {job_id} niet gevonden"))?;
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let directory = tempfile::Builder::new()
            .prefix("template_profile_snapshot_")
            .tempdir_in(&self.root)
            .map_err(|e| e.to_string())?;
        let path = self.client.download_job_with_browser(
            job,
            crate::kickthemap_live::RemoteFileKind::JobFeatures,
            directory.path(),
            control.flag(),
            self.browser.as_ref(),
        )?;
        let _ = directory.keep();
        Ok(HydratedFile {
            path,
            title: job.title.clone(),
        })
    }
}

#[cfg(test)]
mod hydration_tests {
    use super::*;
    const RAW: &str = r#"{"features":[{"geometry":{"type":"Point","coordinates":[1,2,3]},"properties":{"idName":"water"}}]}"#;
    fn raster(root: &Path, id: Option<u64>) -> GeoTiffInfo {
        let transform = crate::geo::GeoTransform {
            a: 1.,
            b: 0.,
            c: 0.,
            d: 0.,
            e: -1.,
            f: 10.,
        };
        GeoTiffInfo {
            path: root.join("PS42.tif"),
            width: 10,
            height: 10,
            transform,
            bounds: transform.bounds(10, 10).unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: id,
            kickthemap_job_title: "PS42".into(),
            kickthemap_job_sidecar: None,
            metadata: serde_json::json!({}),
        }
    }
    struct Provider {
        file: PathBuf,
        calls: usize,
        fail: bool,
    }
    impl ProfileProvider for Provider {
        fn download(&mut self, _: u64, _: &Control) -> Result<HydratedFile, String> {
            self.calls += 1;
            if self.fail {
                Err("fake request failed".into())
            } else {
                Ok(HydratedFile {
                    path: self.file.clone(),
                    title: "Fresh title".into(),
                })
            }
        }
    }
    #[test]
    fn local_metadata_source_is_authoritative_and_malformed_is_distinct_from_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("job.json");
        std::fs::write(&path, RAW).unwrap();
        let mut r = raster(dir.path(), Some(42));
        r.metadata["kickthemap_job_features_path"] = "job.json".into();
        let old = crate::parse::parse_job_features(RAW, 42, "Old loaded title").unwrap();
        let jobs = [old];
        let result = resolve_raster(&r, &jobs, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        assert_eq!(result.title, "PS42");
        assert_eq!(result.source_path, path);
        std::fs::write(&path, b"broken").unwrap();
        assert!(resolve_raster(&r, &jobs, &AtomicBool::new(false)).is_err());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            resolve_raster(&r, &jobs, &AtomicBool::new(false))
                .unwrap()
                .unwrap()
                .title,
            "Old loaded title"
        );
    }
    #[test]
    fn fresh_hydration_preserves_loaded_source_vectors_and_uses_per_raster_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fresh.json");
        std::fs::write(&path, RAW).unwrap();
        let jobs = vec![
            crate::parse::parse_job_features(RAW, 42, "Original A").unwrap(),
            crate::parse::parse_job_features(RAW, 42, "Original B").unwrap(),
        ];
        let mut inputs = Inputs {
            datasets: jobs,
            rasters: vec![raster(dir.path(), Some(42)), raster(dir.path(), Some(42))],
            ..Default::default()
        };
        let ptr = inputs.datasets.as_ptr();
        let opts = Options {
            choices: crate::template_project::suggest_rasters(&inputs.rasters, &[]),
            ..Default::default()
        };
        let mut provider = Provider {
            file: path.clone(),
            calls: 0,
            fail: false,
        };
        assert_eq!(
            hydrate_missing(&opts, &mut inputs, &mut provider, true, &Control::default()).unwrap(),
            2
        );
        assert_eq!(provider.calls, 1);
        assert_eq!(inputs.datasets.as_ptr(), ptr);
        assert_eq!(inputs.datasets[0].title, "Original A");
        assert_eq!(inputs.datasets[1].title, "Original B");
        assert!(inputs.protected_paths.contains(&path));
        assert_eq!(
            resolve_raster(
                &inputs.rasters[0],
                &inputs.datasets,
                &AtomicBool::new(false)
            )
            .unwrap()
            .unwrap()
            .title,
            "Fresh title"
        );
    }
    #[test]
    fn refresh_failure_reuses_complete_local_profiles_and_refuses_missing_ones(){
        let dir=tempfile::tempdir().unwrap();
        let mut inputs=Inputs{
            rasters:vec![raster(dir.path(),Some(42))],
            datasets:vec![crate::parse::parse_job_features(RAW,42,"Lokaal").unwrap()],
            ..Default::default()
        };
        let mut opts=Options{choices:crate::template_project::suggest_rasters(&inputs.rasters,&[]),..Default::default()};
        let mut provider=Provider{file:dir.path().join("unused"),calls:0,fail:true};
        let control=Control::default();
        assert!(!refresh_or_local(&opts,&mut inputs,&mut provider,&control).unwrap());
        assert_eq!(provider.calls,1);
        assert_eq!(resolve_raster(&inputs.rasters[0],&inputs.datasets,control.flag()).unwrap().unwrap().title,"Lokaal");
        inputs.datasets.clear();
        assert!(refresh_or_local(&opts,&mut inputs,&mut provider,&control).unwrap_err().contains("objectpunten"));
        opts.choices[0].profile=false;
        assert!(refresh_or_local(&opts,&mut inputs,&mut provider,&control).unwrap());
        assert_eq!(provider.calls,2);
    }
    #[test]
    fn image_only_and_unpaired_job_choices_do_not_trigger_spurious_requests() {
        let dir = tempfile::tempdir().unwrap();
        let mut inputs = Inputs {
            rasters: vec![raster(dir.path(), None)],
            ..Default::default()
        };
        let mut opts = Options {
            choices: crate::template_project::suggest_rasters(&inputs.rasters, &[]),
            ..Default::default()
        };
        let mut provider = Provider {
            file: dir.path().join("unused"),
            calls: 0,
            fail: true,
        };
        assert_eq!(
            hydrate_missing(&opts, &mut inputs, &mut provider, true, &Control::default()).unwrap(),
            0
        );
        opts.choices[0].source = Source::Job(0);
        opts.choices[0].raster = None;
        assert_eq!(
            hydrate_missing(&opts, &mut inputs, &mut provider, true, &Control::default()).unwrap(),
            0
        );
        assert_eq!(provider.calls, 0);
        opts.choices[0].source = Source::Raster(0);
        opts.choices[0].raster = Some(0);
        opts.choices[0].profile = false;
        inputs.rasters[0].metadata["kickthemap_job_features_path"] = "missing.json".into();
        assert_eq!(
            hydrate_missing(
                &opts,
                &mut inputs,
                &mut provider,
                false,
                &Control::default()
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn hydration_follows_selected_effective_raster_instead_of_source_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fresh.json");
        std::fs::write(&path, RAW).unwrap();
        let mut inputs = Inputs {
            rasters: vec![raster(dir.path(), None), raster(dir.path(), Some(42))],
            ..Default::default()
        };
        let mut choices = crate::template_project::suggest_rasters(&inputs.rasters, &[]);
        choices.truncate(1);
        choices[0].source = Source::Raster(0);
        choices[0].raster = Some(1);
        let opts = Options {
            choices,
            ..Default::default()
        };
        let mut provider = Provider {
            file: path.clone(),
            calls: 0,
            fail: false,
        };
        assert_eq!(
            hydrate_missing(&opts, &mut inputs, &mut provider, true, &Control::default()).unwrap(),
            1
        );
        assert_eq!(provider.calls, 1);
        assert!(local_path(&inputs.rasters[0]).is_none());
        assert_eq!(local_path(&inputs.rasters[1]), Some(path));
    }
    #[test]
    fn hydration_failure_and_cancellation_never_apply_partial_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let mut inputs = Inputs {
            rasters: vec![raster(dir.path(), Some(42))],
            ..Default::default()
        };
        let opts = Options {
            choices: crate::template_project::suggest_rasters(&inputs.rasters, &[]),
            ..Default::default()
        };
        let before = inputs.rasters[0].metadata.clone();
        let mut provider = Provider {
            file: dir.path().join("unused"),
            calls: 0,
            fail: true,
        };
        assert!(
            hydrate_missing(&opts, &mut inputs, &mut provider, true, &Control::default()).is_err()
        );
        assert_eq!(inputs.rasters[0].metadata, before);
        let control = Control::default();
        control.cancel();
        assert!(hydrate_missing(&opts, &mut inputs, &mut provider, true, &control).is_err());
        assert_eq!(inputs.rasters[0].metadata, before);
    }
    #[test]
    fn later_invalid_response_keeps_every_raster_and_protected_source_unchanged() {
        struct TwoFiles {
            good: PathBuf,
            bad: PathBuf,
            calls: usize,
        }
        impl ProfileProvider for TwoFiles {
            fn download(&mut self, id: u64, _: &Control) -> Result<HydratedFile, String> {
                self.calls += 1;
                Ok(HydratedFile {
                    path: if id == 42 {
                        self.good.clone()
                    } else {
                        self.bad.clone()
                    },
                    title: "Fresh".into(),
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("valid.json");
        let bad = dir.path().join("invalid.json");
        std::fs::write(&good, RAW).unwrap();
        std::fs::write(&bad, b"{broken").unwrap();
        let mut inputs = Inputs {
            rasters: vec![raster(dir.path(), Some(42)), raster(dir.path(), Some(43))],
            protected_paths: vec!["original.klicproject".into()],
            ..Default::default()
        };
        let before = serde_json::to_value(&inputs.rasters).unwrap();
        let protected = inputs.protected_paths.clone();
        let opts = Options {
            choices: crate::template_project::suggest_rasters(&inputs.rasters, &[]),
            ..Default::default()
        };
        let mut provider = TwoFiles {
            good,
            bad,
            calls: 0,
        };
        assert!(
            hydrate_missing(&opts, &mut inputs, &mut provider, true, &Control::default()).is_err()
        );
        assert_eq!(provider.calls, 2);
        assert_eq!(serde_json::to_value(&inputs.rasters).unwrap(), before);
        assert_eq!(inputs.protected_paths, protected);
    }
}
