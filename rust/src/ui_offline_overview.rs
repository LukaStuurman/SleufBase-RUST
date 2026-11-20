//! Always-available bundled PDOK photo and a stronger ~50 MiB offline raster.
//! The high-resolution version is split into tiles, decoded off the egui
//! thread, and shown underneath the sharp online WMTS tiles. Both background
//! download and half-year refresh remain optional for offline operation.
use super::*;
use sleufbase_rust::{offline_overview,offline_detail};
use std::{
    collections::{HashSet,VecDeque},
    sync::{atomic::{AtomicBool,AtomicUsize,Ordering},Arc,Mutex},
    time::{Instant,SystemTime},
};

const RETRY_AFTER_FAILURE:Duration=Duration::from_secs(24*60*60);
const MAX_GPU_TILES:usize=18;
const MIN_TILE_SCREEN_POINTS:f64=130.;
const DECODES_PER_FRAME:usize=2;
type Decoded=(offline_detail::TileId,u16,Result<Option<offline_detail::ImageTile>,String>);
struct LoadedTile{
    id:offline_detail::TileId,
    modified:SystemTime,
    texture:egui::TextureHandle,
}
pub(super) struct State {
    initialized:bool,
    texture:Option<egui::TextureHandle>,
    last_success:SystemTime,
    pending:Option<Receiver<Result<offline_overview::Overview,String>>>,
    retry_after:Instant,
    // Fixed-RD 1536px tiles, capped on disk at 50 MiB. Only visible ones
    // become GPU textures, so a 18,432px-wide map is never uploaded at once.
    tiles:Vec<(egui::TextureHandle,Bounds)>,
    loaded:VecDeque<LoadedTile>,
    decoding:HashSet<(offline_detail::TileId,u16)>,
    decoder:Option<mpsc::SyncSender<(offline_detail::TileId,u16)>>,
    decoded:Option<Receiver<Decoded>>,
    detail_worker:Option<(u16,bool,Receiver<Result<(),String>>)>,
    detail_cancel:Option<Arc<AtomicBool>>,
    detail_next_sweep:Instant,
    detail_focus:Arc<Mutex<[f64;2]>>,
    manual_queued:Option<u16>,
    manual_progress:Arc<AtomicUsize>,
    manual_notice:Option<String>,
    force_overview:bool,
    manual_preview_pending:bool,
    auto_enabled:bool,
    active_level:u16,
}
impl Default for State{
    fn default()->Self{
        Self{initialized:false,texture:None,
            last_success:offline_overview::seed_timestamp(),
            pending:None,retry_after:Instant::now(),
            tiles:Vec::new(),loaded:VecDeque::new(),
            decoding:HashSet::new(),decoder:None,decoded:None,
            detail_worker:None,detail_cancel:None,
            detail_next_sweep:Instant::now(),
            detail_focus:Arc::new(Mutex::new([155_000.,463_000.])),
            manual_queued:None,manual_progress:Arc::new(AtomicUsize::new(0)),
            manual_notice:None,force_overview:false,manual_preview_pending:false,
            auto_enabled:false,active_level:50,
        }
    }
}
impl State{
    pub(super) fn texture(&self)->Option<&egui::TextureHandle>{
        self.texture.as_ref()
    }
    pub(super) fn tiles(&self)->&[(egui::TextureHandle,Bounds)]{
        &self.tiles
    }
    /// A manual refresh never changes the scheduled-update opt-in.
    /// It is requested explicitly from Settings and rebuilds all tiles,
    /// including fresh ones, while keeping old JPEGs visible until replaced.
    pub(super) fn download_now(&mut self,mb:u16){
        if !offline_detail::valid_level(mb){return;}
        self.manual_queued=Some(mb);
        self.manual_progress.store(0,Ordering::Relaxed);
        self.manual_notice=None;
        self.force_overview=true;
        // A button press bypasses any earlier automatic retry-backoff.
        self.retry_after=Instant::now();
        self.detail_next_sweep=Instant::now();
        if let Some(cancel)=&self.detail_cancel{
            cancel.store(true,Ordering::Relaxed);
        }
    }
    pub(super) fn cancel_manual(&mut self){
        self.manual_queued=None;
        if let Some((_,true,_))=self.detail_worker.as_ref(){
            if let Some(cancel)=&self.detail_cancel{cancel.store(true,Ordering::Relaxed);}
        }
        self.force_overview=false;
        self.manual_notice=Some("Handmatige download geannuleerd; bestaande kaart blijft beschikbaar.".into());
        self.detail_next_sweep=Instant::now()+RETRY_AFTER_FAILURE;
    }
    pub(super) fn manual_active(&self)->bool{
        self.manual_queued.is_some() || self.detail_worker.as_ref()
            .is_some_and(|(_,manual,_)|*manual) || self.manual_preview_pending
            || self.force_overview
    }
    pub(super) fn manual_status(&self)->Option<String>{
        if let Some(mb)=self.manual_queued{
            return Some(format!("Offline luchtfoto ({mb} MB): download wordt gestart…"));
        }
        if let Some((mb,true,_))=&self.detail_worker{
            let done=self.manual_progress.load(Ordering::Relaxed);
            return Some(format!("Nieuwe offline luchtfoto ({mb} MB) downloaden: {done}/144 tegels. Bestaande kaart blijft zichtbaar."));
        }
        if self.manual_preview_pending || self.force_overview{
            return Some("Offline overzichtsfoto vernieuwen…".into());
        }
        self.manual_notice.clone()
    }
    fn apply(&mut self,ctx:&egui::Context,overview:offline_overview::Overview){
        let image=egui::ColorImage::from_rgba_unmultiplied(
            [overview.image.width() as usize,overview.image.height() as usize],
            overview.image.as_raw());
        if let Some(texture)=&mut self.texture {
            texture.set(image,egui::TextureOptions::LINEAR);
        }else{
            self.texture=Some(ctx.load_texture("PDOK offline luchtfoto overzicht",
                image,egui::TextureOptions::LINEAR));
        }
        self.last_success=overview.updated_at;
        ctx.request_repaint();
    }
    fn start_decoder(&mut self){
        if self.decoder.is_some(){return;}
        let (tx,requests)=mpsc::sync_channel::<(offline_detail::TileId,u16)>(6);
        let (outputs,rx)=mpsc::sync_channel::<Decoded>(3);
        if std::thread::Builder::new().name("sleufbase-offline-tile-decode".into())
            .spawn(move||{
                while let Ok((id,mb))=requests.recv(){
                    let tile=offline_detail::read_tile(&offline_detail::root_for(mb),id,mb);
                    if outputs.send((id,mb,tile)).is_err(){break;}
                }
            }).is_ok(){
            self.decoder=Some(tx);
            self.decoded=Some(rx);
        }
    }
    fn start_detail_refresh(&mut self,ctx:&egui::Context,mb:u16){
        if self.detail_worker.is_some() || Instant::now()<self.detail_next_sweep{return;}
        let manual=self.manual_queued==Some(mb);
        if !self.auto_enabled && !manual{return;}
        let (tx,rx)=mpsc::sync_channel(1);
        let focus=Arc::clone(&self.detail_focus);
        let cancel=Arc::new(AtomicBool::new(false));
        let cancellation=Arc::clone(&cancel);
        let repaint=ctx.clone();
        let done=Arc::clone(&self.manual_progress);
        if std::thread::Builder::new()
            .name(if manual {"sleufbase-manual-offline-photo"} else {"sleufbase-sixmonth-offline-photo-tiles"}.into())
            .spawn(move||{
                let result=offline_detail::refresh_all(
                    &offline_detail::root_for(mb),&focus,mb,&cancellation,manual,
                    |_id|{
                        if manual{done.fetch_add(1,Ordering::Relaxed);}
                        repaint.request_repaint();
                    });
                let _=tx.send(result);
                repaint.request_repaint();
            }).is_ok(){
            if manual{self.manual_queued=None;}
            self.detail_worker=Some((mb,manual,rx));
            self.detail_cancel=Some(cancel);
        }else{
            self.manual_queued=None;
            if manual{self.manual_notice=Some("Handmatige offline download kon niet starten.".into());}
            self.detail_next_sweep=Instant::now()+RETRY_AFTER_FAILURE;
        }
    }
    fn poll_detail_downloads(&mut self,ctx:&egui::Context,mb:u16){
        let completed=self.detail_worker.as_ref().and_then(|(running,manual,receiver)|{
            match receiver.try_recv(){
                Ok(result)=>Some((*running,*manual,result)),
                Err(TryRecvError::Disconnected)=>
                    Some((*running,*manual,Err("Offline-kaartworker gestopt".into()))),
                Err(TryRecvError::Empty)=>None,
            }
        });
        if let Some((running,manual,result))=completed{
            let cancelled=self.detail_cancel.as_ref()
                .is_some_and(|flag|flag.load(Ordering::Relaxed));
            self.detail_worker=None;
            self.detail_cancel=None;
            if manual && !cancelled{
                let done=self.manual_progress.load(Ordering::Relaxed);
                self.manual_notice=Some(match &result {
                    Ok(())=>format!("Offline kaart bijgewerkt ({running} MB, {done}/144 tegels)."),
                    Err(error)=>format!("Offline kaart: {done}/144 tegels bijgewerkt; {error}. Bestaande beelden blijven staan."),
                });
            }
            self.detail_next_sweep=if running!=mb || cancelled || self.manual_queued.is_some() {
                Instant::now()
            }else{
                Instant::now()+if result.is_ok() {
                    Duration::from_secs(24*60*60)
                }else{RETRY_AFTER_FAILURE}
            };
        }
        self.start_detail_refresh(ctx,mb);
    }
    fn visible_ids(&self,ctx:&egui::Context,view:Bounds)->Vec<offline_detail::TileId>{
        let width=view.max_x-view.min_x;
        let height=view.max_y-view.min_y;
        if width<=0.||height<=0. {return Vec::new();}
        let screen=ctx.screen_rect();
        let points_per_tile=(offline_overview::BOUNDS.max_x-
            offline_overview::BOUNDS.min_x)/f64::from(offline_detail::GRID)
            *f64::from(screen.width())/width;
        if points_per_tile<MIN_TILE_SCREEN_POINTS {return Vec::new();}
        let mid=[(view.min_x+view.max_x)*0.5,(view.min_y+view.max_y)*0.5];
        let mut ids=offline_detail::ids_for_view(view);
        ids.sort_by(|a,b|{
            let distance=|id:&offline_detail::TileId|{
                let bounds=id.bounds();
                let x=(bounds.min_x+bounds.max_x)*0.5-mid[0];
                let y=(bounds.min_y+bounds.max_y)*0.5-mid[1];
                x*x+y*y
            };
            distance(a).total_cmp(&distance(b))
        });
        ids.truncate(MAX_GPU_TILES);
        ids
    }
    fn poll_visible_tiles(&mut self,ctx:&egui::Context,view:Option<Bounds>,mb:u16){
        let Some(view)=view else{self.tiles.clear();return;};
        self.start_decoder();
        if let Ok(mut focus)=self.detail_focus.lock(){
            *focus=[(view.min_x+view.max_x)*0.5,(view.min_y+view.max_y)*0.5];
        }
        let ids=self.visible_ids(ctx,view);
        // Consume at most two decoded images per frame, avoiding a large
        // burst of expensive GPU uploads when first opening a cached project.
        for _ in 0..DECODES_PER_FRAME {
            let next=self.decoded.as_ref().map(|rx|rx.try_recv());
            let (id,requested_mb,result)=match next {
                Some(Ok(tile))=>tile,
                Some(Err(TryRecvError::Disconnected))=>{
                    // Recreate a stopped decoder instead of scheduling a
                    // repaint every 60 ms forever with stuck pending IDs.
                    self.decoding.clear();
                    self.decoder=None;
                    self.decoded=None;
                    break;
                }
                _=>break,
            };
            self.decoding.remove(&(id,requested_mb));
            if requested_mb!=mb{continue;}
            if let Ok(Some(tile))=result {
                // Keep only the textures needed for the current camera. Old
                // cached JPEG files remain available across zoom/pan sessions.
                if ids.contains(&id){
                    let image=egui::ColorImage::from_rgba_unmultiplied(
                        [tile.image.width() as usize,tile.image.height() as usize],
                        tile.image.as_raw());
                    let texture=ctx.load_texture(format!("PDOK offline {mb} MB detail {}-{}",
                        id.col,id.row),image,egui::TextureOptions::LINEAR);
                    self.loaded.retain(|item|item.id!=id);
                    self.loaded.push_back(LoadedTile{id,
                        modified:tile.modified,texture});
                    ctx.request_repaint();
                }
            }
        }
        let dir=offline_detail::root_for(mb);
        for &id in &ids {
            let Some(stamp)=offline_detail::file_stamp(&dir,id,mb) else{continue;};
            if let Some(pos)=self.loaded.iter().position(|t|t.id==id
                && t.modified==stamp){
                // Move currently visible images to the MRU end.
                if let Some(tile)=self.loaded.remove(pos){self.loaded.push_back(tile);}
                continue;
            }
            if self.decoding.contains(&(id,mb)){continue;}
            if self.decoder.as_ref().is_some_and(|sender|
                sender.try_send((id,mb)).is_ok()){
                self.decoding.insert((id,mb));
            }
        }
        while self.loaded.len()>MAX_GPU_TILES {self.loaded.pop_front();}
        self.tiles.clear();
        for id in ids {
            if let Some(tile)=self.loaded.iter().find(|entry|entry.id==id){
                self.tiles.push((tile.texture.clone(),id.bounds()));
            }
        }
        if !self.decoding.is_empty(){
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }
    pub(super) fn poll(&mut self,ctx:&egui::Context,view:Option<Bounds>,
        auto_backup:bool,detail_mb:u16){
        let mb=if offline_detail::valid_level(detail_mb){detail_mb}else{50};
        if self.active_level!=mb {
            if let Some(cancel)=&self.detail_cancel {
                cancel.store(true,Ordering::Relaxed);
            }
            self.active_level=mb;
            self.loaded.clear();
            self.decoding.clear();
            self.tiles.clear();
            self.detail_next_sweep=Instant::now();
        }
        if self.auto_enabled!=auto_backup {
            self.auto_enabled=auto_backup;
            if !auto_backup {
                if let Some(cancel)=&self.detail_cancel {
                    cancel.store(true,Ordering::Relaxed);
                }
            }else{
                self.detail_next_sweep=Instant::now();
            }
        }
        if !self.initialized{
            self.initialized=true;
            if let Ok(image)=offline_overview::load_initial(&offline_overview::cache_path()){
                self.apply(ctx,image);
            }
        }
        if let Some(receiver)=&self.pending{
            match receiver.try_recv(){
                Ok(Ok(overview))=>{
                    self.pending=None;
                    self.apply(ctx,overview);
                    self.retry_after=Instant::now();
                    self.manual_preview_pending=false;
                }
                Ok(Err(error))=>{
                    self.pending=None;
                    if self.manual_preview_pending {
                        self.manual_notice=Some(format!("Overzichtsfoto vernieuwen mislukt: {error}; opgeslagen kaart blijft beschikbaar."));
                    }
                    self.manual_preview_pending=false;
                    self.retry_after=Instant::now()+RETRY_AFTER_FAILURE;
                }
                Err(TryRecvError::Disconnected)=>{
                    self.pending=None;
                    self.manual_preview_pending=false;
                    self.retry_after=Instant::now()+RETRY_AFTER_FAILURE;
                }
                Err(TryRecvError::Empty)=>{}
            }
        }
        if self.pending.is_none() && Instant::now()>=self.retry_after
            && (self.force_overview ||
                (self.auto_enabled &&
                 offline_overview::needs_refresh(self.last_success,SystemTime::now()))) {
            let (tx,rx)=mpsc::sync_channel(1);
            let path=offline_overview::cache_path();
            let repaint=ctx.clone();
            if std::thread::Builder::new()
                .name("sleufbase-offline-overview-refresh".into())
                .spawn(move||{
                    let result=offline_overview::download_and_store(&path);
                    let _=tx.send(result);
                    repaint.request_repaint();
                }).is_ok(){
                self.manual_preview_pending=self.force_overview;
                self.force_overview=false;
                self.pending=Some(rx);
            }else{
                self.force_overview=false;
                self.retry_after=Instant::now()+RETRY_AFTER_FAILURE;
            }
        }
        // The embedded photo is always readable. Large background downloads
        // happen only after an explicit opt-in in Settings (default: off).
        self.poll_detail_downloads(ctx,mb);
        self.poll_visible_tiles(ctx,view,mb);
        if self.pending.is_some(){
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn manual_download_is_a_one_time_action_without_scheduling_opt_in(){
        let mut cache=State::default();
        assert!(!cache.auto_enabled);
        cache.download_now(25);
        assert!(cache.manual_active());
        assert_eq!(cache.manual_queued,Some(25));
        assert!(cache.force_overview);
        assert!(!cache.auto_enabled);
        cache.cancel_manual();
        assert!(!cache.manual_active());
        assert!(!cache.auto_enabled);
    }
    #[test]
    fn offline_texture_exists_before_any_sharp_wmts_tile(){
        let ctx=egui::Context::default();
        let mut cache=State::default();
        assert!(!cache.auto_enabled, "offline downloads must be opt-in");
        // Tests do not call the actual background update worker.
        let image=offline_overview::load_initial(&offline_overview::cache_path()).unwrap();
        cache.apply(&ctx,image);
        assert!(cache.texture().is_some());
        assert_eq!(cache.texture().unwrap().size(),
            [offline_overview::SIZE as usize,offline_overview::SIZE as usize]);
    }
}
