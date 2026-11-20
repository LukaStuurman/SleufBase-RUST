//! Always-available bundled PDOK photo and a stronger ~50 MiB offline raster.
//! The high-resolution version is split into tiles, decoded off the egui
//! thread, and shown underneath the sharp online WMTS tiles. Both background
//! download and half-year refresh remain optional for offline operation.
use super::*;
use sleufbase_rust::{offline_overview,offline_detail};
use std::{
    collections::{HashSet,VecDeque},
    sync::{atomic::{AtomicBool,Ordering},Arc,Mutex},
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
    detail_worker:Option<(u16,Receiver<Result<(),String>>)>,
    detail_cancel:Option<Arc<AtomicBool>>,
    detail_next_sweep:Instant,
    detail_focus:Arc<Mutex<[f64;2]>>,
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
        if !self.auto_enabled || self.detail_worker.is_some()
            || Instant::now()<self.detail_next_sweep{return;}
        let (tx,rx)=mpsc::sync_channel(1);
        let focus=Arc::clone(&self.detail_focus);
        let cancel=Arc::new(AtomicBool::new(false));
        let cancellation=Arc::clone(&cancel);
        let repaint=ctx.clone();
        if std::thread::Builder::new()
            .name("sleufbase-sixmonth-offline-photo-tiles".into())
            .spawn(move||{
                let result=offline_detail::refresh_all(
                    &offline_detail::root_for(mb),&focus,mb,&cancellation,
                    |_id|repaint.request_repaint());
                let _=tx.send(result);
                repaint.request_repaint();
            }).is_ok(){
            self.detail_worker=Some((mb,rx));
            self.detail_cancel=Some(cancel);
        }else{
            self.detail_next_sweep=Instant::now()+RETRY_AFTER_FAILURE;
        }
    }
    fn poll_detail_downloads(&mut self,ctx:&egui::Context,mb:u16){
        let completed=self.detail_worker.as_ref().and_then(|(running,receiver)|{
            match receiver.try_recv(){
                Ok(result)=>Some((*running,result)),
                Err(TryRecvError::Disconnected)=>
                    Some((*running,Err("Offline-kaartworker gestopt".into()))),
                Err(TryRecvError::Empty)=>None,
            }
        });
        if let Some((running,result))=completed{
            let cancelled=self.detail_cancel.as_ref()
                .is_some_and(|flag|flag.load(Ordering::Relaxed));
            self.detail_worker=None;
            self.detail_cancel=None;
            self.detail_next_sweep=if running!=mb||cancelled {
                // An old mode or an opt-out cancelled this fetch.
                // Do not delay a newly enabled preset for a whole day.
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
                }
                Ok(Err(_))|Err(TryRecvError::Disconnected)=>{
                    // Failed WMS downloads never remove the bundled or cached
                    // map. Retry tomorrow rather than spamming the service.
                    self.pending=None;
                    self.retry_after=Instant::now()+RETRY_AFTER_FAILURE;
                }
                Err(TryRecvError::Empty)=>{}
            }
        }
        if self.auto_enabled && self.pending.is_none() && Instant::now()>=self.retry_after
            && offline_overview::needs_refresh(self.last_success,SystemTime::now()) {
            let (tx,rx)=mpsc::sync_channel(1);
            let path=offline_overview::cache_path();
            let repaint=ctx.clone();
            if std::thread::Builder::new()
                .name("sleufbase-halfyearly-pdok-overview".into())
                .spawn(move||{
                    let result=offline_overview::download_and_store(&path);
                    let _=tx.send(result);
                    repaint.request_repaint();
                }).is_ok(){
                self.pending=Some(rx);
            }else{
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
