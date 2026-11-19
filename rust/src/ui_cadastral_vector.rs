//! Temporary vector parcel overlay for close-range interactive map views.
//! The manual Kadaster WFS project layer remains independent and persistent.
use super::*;
use sleufbase_rust::project_view::BackgroundProvider as B;
use std::{sync::{atomic::{AtomicBool, Ordering}, Arc, mpsc::{self,Receiver,TryRecvError}},time::{Instant,Duration}};

const MAX_SHAPE_SPAN_METERS: f64 = 950.;
const MAX_QUERY_SPAN_METERS: f64 = 1600.;
const REFRESH_DELAY: Duration = Duration::from_millis(140);
const RETRY_DELAY: Duration = Duration::from_secs(12);

fn eligible(provider: B, bounds: Bounds) -> bool {
    matches!(provider, B::PdokAerialCadastral | B::OpenStreetMapCadastral | B::Cadastral)
        && [bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y].iter().all(|n|n.is_finite())
        && bounds.max_x > bounds.min_x && bounds.max_y > bounds.min_y
        && bounds.max_x-bounds.min_x <= MAX_SHAPE_SPAN_METERS
        && bounds.max_y-bounds.min_y <= MAX_SHAPE_SPAN_METERS
}
fn covers(have:Bounds,want:Bounds)->bool {
    have.min_x<=want.min_x && have.min_y<=want.min_y
        && have.max_x>=want.max_x && have.max_y>=want.max_y
}
fn buffered(view:Bounds)->Bounds {
    let dx=(view.max_x-view.min_x).max(20.)*0.22;
    let dy=(view.max_y-view.min_y).max(20.)*0.22;
    let fx=dx.min((MAX_QUERY_SPAN_METERS-(view.max_x-view.min_x)).max(0.)/2.);
    let fy=dy.min((MAX_QUERY_SPAN_METERS-(view.max_y-view.min_y)).max(0.)/2.);
    Bounds{min_x:view.min_x-fx,max_x:view.max_x+fx,min_y:view.min_y-fy,max_y:view.max_y+fy}
}
struct Fetch {
    bounds:Bounds,
    cancel:Arc<AtomicBool>,
    rx:Receiver<Result<CadastralLayer,String>>,
}
impl Drop for Fetch {
    fn drop(&mut self) { self.cancel.store(true, Ordering::Release); }
}
#[derive(Default)]
pub(super) struct State {
    layer:Option<CadastralLayer>,
    task:Option<Fetch>,
    requested:Option<Bounds>,
    provider:Option<B>,
    next:Option<Instant>,
    error:Option<String>,
}
impl State {
    pub(super) fn reset(&mut self) {
        self.task=None;
        self.layer=None;
        self.requested=None;
        self.provider=None;
        self.next=None;
        self.error=None;
    }
    pub(super) fn layer(&self)->Option<&CadastralLayer> {
        self.layer.as_ref().filter(|layer|layer.visible)
    }
    pub(super) fn layer_for(&self,provider:B)->Option<&CadastralLayer> {
        (self.provider==Some(provider)).then(||self.layer()).flatten()
    }
    pub(super) fn ready_for(&self, view:Bounds)->bool {
        self.layer().is_some_and(|layer|covers(layer.bounds,view))
    }
    pub(super) fn fallback(&self)->bool { self.error.is_some() }
    pub(super) fn wants(provider:B,view:Bounds)->bool { eligible(provider,view) }
    pub(super) fn update(&mut self,provider:B,view:Bounds,ctx:&egui::Context) {
        if !eligible(provider,view) {
            if self.provider.is_some() {self.reset();}
            return;
        }
        if self.provider!=Some(provider) {
            self.reset();
            self.provider=Some(provider);
            self.next=Some(Instant::now()+REFRESH_DELAY);
        }
        if self.ready_for(view) {return;}
        if self.task.as_ref().is_some_and(|task|covers(task.bounds,view)) {
            ctx.request_repaint_after(Duration::from_millis(90));return;
        }
        if self.task.is_some() {
            self.task=None;
            self.next=Some(Instant::now()+REFRESH_DELAY);
        }
        let now=Instant::now();
        if self.next.is_some_and(|next|now<next) {
            ctx.request_repaint_after(self.next.unwrap().saturating_duration_since(now));
            return;
        }
        let bounds=buffered(view);
        let (tx,rx)=mpsc::sync_channel(1);
        let cancel=Arc::new(AtomicBool::new(false));
        let worker_cancel=Arc::clone(&cancel);
        let repaint=ctx.clone();
        match std::thread::Builder::new().name("sleufbase-vector-percelen".into()).spawn(move|| {
            let result=cadastral::fetch_view_cadastral(bounds,&worker_cancel);
            let _=tx.send(result);repaint.request_repaint();
        }) {
            Ok(_) => {
                self.task=Some(Fetch{bounds,cancel,rx});
                self.requested=Some(bounds);
                self.next=None;
                self.error=None;
                ctx.request_repaint_after(Duration::from_millis(90));
            }
            Err(error) => {
                self.error=Some(format!("Perceelvormen ophalen mislukt: {error}"));
                self.next=Some(now+RETRY_DELAY);
            }
        }
    }
    pub(super) fn poll(&mut self,ctx:&egui::Context) {
        let completed=self.task.as_ref().map(|task|match task.rx.try_recv(){
            Ok(result)=>Some(result),
            Err(TryRecvError::Disconnected)=>Some(Err("Perceelservice onderbroken".into())),
            Err(TryRecvError::Empty)=>None,
        }).flatten();
        let Some(result)=completed else {return;};
        self.task=None;
        match result {
            Ok(layer) => {self.layer=Some(layer);self.error=None;self.next=None;},
            Err(error) => {
                self.error=Some(error);
                self.next=Some(Instant::now()+RETRY_DELAY);
            },
        }
        ctx.request_repaint();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn b(min:f64,max:f64)->Bounds {
        Bounds{min_x:min,min_y:420000.,max_x:max,max_y:420000.+max-min}
    }
    #[test]
    fn near_shape_view_buffer_covers_zoom_and_far_view_uses_raster(){
        let near=b(150000.,150250.);
        assert!(eligible(B::Cadastral,near));
        assert!(covers(buffered(near),near));
        assert!(covers(buffered(near),b(150010.,150240.)));
        assert!(!eligible(B::Cadastral,b(150000.,152000.)));
        assert!(!eligible(B::PdokAerial,near));
    }
}
