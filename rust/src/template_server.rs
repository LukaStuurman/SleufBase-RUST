//! Active template service composition, shared by both desktop planners.
//! Fetch everything before replacing the prepared request; failures cannot leave
//! partially refreshed geometry or publish any files.
#[path="template_address.rs"]
pub mod address;
use crate::{Bounds, cadastral::{CadLinework,CadLabel}, export_control::Control, template_export::Request};
use serde::{Serialize,Deserialize};
use std::collections::HashSet;
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Mode { #[default] Fresh, Offline }
pub type Paths=Vec<Vec<(f64,f64)>>;
#[derive(Clone,Copy,Debug)]
pub enum FetchKind { Outlines, Surfaces, Parcels, Labels, Roads, Terrain }
pub enum FetchResult { Paths(Paths), Surfaces(Vec<crate::bgt_features::SurfaceFeature>), Labels(Vec<CadLabel>) }
pub trait Provider {
    fn address(&mut self,_bounds:Bounds,_control:&Control)->Result<Option<String>,String>{Ok(None)}
    fn visit(&mut self,kind:FetchKind,bounds:&[Bounds],control:&Control,accept:&mut dyn FnMut(usize,FetchResult)->Result<(),String>)->Result<(),String>{
        for (index,&b) in bounds.iter().enumerate(){control.check()?;let result=match kind {
            FetchKind::Outlines=>FetchResult::Paths(self.outlines(b,control)?),
            FetchKind::Surfaces=>FetchResult::Surfaces(self.surfaces(b,control)?),
            FetchKind::Parcels=>FetchResult::Paths(self.parcels(b,control)?),
            FetchKind::Labels=>FetchResult::Labels(self.labels(b,control)?),
            FetchKind::Roads=>FetchResult::Paths(self.roads(b,control)?),
            FetchKind::Terrain=>FetchResult::Paths(self.terrain(b,control)?),
        };accept(index,result)?;}Ok(())
    }
    fn outlines(&mut self,b:Bounds,c:&Control)->Result<Paths,String>;
    fn surfaces(&mut self,b:Bounds,c:&Control)->Result<Vec<crate::bgt_features::SurfaceFeature>,String>;
    fn parcels(&mut self,b:Bounds,c:&Control)->Result<Paths,String>;
    fn labels(&mut self,b:Bounds,c:&Control)->Result<Vec<CadLabel>,String>;
    fn roads(&mut self,b:Bounds,c:&Control)->Result<Paths,String>;
    fn terrain(&mut self,b:Bounds,c:&Control)->Result<Paths,String>;
}
#[derive(Default)]
pub struct Native { addresses:Option<crate::location_search::Client>,tiles:Option<std::sync::Arc<crate::bgt_tile_client::Client>>,context:Option<std::sync::Arc<std::sync::Mutex<crate::profile_context::Client>>> }
impl Native {
    fn tiles(&mut self)->Result<&crate::bgt_tile_client::Client,String>{
        if self.tiles.is_none(){self.tiles=Some(std::sync::Arc::new(crate::bgt_tile_client::Client::new(crate::bgt_tile_client::Options{workers:1,..Default::default()})?));}
        Ok(self.tiles.as_ref().unwrap())
    }
    fn context(&mut self,kind:crate::profile_context::Kind,b:Bounds,c:&Control)->Result<Paths,String>{
        if self.context.is_none(){self.context=Some(std::sync::Arc::new(std::sync::Mutex::new(crate::profile_context::Client::new()?)));}
        let client=self.context.as_ref().unwrap().clone();
        let paths=crate::cadastral::cancellable_request(c.flag(),move|stop|client.lock().map_err(|_|"Profielcontext is niet beschikbaar".to_owned())?.fetch(kind,b,&stop))??;
        let mut out=Vec::new();
        for path in paths.iter(){c.check()?;out.push(path.iter().map(|&[x,y]|(x,y)).collect());}
        Ok(out)
    }
}
impl Provider for Native {
    fn address(&mut self,bounds:Bounds,control:&Control)->Result<Option<String>,String>{
        if self.addresses.is_none(){self.addresses=crate::location_search::Client::new().ok();}
        let Some(client)=self.addresses.as_ref()else{return Ok(None);};
        address::lookup(bounds,control,|x,y,c|client.reverse(x,y,1,c.flag()))
    }
    fn visit(&mut self,kind:FetchKind,bounds:&[Bounds],control:&Control,accept:&mut dyn FnMut(usize,FetchResult)->Result<(),String>)->Result<(),String>{
        let cap=match kind {FetchKind::Surfaces|FetchKind::Outlines=>10,_=>12};
        let policy=crate::resource_policy::get();
        let available=match kind {FetchKind::Roads|FetchKind::Terrain=>policy.local_geometry_workers,_=>policy.network_workers};
        let workers=available.min(cap).max(1).min(bounds.len().max(1));
        // One tile pool/cache is shared across bounds. One inner tile worker
        // keeps concurrent HTTP requests within the outer source worker cap.
        let tiles=if matches!(kind,FetchKind::Outlines|FetchKind::Surfaces){self.tiles()?;self.tiles.clone()}else{None};
        let clients=(0..workers).map(|_|std::sync::Mutex::new(Native{addresses:None,tiles:tiles.clone(),context:None})).collect::<Vec<_>>();
        let batch=Control::default();
        crate::template_fetch::ordered(bounds.len(),workers,control,|worker,index,_|{
            let mut client=clients[worker].lock().map_err(|_|"Sjabloonfetch-client gesloten".to_owned())?;
            let b=bounds[index];let result=match kind{
                FetchKind::Outlines=>FetchResult::Paths(client.outlines(b,&batch)?),
                FetchKind::Surfaces=>FetchResult::Surfaces(client.surfaces(b,&batch)?),
                FetchKind::Parcels=>FetchResult::Paths(client.parcels(b,&batch)?),
                FetchKind::Labels=>FetchResult::Labels(client.labels(b,&batch)?),
                FetchKind::Roads=>FetchResult::Paths(client.roads(b,&batch)?),
                FetchKind::Terrain=>FetchResult::Paths(client.terrain(b,&batch)?),
            };Ok((index,result))
        },|(index,result)|accept(index,result),||{batch.cancel();})
    }
    fn outlines(&mut self,b:Bounds,c:&Control)->Result<Paths,String>{Ok(self.tiles()?.fetch_paths(b,c.flag())?.into_iter().map(|p|p.into_iter().map(|[x,y]|(x,y)).collect()).collect())}
    fn surfaces(&mut self,b:Bounds,c:&Control)->Result<Vec<crate::bgt_features::SurfaceFeature>,String>{self.tiles()?.fetch_surface_features(b,c.flag())}
    fn parcels(&mut self,b:Bounds,c:&Control)->Result<Paths,String>{Ok(crate::cadastral::fetch_template_cadastral(b,false,c.flag())?.linework.into_iter().flat_map(|g|g.paths).collect())}
    fn labels(&mut self,b:Bounds,c:&Control)->Result<Vec<CadLabel>,String>{Ok(crate::cadastral::fetch_template_cadastral(b,true,c.flag())?.labels)}
    fn roads(&mut self,b:Bounds,c:&Control)->Result<Paths,String>{self.context(crate::profile_context::Kind::Roads,b,c)}
    fn terrain(&mut self,b:Bounds,c:&Control)->Result<Paths,String>{self.context(crate::profile_context::Kind::Terrain,b,c)}
}
fn union(a:Bounds,b:Bounds)->Bounds{Bounds{min_x:a.min_x.min(b.min_x),min_y:a.min_y.min(b.min_y),max_x:a.max_x.max(b.max_x),max_y:a.max_y.max(b.max_y)}}
fn intersects(a:Bounds,b:Bounds)->bool{a.min_x<=b.max_x&&a.max_x>=b.min_x&&a.min_y<=b.max_y&&a.max_y>=b.min_y}
fn pad(b:Bounds,p:f64)->Bounds{Bounds{min_x:b.min_x-p,min_y:b.min_y-p,max_x:b.max_x+p,max_y:b.max_y+p}}
fn merged(boxes:Vec<Bounds>)->Vec<Bounds>{
    let mut out:Vec<Bounds>=Vec::new();
    for mut b in boxes {loop{let mut changed=false;out.retain(|a|{if intersects(b,*a){b=union(b,*a);changed=true;false}else{true}});if !changed{break;}}out.push(b);}out
}
/// Active startup patch v3: ten times the local 35–75 m margin.
/// Merge only intersecting boxes; distant trenches stay local regardless of count.
pub fn fetch_bounds(bounds:&[Bounds])->Result<Vec<Bounds>,String>{
    if bounds.is_empty()||bounds.len()>128{return Err("Ongeldig aantal serveruitsneden".into());}
    for b in bounds {if ![b.min_x,b.min_y,b.max_x,b.max_y].iter().all(|v|v.is_finite()&&v.abs()<1e9)||b.max_x<=b.min_x||b.max_y<=b.min_y{return Err("Ongeldige serveruitsnede".into());}}
    Ok(merged(bounds.iter().map(|&b|pad(b,((b.max_x-b.min_x).max(b.max_y-b.min_y)*8.).clamp(35.,75.)*10.)).collect()))
}

fn key(v:f64)->u64{let value=format!("{v:.3}").parse::<f64>().expect("finite validated coordinate");if value==0.{0}else{value.to_bits()}}
fn dedupe(paths:Paths,undirected:bool,c:&Control)->Result<Paths,String>{
    let mut seen=HashSet::new();let mut out=Vec::new();
    for p in paths{c.check()?;if p.len()<2{continue;}let mut a=Vec::new();a.try_reserve_exact(p.len()).map_err(|_|"Onvoldoende geheugen voor servergeometrie")?;
        for (i,&(x,y)) in p.iter().enumerate(){if i%16384==0{c.check()?;}a.push((key(x),key(y)));}
        if undirected&&a.iter().rev().cmp(a.iter()).is_lt(){a.reverse();}
        if seen.insert(a){out.push(p);}
    }Ok(out)
}
fn append_paths(to:&mut Paths,paths:Paths,total:&mut usize,c:&Control)->Result<(),String>{
    for p in paths{c.check()?;*total=total.saturating_add(p.len());if *total>2_000_000||p.iter().any(|&(x,y)|!x.is_finite()||!y.is_finite()||x.abs()>1e9||y.abs()>1e9){return Err("Te veel of ongeldige servergeometrie".into());}to.push(p);}Ok(())
}
pub fn refresh(request:&mut Request,mode:Mode,bgt:bool,cadastral:bool,provider:&mut impl Provider,control:&Control)->Result<(),String>{
    control.check()?;if mode==Mode::Offline{return Ok(());}
    let bounds=request.slots.iter().flatten().map(|s|s.bounds).collect::<Vec<_>>();
    let boxes=fetch_bounds(&bounds)?;
    let terrain_needed=request.slots.iter().flatten().any(|s|s.profile.is_some());
    let surfaces_needed=request.slots.iter().flatten().any(|s|s.profile.as_ref().is_some_and(|p|p.draw&&p.auto_fill_bgt));
    let mut raw_points=0usize;let mut label_bytes=0usize;
    let mut outlines=Vec::new();let mut parcels=Vec::new();let mut labels=Vec::new();let mut surfaces=Vec::new();let mut roads=Vec::new();let mut empty=Vec::new();let mut terrain=Vec::new();
    // Keep the active original's phase order, including the road-empty fallback.
    if bgt{control.set_detail("BGT-vectoren ophalen");provider.visit(FetchKind::Outlines,&boxes,control,&mut |_,result|{let FetchResult::Paths(paths)=result else{return Err("Ongeldig serverresultaat".into());};append_paths(&mut outlines,paths,&mut raw_points,control)})?;}
    if bgt&&surfaces_needed{control.set_detail("BGT-ondergrond ophalen");provider.visit(FetchKind::Surfaces,&boxes,control,&mut |_,result|{
        let FetchResult::Surfaces(fetched)=result else{return Err("Ongeldig serverresultaat".into());};
        for f in fetched{raw_points=raw_points.saturating_add(crate::bgt_geometry::vertex_count(&f.surface.geometry,control.flag())?);if raw_points>2_000_000||surfaces.len()>=100_000{return Err("Te veel BGT-ondergrond voor één export".into());}surfaces.push(f);}Ok(())
    })?;}
    if cadastral{control.set_detail("Kadastrale grenzen ophalen");provider.visit(FetchKind::Parcels,&boxes,control,&mut |_,result|{let FetchResult::Paths(paths)=result else{return Err("Ongeldig serverresultaat".into());};append_paths(&mut parcels,paths,&mut raw_points,control)})?;}
    if cadastral{control.set_detail("Kadastrale teksten ophalen");provider.visit(FetchKind::Labels,&boxes,control,&mut |_,result|{
        let FetchResult::Labels(fetched)=result else{return Err("Ongeldig serverresultaat".into());};
        if labels.len().saturating_add(fetched.len())>100_000{return Err("Te veel kadastrale teksten voor één export".into());}
        for l in fetched{control.check()?;crate::object_dxf::layer_name(&l.layer_name)?;label_bytes=label_bytes.saturating_add(l.text.len()).saturating_add(l.layer_name.len());
        if l.text.len()>2049||label_bytes>16*1024*1024||l.text.chars().any(char::is_control)||![l.position.0,l.position.1,l.rotation].iter().all(|v|v.is_finite())||l.position.0.abs()>1e9||l.position.1.abs()>1e9{return Err("Ongeldige of te grote kadastrale tekstbatch".into());}labels.push(l);}Ok(())
    })?;}
    control.set_detail("Weg-hartlijnen ophalen");provider.visit(FetchKind::Roads,&boxes,control,&mut |index,result|{let FetchResult::Paths(paths)=result else{return Err("Ongeldig serverresultaat".into());};if paths.is_empty(){empty.push(boxes[index]);}append_paths(&mut roads,paths,&mut raw_points,control)})?;
    if terrain_needed{control.set_detail("Terreinranden ophalen");provider.visit(FetchKind::Terrain,&empty,control,&mut |_,result|{let FetchResult::Paths(paths)=result else{return Err("Ongeldig serverresultaat".into());};append_paths(&mut terrain,paths,&mut raw_points,control)})?;}
    outlines=dedupe(outlines,true,control)?;parcels=dedupe(parcels,true,control)?;roads=dedupe(roads,false,control)?;terrain=dedupe(terrain,false,control)?;
    // The active local-client patch unions the same source feature fetched in
    // multiple boxes. Keep first-feature order and identity until after union.
    let all=boxes.iter().copied().reduce(union).unwrap();
    let mut positions=std::collections::HashMap::new();
    let mut combined:Vec<crate::bgt_features::SurfaceFeature>=Vec::new();
    for f in surfaces {control.check()?;let k=(f.layer_name.clone(),f.feature_id.clone(),f.surface.physical_appearance.clone());
        if let Some(&i)=positions.get(&k){let old:&mut crate::bgt_features::SurfaceFeature=&mut combined[i];if let Some(g)=crate::bgt_geometry::normalize(&[&old.surface.geometry,&f.surface.geometry],all,control.flag())?{old.surface.geometry=g;}}
        else{positions.insert(k,combined.len());combined.push(f);}
    }
    let surfaces=combined.into_iter().map(|f|f.surface).collect::<Vec<_>>();
    let mut seen=HashSet::new();labels.retain(|l|seen.insert((l.layer_name.clone(),l.text.clone(),key(l.position.0),key(l.position.1),key(l.rotation))));
    let mut points=outlines.iter().chain(&parcels).chain(&roads).chain(&terrain).map(Vec::len).sum::<usize>();
    for f in &surfaces{control.check()?;points=points.saturating_add(crate::bgt_geometry::vertex_count(&f.geometry,control.flag())?);}
    if points>2_000_000||surfaces.len()>100_000||labels.len()>100_000{return Err("Serverdata overschrijdt het gezamenlijke exportbudget".into());}
    let mut linework=Vec::new();if !outlines.is_empty(){linework.push(CadLinework{layer_name:"BGT_VECTOR_OMTREK".into(),paths:outlines});}if !parcels.is_empty(){linework.push(CadLinework{layer_name:"KAD_GRENS".into(),paths:parcels});}
    let mut addresses=Vec::new();
    for slot in &request.slots{control.check()?;addresses.push(if let Some(slot)=slot{if slot.address.as_deref().is_some_and(|s|!s.is_empty()){slot.address.clone()}else{
        control.set_detail("Proefsleufadres ophalen");let result=provider.address(slot.bounds,control);control.check()?;result.ok().flatten().filter(|value|value.len()<=4096&&!value.chars().any(char::is_control))
    }}else{None});}
    control.check()?;
    for (slot,address) in request.slots.iter_mut().zip(addresses){if let Some(slot)=slot{slot.address=address;}}
    if let Some(overview)=request.overview.as_mut(){overview.linework=linework;overview.labels=labels;}
    request.roads=roads.into_iter().map(|p|p.into_iter().map(|(x,y)|[x,y]).collect()).collect();
    request.terrain_boundaries=terrain.into_iter().map(|p|p.into_iter().map(|(x,y)|[x,y]).collect()).collect();
    request.surfaces=surfaces;
    for slot in request.slots.iter_mut().flatten(){slot.surfaces=None;slot.surfaces_constructed=false;}
    Ok(())
}
