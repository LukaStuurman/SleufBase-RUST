//! Durable, identity-based template layout preferences for Rust project v3.
//! Never serialize volatile vector indices: reopening or importing projects
//! may reorder loaded jobs/rasters. Missing/ambiguous sources are never guessed.
use crate::{
    geotiff::GeoTiffInfo, model::Dataset,
    template_project::{self, BackgroundKind, Options, Source},
    virtual_trench::VirtualTrench,
};
use serde::{Deserialize,Serialize};
use std::{collections::HashSet,path::PathBuf};

#[derive(Clone,Debug,PartialEq,Eq,Hash,Serialize,Deserialize)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
enum SourceKey {
    Job {job_id:u64, source_path:PathBuf},
    Virtual {job_id:u64, label:String, #[serde(default)] origin:Option<u64>},
    Raster {path:PathBuf, #[serde(default)] origin:Option<u64>},
    Empty {ordinal:usize},
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedChoice {
    source:SourceKey,
    raster_path:Option<PathBuf>,
    #[serde(default)]
    raster_origin:Option<u64>,
    include:bool,
    title:String,
    #[serde(default)]
    label:Option<String>,
    #[serde(default)]
    legacy_slot:Option<usize>,
    profile:bool,
    #[serde(default)]
    manual_scale:Option<i64>,
    reverse:bool,
    comments:String,
    #[serde(default)]
    photos:Vec<crate::template_photo::Photo>,
}
fn default_paired()->bool{true}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplatePreferences {
    #[serde(default)]
    pub legacy_layout_only:bool,
    #[serde(default)]
    pub raster_layout:bool,
    #[serde(default)]
    pub server_mode:crate::template_server::Mode,
    #[serde(default="crate::cadastral_overview::template_defaults")]
    pub overview_settings:crate::cadastral_overview::Settings,
    #[serde(default)]
    pub profile_style:crate::settings_compat::ProfilePreferences,
    #[serde(default,skip_serializing_if="crate::template_branding::Logos::is_empty")]
    pub logos:crate::template_branding::Logos,
    pub template:PathBuf,
    #[serde(default="default_paired")]
    pub paired:bool,
    pub project:String,
    pub drawn_by:String,
    #[serde(default)]
    pub font_directories:Vec<PathBuf>,
    pub dpi:u32,
    pub scale:f64,
    pub background:BackgroundKind,
    pub include_bgt:bool,
    pub include_cadastral:bool,
    pub include_cad:bool,
    choices:Vec<SavedChoice>,
}
fn key(source:Source,jobs:&[Dataset],rasters:&[GeoTiffInfo],trenches:&[VirtualTrench])->Option<SourceKey>{
    match source {
        Source::Job(i) => jobs.get(i).map(|j|SourceKey::Job {
            job_id:j.job_id,source_path:j.source_path.clone(),
        }),
        Source::Virtual(i)=>trenches.get(i).map(|v|SourceKey::Virtual {
            job_id:v.job_id,label:v.label.clone(),origin:v.metadata.get("_sleufbase_python_tiff_index").and_then(serde_json::Value::as_u64),
        }),
        Source::Raster(i)=>rasters.get(i).map(|r|SourceKey::Raster{path:r.path.clone(),origin:r.metadata.get("_sleufbase_python_tiff_index").and_then(serde_json::Value::as_u64)}),
        Source::Empty(ordinal)=>Some(SourceKey::Empty{ordinal}),
    }
}
fn same_key(old:&SourceKey,current:&SourceKey)->bool{
    match (old,current){
        (SourceKey::Virtual{job_id:a,label:an,origin:ao},SourceKey::Virtual{job_id:b,label:bn,origin:bo})=>a==b&&if ao.is_some(){ao==bo}else{an==bn},
        (SourceKey::Raster{path:a,origin:ao},SourceKey::Raster{path:b,origin:bo})=>if ao.is_some(){ao==bo}else{a==b},
        _=>old==current,
    }
}
impl TemplatePreferences {
    pub fn capture(opts:&Options,jobs:&[Dataset],rasters:&[GeoTiffInfo],
        trenches:&[VirtualTrench])->Self
    {
        let choices=opts.choices.iter().filter_map(|item|{
            Some(SavedChoice {
                source:key(item.source,jobs,rasters,trenches)?,
                raster_path:item.raster.and_then(|idx|rasters.get(idx)).map(|r|r.path.clone()),
                raster_origin:item.raster.and_then(|idx|rasters.get(idx)).and_then(|r|r.metadata.get("_sleufbase_python_tiff_index")).and_then(serde_json::Value::as_u64),
                include:item.include,title:item.title.clone(),label:item.label.clone(),legacy_slot:item.legacy_slot,
                profile:item.profile,manual_scale:item.manual_scale,reverse:item.reverse,comments:item.comments.clone(),
                photos:item.photos.clone(),
            })
        }).collect();
        Self {legacy_layout_only:false,raster_layout:opts.raster_layout,server_mode:opts.server_mode,template:opts.template.clone(),paired:opts.paired,project:opts.project.clone(),
            overview_settings:opts.overview_settings.clone(),
            profile_style:opts.profile_style.clone(),
            logos:opts.logos.clone(),
            drawn_by:opts.drawn_by.clone(),font_directories:opts.font_directories.clone(),dpi:opts.dpi,scale:opts.scale,
            background:opts.background,include_bgt:opts.include_bgt,
            include_cadastral:opts.include_cadastral,include_cad:opts.include_cad,
            choices}
    }
    pub fn validate(&self)->Result<(),String>{
        self.profile_style.validate()?;
        crate::dxf_fonts::Catalog::validate_dirs(&self.font_directories)?;
        if self.choices.len()>20_000 {return Err("Te veel sjabloonvoorkeuren".into());}
        if self.project.len()>2048 || self.drawn_by.len()>2048
            || !self.scale.is_finite() || !(1. ..=100_000.).contains(&self.scale)
            || !(30..=600).contains(&self.dpi)
        {return Err("Ongeldige sjabloonvoorkeuren".into());}
        for item in &self.choices {
            if item.label.as_ref().is_some_and(|s|s.len()>256||s.chars().any(char::is_control)){return Err("Ongeldig opgeslagen proefsleufnummer".into());}
            if item.manual_scale.is_some_and(|s|!(1..=1_000_000).contains(&s)){return Err("Ongeldige opgeslagen profielschaal".into());}
            if item.photos.len()>crate::template_photo::MAX_PHOTOS_PER_SLOT
                || item.photos.iter().any(|p|p.turns>3) {
                return Err("Te veel of ongeldig gedraaide projectfoto's".into());
            }
            if item.title.len()>256 || item.comments.len()>4096
                || item.title.chars().any(char::is_control)
                || item.comments.chars().any(|c|c.is_control()&&c!='\n')
            {return Err("Ongeldige opgeslagen sjabloontekst".into());}
        }
        Ok(())
    }
    pub fn contains_raster_slot(&self,raster:&GeoTiffInfo)->bool{
        let origin=raster.metadata.get("_sleufbase_python_tiff_index").and_then(serde_json::Value::as_u64);
        self.choices.iter().any(|choice|match &choice.source{
            SourceKey::Raster{origin:Some(saved),..}=>origin==Some(*saved),
            SourceKey::Raster{path,origin:None}=>path==&raster.path,
            _=>false,
        })
    }

    pub fn restore(&self,jobs:&[Dataset],rasters:&[GeoTiffInfo],
        trenches:&[VirtualTrench])->Options
    {
        let mut options=Options {
            raster_layout:self.raster_layout,
            server_mode:self.server_mode,
            overview_settings:self.overview_settings.clone(),
            profile_style:self.profile_style.clone(),
            logos:self.logos.clone(),
            template:self.template.clone(),paired:self.paired,project:self.project.clone(),
            drawn_by:self.drawn_by.clone(),font_directories:self.font_directories.clone(),dpi:self.dpi,scale:self.scale,
            background:self.background,include_bgt:self.include_bgt,
            include_cadastral:self.include_cadastral,include_cad:self.include_cad,
            // A saved raster-only layout must not conceal imported KickTheMap jobs.
            choices:if self.raster_layout && jobs.is_empty() {
                template_project::suggest_rasters(rasters,trenches)
            }else{
                template_project::suggest(jobs,rasters,trenches)
            },
        };
        let mut candidates=std::mem::take(&mut options.choices);
        for old in &self.choices{
            let source=match &old.source{
                SourceKey::Empty{ordinal}=>Some(Source::Empty(*ordinal)),
                SourceKey::Raster{path,origin}=>{
                    let hits=rasters.iter().enumerate().filter(|(_,r)|if let Some(origin)=origin{r.metadata.get("_sleufbase_python_tiff_index").and_then(serde_json::Value::as_u64)==Some(*origin)}else{r.path==*path}).map(|(i,_)|i).collect::<Vec<_>>();
                    (hits.len()==1).then(||Source::Raster(hits[0]))
                },
                _=>None,
            };
            if let Some(source)=source{if !candidates.iter().any(|c|c.source==source){candidates.push(template_project::Choice{
                source,raster:match source{Source::Raster(i)=>Some(i),_=>None},include:true,title:old.title.clone(),label:old.label.clone(),legacy_slot:old.legacy_slot,profile:old.profile,manual_scale:old.manual_scale,reverse:old.reverse,comments:old.comments.clone(),photos:old.photos.clone(),
            });}}
        }
        let keys=candidates.iter().map(|item|key(item.source,jobs,rasters,trenches))
            .collect::<Vec<_>>();
        let mut selected=HashSet::new();
        for old in &self.choices {
            let indices=keys.iter().enumerate().filter(|(_,value)|value.as_ref().is_some_and(|value|same_key(&old.source,value)))
                .map(|(i,_)|i).collect::<Vec<_>>();
            // The same identity can appear twice in legacy imports. Never
            // remap settings to an arbitrary duplicate job or virtual trench.
            if indices.len()!=1 || !selected.insert(indices[0]) {continue;}
            let mut next=candidates[indices[0]].clone();
            next.include=old.include;
            next.title=old.title.clone();
            next.label=old.label.clone();
            next.legacy_slot=old.legacy_slot;
            next.profile=old.profile;
            next.manual_scale=old.manual_scale;
            next.reverse=old.reverse;
            next.comments=old.comments.clone();
            next.photos=old.photos.clone();
            if let Some(saved_path)=&old.raster_path {
                let found=rasters.iter().enumerate().filter(|(_,r)|if let Some(origin)=old.raster_origin{r.metadata.get("_sleufbase_python_tiff_index").and_then(serde_json::Value::as_u64)==Some(origin)}else{&r.path==saved_path})
                    .map(|(i,_)|i).collect::<Vec<_>>();
                next.raster=if found.len()==1 {Some(found[0])} else {None};
            } else {next.raster=None;}
            options.choices.push(next);
        }
        for (i,item) in candidates.into_iter().enumerate() {
            if !selected.contains(&i) {options.choices.push(item);}
        }
        options
    }
}

/// Activate the original order without changing the archived Python payload.
/// Raster-only and explicit blank slots are kept alongside virtual sources.
pub fn import_legacy(project:&crate::project::NativeProject)->Result<Option<TemplatePreferences>,String>{
    use crate::legacy_project::template_import as legacy;
    let Some(layout)=legacy::restore(project)? else{return Ok(None);};
    let settings=if let Some(settings)=&project.project_settings{settings.clone()}else{
        let defaults=crate::settings_compat::MigratedSettings::default();
        if let Some(raw)=project.legacy_archive.as_ref().and_then(|a|a.original.get("template_export_settings")){
            crate::settings_compat::with_project_overrides(&defaults,raw)?
        }else{defaults}
    };
    let mut options=Options{
        raster_layout:true,project:project.name.clone(),background:BackgroundKind::Pdok,
        profile_style:crate::settings_compat::ProfilePreferences::from_settings(&settings),
        overview_settings:crate::cadastral_overview::Settings::from_settings(&settings)?,
        ..Default::default()
    };
    let mut used_rasters=HashSet::new();let mut used_trenches=HashSet::new();
    for (ordinal,slot) in layout.slots.into_iter().enumerate(){
        let (source,raster,title)=match slot.source{
            legacy::Source::Empty=>(Source::Empty(ordinal),None,String::new()),
            legacy::Source::Raster(path)=>{
                let index=project.rasters.iter().enumerate().find(|(i,r)|!used_rasters.contains(i)&&r.path==path&&r.metadata==slot.metadata).map(|(i,_)|i).ok_or("Opgeslagen TIFF-slotbron kan niet eenduidig worden hersteld")?;
                used_rasters.insert(index);
                (Source::Raster(index),Some(index),format!("Proefsleuf {}",slot.number))
            },
            legacy::Source::Virtual{job_id,label}=>{
                let index=project.virtual_trenches.iter().enumerate().find(|(i,t)|!used_trenches.contains(i)&&t.job_id==job_id&&t.label==label&&t.metadata==slot.metadata).map(|(i,_)|i).ok_or("Opgeslagen virtueel slot kan niet eenduidig worden hersteld")?;
                used_trenches.insert(index);
                (Source::Virtual(index),None,label)
            },
        };
        options.choices.push(template_project::Choice{
            source,raster,title,label:(!matches!(source,Source::Empty(_))).then(||format!("PS{}{}",slot.number,slot.variant)),
            legacy_slot:slot.original_index,include:slot.included,profile:settings.export_preferences.template_include_cross_sections,
            manual_scale:slot.scale,reverse:false,comments:String::new(),photos:Vec::new(),
        });
    }
    let mut preferences=TemplatePreferences::capture(&options,&project.datasets,&project.rasters,&project.virtual_trenches);
    preferences.legacy_layout_only=true;
    preferences.validate()?;Ok(Some(preferences))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geo::GeoTransform,model::{Bounds,Feature,Geometry,Vertex}};
    fn job(id:u64,path:&str)->Dataset {
        Dataset{job_id:id,title:format!("PS{id}"),source_path:PathBuf::from(path),features:
            vec![Feature {object_name:"Kabel".into(),source_name:"KABEL".into(),
                geometry:Geometry::Point(Vertex{x:155000.,y:463000.,z:None}),
                attributes:Default::default()}]}
    }
    fn raster(path:&str)->GeoTiffInfo {
        GeoTiffInfo {path:PathBuf::from(path),width:16,height:16,
            transform:GeoTransform{a:1.,b:0.,c:154999.,d:0.,e:-1.,f:463010.},
            bounds:Bounds{min_x:154999.,min_y:462994.,max_x:155015.,max_y:463010.},
            epsg:Some(28992),kickthemap_job_id:None,kickthemap_job_title:String::new(),
            kickthemap_job_sidecar:None,metadata:Default::default()}
    }
    #[test]
    fn stable_identity_restores_sheet_order_and_moved_raster_indices() {
        let jobs=[job(1,"one.json"),job(2,"two.json")];
        let rasters=[raster("one.tif"),raster("two.tif")];
        let mut original=Options {choices:template_project::suggest(&jobs,&rasters,&[]),
            include_cad:false,..Default::default()};
        original.profile_style=crate::settings_compat::ProfilePreferences{marker_diameter:0.07,show_reference:false,use_custom_maaiveld:false,auto_fill_bgt:false,avoid_collisions:false,clip_markers:true,diameter_prefix:"DN".into()};
        original.logos=crate::template_branding::Logos{techbase:Some("company.png".into()),client:Some("client.jpg".into())};
        original.choices.swap(0,1);
        original.choices[0].raster=Some(1);
        original.choices[0].title="Custom PS2".into();
        original.choices[0].reverse=true;
        original.choices[0].photos.push(crate::template_photo::Photo {
            path:PathBuf::from("site-photo.png"),
            turns:3,area:crate::template_photo::Area::Map,
        });
        original.choices[1].raster=Some(0);
        original.choices[1].include=false;
        let prefs=TemplatePreferences::capture(&original,&jobs,&rasters,&[]);
        let encoded=serde_json::to_string(&prefs).unwrap();
        let prefs:TemplatePreferences=serde_json::from_str(&encoded).unwrap();
        assert!(prefs.validate().is_ok());
        let next=prefs.restore(&[jobs[1].clone(),jobs[0].clone()],
            &[rasters[1].clone(),rasters[0].clone()],&[]);
        assert_eq!(next.profile_style,original.profile_style);assert_eq!(next.logos,original.logos);
        assert_eq!(next.choices[0].title,"Custom PS2");
        assert_eq!(next.choices[0].raster,Some(0));
        assert!(next.choices[0].reverse);
        assert_eq!(next.choices[0].photos.len(),1);
        assert_eq!(next.choices[0].photos[0].turns,3);
        assert_eq!(next.choices[0].photos[0].area,crate::template_photo::Area::Map);
        assert_eq!(next.choices[0].photos[0].path,PathBuf::from("site-photo.png"));
        let mut legacy:serde_json::Value=serde_json::from_str(&encoded).unwrap();
        legacy.as_object_mut().unwrap().remove("profile_style");legacy.as_object_mut().unwrap().remove("logos");
        for choice in legacy["choices"].as_array_mut().unwrap() {
            choice.as_object_mut().unwrap().remove("photos");
        }
        let older:TemplatePreferences=serde_json::from_value(legacy).unwrap();
        assert_eq!(older.profile_style,Default::default());assert!(older.logos.is_empty());
        assert!(older.restore(&jobs,&rasters,&[]).choices.iter().all(|x|x.photos.is_empty()));
        assert_eq!(next.choices[1].raster,Some(1));
        assert!(!next.choices[1].include);
        assert!(!next.include_cad);
    }
    #[test]
    fn missing_source_is_skipped_and_missing_tiff_is_never_replaced_by_guess() {
        let jobs=[job(7,"seven.json")];
        let originals=[raster("A.tif")];
        let mut o=Options {choices:template_project::suggest(&jobs,&originals,&[]),
            ..Default::default()};
        o.choices[0].raster=Some(0);
        let prefs=TemplatePreferences::capture(&o,&jobs,&originals,&[]);
        let restored=prefs.restore(&jobs,&[raster("B.tif")],&[]);
        assert!(restored.choices[0].raster.is_none());
        assert_eq!(prefs.restore(&[],&originals,&[]).choices[0].source,Source::Raster(0));
        let restored=prefs.restore(&[job(7,"seven.json"),job(8,"new.json")],&originals,&[]);
        assert_eq!(restored.choices.len(),3);
        assert!(restored.choices.iter().any(|c|c.source==Source::Raster(0)));
    }
    #[test]
    fn physical_origin_restores_duplicate_paths_links_and_legacy_slots(){
        let mut a=raster("same.tif");a.metadata=serde_json::json!({"_sleufbase_python_tiff_index":3,"unknown":"A"});
        let mut b=a.clone();b.metadata=serde_json::json!({"_sleufbase_python_tiff_index":7,"unknown":"B"});
        let rasters=[a,b];let mut options=Options{raster_layout:true,choices:template_project::suggest_rasters(&rasters,&[]),..Default::default()};
        options.choices[0].legacy_slot=Some(11);options.choices[0].comments="first".into();options.choices[1].legacy_slot=Some(19);options.choices[1].comments="second".into();options.choices.swap(0,1);
        let prefs=TemplatePreferences::capture(&options,&[],&rasters,&[]);let prefs:TemplatePreferences=serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
        let moved=[rasters[1].clone(),rasters[0].clone()];let restored=prefs.restore(&[],&moved,&[]);
        assert_eq!(restored.choices[0].source,Source::Raster(0));assert_eq!(restored.choices[0].raster,Some(0));assert_eq!(restored.choices[0].legacy_slot,Some(19));assert_eq!(restored.choices[0].comments,"second");
        assert_eq!(restored.choices[1].source,Source::Raster(1));assert_eq!(restored.choices[1].raster,Some(1));assert_eq!(restored.choices[1].legacy_slot,Some(11));assert_eq!(moved[0].metadata["unknown"],"B");
        let mut legacy=serde_json::to_value(&prefs).unwrap();for row in legacy["choices"].as_array_mut().unwrap(){row.as_object_mut().unwrap().remove("raster_origin");row["source"].as_object_mut().unwrap().remove("origin");}
        let legacy:TemplatePreferences=serde_json::from_value(legacy).unwrap();let fallback=legacy.restore(&[],&moved,&[]);assert!(fallback.choices.iter().all(|c|c.legacy_slot.is_none()&&c.comments.is_empty()));
        let single=legacy.restore(&[],&[moved[0].clone()],&[]);assert_eq!(single.choices[0].comments,"second");
    }

}
