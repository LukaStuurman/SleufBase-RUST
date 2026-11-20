//! A small always-present aerial texture below the sharper interactive WMTS.
//! The network refresher never blocks egui and never removes the old image.
use super::*;
use sleufbase_rust::offline_overview;
use std::time::{Instant, SystemTime};

const RETRY_AFTER_FAILURE:Duration=Duration::from_secs(24*60*60);

pub(super) struct State {
    initialized:bool,
    texture:Option<egui::TextureHandle>,
    last_success:SystemTime,
    pending:Option<Receiver<Result<offline_overview::Overview,String>>>,
    retry_after:Instant,
}
impl Default for State{
    fn default()->Self{
        Self{initialized:false,texture:None,
            last_success:offline_overview::seed_timestamp(),
            pending:None,retry_after:Instant::now()}
    }
}
impl State{
    pub(super) fn texture(&self)->Option<&egui::TextureHandle>{
        self.texture.as_ref()
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
    pub(super) fn poll(&mut self,ctx:&egui::Context){
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
                    // Failed WMS downloads never remove the bundled or cached map.
                    // A temporarily unavailable PDOK service is retried after
                    // one day while the app is running or at the next start.
                    self.pending=None;
                    self.retry_after=Instant::now()+RETRY_AFTER_FAILURE;
                }
                Err(TryRecvError::Empty)=>{}
            }
        }
        if self.pending.is_none() && Instant::now()>=self.retry_after
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
        cache.poll(&ctx);
        assert!(cache.texture().is_some());
        assert_eq!(cache.texture().unwrap().size(),
            [offline_overview::SIZE as usize,offline_overview::SIZE as usize]);
    }
}
