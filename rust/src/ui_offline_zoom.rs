//! Actual offline PDOK WMTS pyramid. Only highest-detail JPEGs come from
//! PDOK; lower zoom levels are composed locally when the user zooms out.
//! The small bundled overview and legacy photo caches remain as fallbacks.
//! Only georeferenced photos around the camera enter GPU memory.
use super::*;
use sleufbase_rust::offline_zoom as cache;
use std::{
    collections::{HashSet,VecDeque},
    sync::{atomic::{AtomicBool,AtomicU64,Ordering},Arc,mpsc::{self,Receiver,TryRecvError,SyncSender}},
    time::{Duration,Instant,SystemTime},
};

const MAX_LEVEL_TILES:u64=24;
// Three visible zoom levels may each require up to 24 small 256px tiles.
// A bounded 96-tile GPU LRU avoids rapid eviction while moving the camera.
const MAX_TEXTURES:usize=96;
const MAX_DECODES_PER_FRAME:usize=4;
const AUTO_RESCAN:Duration=Duration::from_secs(24*60*60);
const RETRY_AFTER_FAILURE:Duration=Duration::from_secs(24*60*60);
type Decoded=(cache::TileId,Result<Option<cache::ImageTile>,String>);
struct TextureTile{
    id:cache::TileId,
    modified:SystemTime,
    texture:egui::TextureHandle,
}
pub(super) struct State {
    loaded:VecDeque<TextureTile>,
    visible:Vec<(egui::TextureHandle,Bounds)>,
    decoding:HashSet<cache::TileId>,
    decoder:Option<SyncSender<cache::TileId>>,
    decoded:Option<Receiver<Decoded>>,
    active_zoom:u8,
    auto_enabled:bool,
    configured_area:Option<[i32;4]>,
    pending:Option<(u8,bool,Receiver<Result<(),String>>)>,
    cancel:Option<Arc<AtomicBool>>,
    queued:Option<(u8,[i32;4])>,
    progress:Arc<AtomicU64>,
    total:u64,
    notice:Option<String>,
    next_check:Instant,
}
impl Default for State{
    fn default()->Self{
        Self{
            loaded:VecDeque::new(),visible:Vec::new(),
            decoding:HashSet::new(),decoder:None,decoded:None,
            active_zoom:cache::DEFAULT_ZOOM,auto_enabled:false,
            configured_area:None,pending:None,cancel:None,queued:None,
            progress:Arc::new(AtomicU64::new(0)),total:0,
            notice:None,next_check:Instant::now(),
        }
    }
}
impl State{
    pub(super) fn tiles(&self)->&[(egui::TextureHandle,Bounds)]{&self.visible}
    pub(super) fn manual_active(&self)->bool{
        self.queued.is_some() || self.pending.as_ref().is_some_and(|(_,manual,_)|*manual)
    }
    pub(super) fn manual_status(&self)->Option<String>{
        if let Some((zoom,_))=self.queued{
            return Some(format!("PDOK niveau {zoom}: offline kaart wordt voorbereid…"));
        }
        if let Some((zoom,true,_))=&self.pending{
            let done=self.progress.load(Ordering::Relaxed);
            return Some(format!("Offline luchtfoto · zoom {zoom}: {done}/{} stappen (scherpe foto's downloaden en lokaal verkleinen). Bestaande kaart blijft beschikbaar.",self.total));
        }
        self.notice.clone()
    }
    /// Downloads only the selected highest-resolution level for this area.
    /// All lower zoom levels are synthesized on disk from sharp JPEGs,
    /// without additional requests to PDOK or changing auto-update opt-in.
    pub(super) fn download_now(&mut self,zoom:u8,area:[i32;4])->Result<(),String>{
        let estimate=cache::estimate(area,zoom)
            .ok_or("Ongeldig PDOK-kaartgebied")?;
        if !estimate.allowed {
            return Err(format!("Dit gebied vraagt ongeveer {} en {} tegels. Zoom verder in op de kaart of kies een lager detailniveau.",
                cache::format_mb(estimate.expected_bytes),estimate.tiles));
        }
        self.queued=Some((zoom,area));
        self.total=estimate.steps();
        self.progress.store(0,Ordering::Relaxed);
        self.notice=None;
        self.next_check=Instant::now();
        if let Some(cancel)=&self.cancel {cancel.store(true,Ordering::Relaxed);}
        Ok(())
    }
    pub(super) fn cancel_manual(&mut self){
        self.queued=None;
        if self.pending.as_ref().is_some_and(|(_,manual,_)|*manual){
            if let Some(cancel)=&self.cancel{cancel.store(true,Ordering::Relaxed);}
        }
        self.notice=Some("Handmatige download gestopt; eerder opgeslagen beelden blijven beschikbaar.".into());
    }
    fn start_decoder(&mut self){
        if self.decoder.is_some(){return;}
        let (tx,requests)=mpsc::sync_channel::<cache::TileId>(16);
        let (outputs,rx)=mpsc::sync_channel::<Decoded>(8);
        let root=cache::root();
        if std::thread::Builder::new()
            .name("sleufbase-offline-wmts-decode".into())
            .spawn(move||{
                while let Ok(id)=requests.recv(){
                    let result=cache::read_tile(&root,id);
                    if outputs.send((id,result)).is_err(){break;}
                }
            }).is_ok(){
            self.decoder=Some(tx);
            self.decoded=Some(rx);
        }
    }
    fn begin_download(&mut self,ctx:&egui::Context,zoom:u8,area:[i32;4],manual:bool){
        if self.pending.is_some(){return;}
        let plan=match cache::estimate(area,zoom){
            Some(plan) if plan.allowed =>plan,
            _=>{
                if manual{
                    self.notice=Some("Gebied te groot. Zoom verder in om de offline luchtfoto te downloaden.".into());
                }
                self.next_check=Instant::now()+RETRY_AFTER_FAILURE;
                return;
            },
        };
        let (tx,rx)=mpsc::sync_channel(1);
        let cancel=Arc::new(AtomicBool::new(false));
        let worker_cancel=Arc::clone(&cancel);
        let progress=Arc::clone(&self.progress);
        let repaint=ctx.clone();
        self.progress.store(0,Ordering::Relaxed);
        self.total=plan.steps();
        let started=std::thread::Builder::new()
            .name(if manual {"sleufbase-manual-wmts-download"}else{"sleufbase-auto-wmts-refresh"}.into())
            .spawn(move||{
                let result=cache::refresh(area,zoom,manual,&worker_cancel,
                    |done,_total|{
                        progress.store(done,Ordering::Relaxed);
                        if done%5==0 || done==plan.steps(){
                            repaint.request_repaint();
                        }
                    });
                let _=tx.send(result);
                repaint.request_repaint();
            });
        match started{
            Ok(_)=>{
                if manual{self.queued=None;}
                self.cancel=Some(cancel);
                self.pending=Some((zoom,manual,rx));
            }
            Err(error)=>{
                if manual{
                    self.queued=None;
                    self.notice=Some(format!("Offline download starten mislukt: {error}"));
                }
                self.next_check=Instant::now()+RETRY_AFTER_FAILURE;
            }
        }
    }
    fn poll_job(&mut self,ctx:&egui::Context){
        let ready=self.pending.as_ref().and_then(|(zoom,manual,rx)|
            match rx.try_recv(){
                Ok(result)=>Some((*zoom,*manual,result)),
                Err(TryRecvError::Disconnected)=>
                    Some((*zoom,*manual,Err("Offline kaartworker gestopt".into()))),
                Err(TryRecvError::Empty)=>None,
            });
        if let Some((zoom,manual,result))=ready{
            let cancelled=self.cancel.as_ref()
                .is_some_and(|flag|flag.load(Ordering::Relaxed));
            self.cancel=None;
            self.pending=None;
            let success=result.is_ok();
            if manual&&!cancelled{
                let done=self.progress.load(Ordering::Relaxed);
                self.notice=Some(match result{
                    Ok(())=>format!("Offline luchtfoto niveau {zoom} opgeslagen ({done}/{} tegels).",self.total),
                    Err(e)=>format!("Offline luchtfoto: {done}/{} tegels verwerkt; {e}. Oude tegels zijn behouden.",self.total),
                });
            }
            self.next_check=if self.queued.is_some() || (cancelled&&self.auto_enabled){
                Instant::now()
            }else if success{Instant::now()+AUTO_RESCAN}
            else{Instant::now()+RETRY_AFTER_FAILURE};
            ctx.request_repaint();
        }
    }
    fn poll_download(&mut self,ctx:&egui::Context){
        self.poll_job(ctx);
        if self.pending.is_some() || Instant::now()<self.next_check{return;}
        if let Some((zoom,area))=self.queued{
            self.begin_download(ctx,zoom,area,true);
        }else if self.auto_enabled {
            if let Some(area)=self.configured_area{
                self.begin_download(ctx,self.active_zoom,area,false);
            }
        }
    }
    fn requested_tiles(&self,ctx:&egui::Context,view:Bounds)->Vec<cache::TileId>{
        let width=view.max_x-view.min_x;
        let height=view.max_y-view.min_y;
        let display=ctx.screen_rect();
        if width<=0.||height<=0.||display.width()<=0. {return Vec::new();}
        let mpp=(width/display.width() as f64)
            .max(height/display.height().max(1.) as f64);
        let best=((3440.64/mpp).log2().ceil())
            .clamp(0.,cache::MAX_ZOOM as f64) as u8;
        let target=self.active_zoom.min(best);
        // A complete tile layer is preferred over the low-res overview.
        // Never queue thousands of textures for a very broad view.
        let highest=(0..=target).rev()
            .find(|z|cache::grid(view,*z).is_some_and(|g|g.count()<=MAX_LEVEL_TILES));
        let Some(highest)=highest else{return Vec::new();};
        // Locally downsampled PNG tiles cover zoom-out without network
        // requests. Also recognize saved tiles at the *edge* of the viewport:
        // the centre can fall outside a small downloaded offline area.
        let root=cache::root();
        let available=(0..=highest).rev().find(|&level|
            cache::tiles_for_view(view,level,MAX_LEVEL_TILES as usize)
                .iter().any(|id|cache::has_tile(&root,*id)));
        let best=available.unwrap_or(highest);
        let mut out=Vec::new();
        for zoom in best.saturating_sub(2)..=best{
            out.extend(cache::tiles_for_view(view,zoom,MAX_LEVEL_TILES as usize));
        }
        out
    }
    fn poll_visible(&mut self,ctx:&egui::Context,view:Option<Bounds>){
        let Some(view)=view else{self.visible.clear();return;};
        self.start_decoder();
        let ids=self.requested_tiles(ctx,view);
        for _ in 0..MAX_DECODES_PER_FRAME {
            let next=self.decoded.as_ref().map(|rx|rx.try_recv());
            let (id,result)=match next{
                Some(Ok(value))=>value,
                Some(Err(TryRecvError::Disconnected))=>{
                    self.decoding.clear();self.decoder=None;self.decoded=None;break;
                },
                _=>break,
            };
            self.decoding.remove(&id);
            if let Ok(Some(tile))=result{
                if ids.contains(&id) {
                    let texture=ctx.load_texture(
                        format!("Offline PDOK WMTS {}:{}:{}",id.zoom,id.col,id.row),
                        egui::ColorImage::from_rgba_unmultiplied(
                            [tile.image.width() as usize,tile.image.height() as usize],
                            tile.image.as_raw()),
                        egui::TextureOptions::LINEAR);
                    self.loaded.retain(|entry|entry.id!=id);
                    self.loaded.push_back(TextureTile{
                        id,modified:tile.modified,texture,
                    });
                    ctx.request_repaint();
                }
            }
        }
        let root=cache::root();
        for &id in &ids{
            let Some(stamp)=cache::tile_modified(&root,id) else{continue;};
            if let Some(index)=self.loaded.iter().position(|t|
                t.id==id&&t.modified==stamp){
                if let Some(tile)=self.loaded.remove(index){
                    self.loaded.push_back(tile);
                }
                continue;
            }
            if self.decoding.contains(&id){continue;}
            if self.decoder.as_ref().is_some_and(|tx|tx.try_send(id).is_ok()){
                self.decoding.insert(id);
            }
        }
        while self.loaded.len()>MAX_TEXTURES{self.loaded.pop_front();}
        self.visible.clear();
        // Low levels first: locally reduced RGBA/PNG fills the underlying
        // area, and downloaded high-resolution JPEG tiles paint over it.
        for id in ids{
            if let Some(tile)=self.loaded.iter().find(|entry|entry.id==id){
                self.visible.push((tile.texture.clone(),id.bounds()));
            }
        }
        if !self.decoding.is_empty(){
            ctx.request_repaint_after(Duration::from_millis(70));
        }
    }
    pub(super) fn poll(&mut self,ctx:&egui::Context,view:Option<Bounds>,
        auto_enabled:bool,zoom:u8,area:Option<[i32;4]>){
        if self.active_zoom!=zoom {
            self.active_zoom=zoom.min(cache::MAX_ZOOM);
            self.loaded.clear();
            self.visible.clear();
            if let Some(cancel)=&self.cancel{cancel.store(true,Ordering::Relaxed);}
            self.next_check=Instant::now();
        }
        if self.auto_enabled!=auto_enabled || self.configured_area!=area {
            if let Some(cancel)=&self.cancel{cancel.store(true,Ordering::Relaxed);}
            self.auto_enabled=auto_enabled;
            self.configured_area=area;
            if auto_enabled {self.next_check=Instant::now();}
        }
        self.poll_download(ctx);
        self.poll_visible(ctx,view);
        if self.pending.is_some(){
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn manual_zoom_refresh_is_opt_in_independent_and_bounded(){
        let mut state=State::default();
        assert!(!state.auto_enabled);
        assert!(state.download_now(15,[155000,463000,155250,463200]).is_ok());
        assert!(state.manual_active());
        assert!(!state.auto_enabled);
        let oversized=[155000,463000,156000,464000];
        assert!(state.download_now(19,oversized).is_err());
        state.cancel_manual();
        assert!(!state.manual_active());
    }
}