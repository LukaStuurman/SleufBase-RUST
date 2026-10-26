//! One bounded background decoder per desktop session. Project changes cancel
//! pending work; generation checks prevent late pixels entering a new project.
use crate::raster::{decode_preview_with_cancel, PreviewOptions, RasterPreview};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
};

pub const RASTER_TEXTURE_BUDGET: usize = 256 * 1024 * 1024;
pub fn preview_options_for_layers(count: usize) -> PreviewOptions {
    let pixels = (RASTER_TEXTURE_BUDGET / count.max(1) / 4).max(1) as u64;
    PreviewOptions {
        max_pixels: PreviewOptions::default().max_pixels.min(pixels),
        ..Default::default()
    }
}
struct Request {
    generation: u64,
    id: usize,
    path: PathBuf,
    options: PreviewOptions,
    cancel: Arc<AtomicBool>,
}
pub struct RasterReady {
    pub id: usize,
    pub path: PathBuf,
    pub result: Result<RasterPreview, String>,
}
struct Finished {
    generation: u64,
    ready: RasterReady,
}
pub struct RasterLoader {
    requests: SyncSender<Request>,
    results: Receiver<Finished>,
    generation: u64,
    cancel: Arc<AtomicBool>,
    options: PreviewOptions,
    pending: VecDeque<(usize, PathBuf)>,
    in_flight: Option<(usize, PathBuf)>,
}
impl RasterLoader {
    pub fn new() -> Result<Self, String> {
        let (requests, rx) = mpsc::sync_channel::<Request>(1);
        let (tx, results) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("sleufbase-raster".into())
            .spawn(move || {
                while let Ok(request) = rx.recv() {
                    let result = std::panic::catch_unwind(|| {
                        decode_preview_with_cancel(&request.path, request.options, &request.cancel)
                    })
                    .unwrap_or_else(|_| {
                        Err("Rasterdecoder is onverwacht gestopt bij dit bestand".into())
                    });
                    if tx
                        .send(Finished {
                            generation: request.generation,
                            ready: RasterReady {
                                id: request.id,
                                path: request.path,
                                result,
                            },
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Rasterachtergrondtaak kon niet starten: {e}"))?;
        Ok(Self {
            requests,
            results,
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            options: PreviewOptions::default(),
            pending: VecDeque::new(),
            in_flight: None,
        })
    }
    pub fn replace(&mut self, paths: Vec<(usize, PathBuf)>, options: PreviewOptions) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation = self.generation.wrapping_add(1);
        self.options = options;
        self.pending = paths.into();
        self.in_flight = None;
        self.pump();
    }
    pub fn remaining(&self) -> usize {
        self.pending.len() + usize::from(self.in_flight.is_some())
    }
    fn pump(&mut self) {
        if self.in_flight.is_some() {
            return;
        }
        let Some((id, path)) = self.pending.front() else {
            return;
        };
        let request = Request {
            generation: self.generation,
            id: *id,
            path: path.clone(),
            options: self.options,
            cancel: self.cancel.clone(),
        };
        match self.requests.try_send(request) {
            Ok(()) => {
                self.in_flight = self.pending.pop_front();
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }
    pub fn poll(&mut self) -> Vec<RasterReady> {
        let mut ready = Vec::new();
        loop {
            match self.results.try_recv() {
                Ok(result) if result.generation == self.generation => {
                    self.in_flight = None;
                    ready.push(result.ready);
                }
                Ok(_) => {} // Already cancelled; dropping the bounded result releases its pixels.
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if let Some((id, path)) = self.in_flight.take() {
                        ready.push(RasterReady {
                            id,
                            path,
                            result: Err("Rasterachtergrondtaak is gestopt".into()),
                        });
                    }
                    ready.extend(self.pending.drain(..).map(|(id, path)| RasterReady {
                        id,
                        path,
                        result: Err("Rasterachtergrondtaak is gestopt".into()),
                    }));
                    break;
                }
            }
        }
        self.pump();
        ready
    }
}
impl Drop for RasterLoader {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Camera corners mapped into the existing oriented full-image texture. This
/// supports rotated/sheared affine TIFFs: callers map all four camera corners,
/// never only its world-axis diagonal. The enclosing crop remains axis aligned
/// in texture space and is drawn through the original affine texture quad.
#[derive(Clone,Debug)]
pub struct ViewportRequest {pub id:usize,pub path:PathBuf,pub corners:[[f64;2];4],pub screen_pixels:[u32;2]}
#[derive(Clone,Debug,PartialEq)]
struct WindowRequest {id:usize,path:PathBuf,uv:[f64;4],visible:[f64;4],max_side:u32,max_pixels:u64}
/// Budget for retained camera-detail RGBA, matching renderer.py's hardware
/// policy floor. Base thumbnails have a separate, existing fallback budget.
pub fn detail_pixel_budget(policy: &crate::resource_policy::ResourcePolicy) -> u64 {
    policy.virtual_tiff_pixel_budget.max(4_000_000)
}
impl ViewportRequest {
    pub fn visible(&self) -> bool {
        WindowRequest::plan_with_budget(self, 1, 4_000_000).is_some()
    }
}
impl WindowRequest {
    #[cfg(test)]
    fn plan(request: &ViewportRequest, count: usize) -> Option<Self> {
        Self::plan_with_budget(request, count, 32_000_000)
    }
    fn plan_with_budget(request:&ViewportRequest,count:usize,budget:u64)->Option<Self>{
        if request.corners.iter().flatten().any(|v|!v.is_finite())||request.screen_pixels.contains(&0){return None;}
        let mut uv=[f64::INFINITY,f64::INFINITY,f64::NEG_INFINITY,f64::NEG_INFINITY];
        for p in request.corners{uv[0]=uv[0].min(p[0]);uv[1]=uv[1].min(p[1]);uv[2]=uv[2].max(p[0]);uv[3]=uv[3].max(p[1]);}
        if uv[0]>=1.||uv[1]>=1.||uv[2]<=0.||uv[3]<=0.||uv[0]>=uv[2]||uv[1]>=uv[3]{return None;}
        let visible=uv.map(|v|v.clamp(0.,1.));
        // One-quarter screen gutter avoids a decode on every small pan.
        let dx=(uv[2]-uv[0])*0.25;let dy=(uv[3]-uv[1])*0.25;
        uv=[(uv[0]-dx).max(0.),(uv[1]-dy).max(0.),(uv[2]+dx).min(1.),(uv[3]+dy).min(1.)];
        let side=request.screen_pixels.into_iter().max().unwrap().saturating_mul(3).div_ceil(2).checked_next_power_of_two().unwrap_or(8192).clamp(256,8192);
        Some(Self{id:request.id,path:request.path.clone(),uv,visible,max_side:side,max_pixels:(budget / count.max(1) as u64).clamp(1,crate::raster::MAX_PREVIEW_PIXELS)})
    }
    fn covers(&self,next:&Self)->bool{
        self.id==next.id&&self.path==next.path&&self.max_pixels==next.max_pixels
        &&self.uv[0]<=next.visible[0]&&self.uv[1]<=next.visible[1]&&self.uv[2]>=next.visible[2]&&self.uv[3]>=next.visible[3]
        &&(0..2).all(|axis|self.max_side as f64/(self.uv[axis+2]-self.uv[axis])>=0.8*next.max_side as f64/(next.uv[axis+2]-next.uv[axis]))
    }
}
pub struct WindowReady {pub id:usize,pub path:PathBuf,pub result:Result<crate::raster::RasterWindow,String>}
/// One bounded decoder, one queued result, generation rejection and cooperative
/// cancellation. update() is cheap enough for each camera frame; stable camera
/// buckets retain their pixels instead of restarting an in-flight request.
pub struct ViewportLoader {
    requests:SyncSender<(u64,WindowRequest,Arc<AtomicBool>)>,results:Receiver<(u64,WindowReady)>,
    generation:u64,cancel:Arc<AtomicBool>,pixel_budget:u64,plans:Vec<WindowRequest>,pending:VecDeque<WindowRequest>,in_flight:bool,
}
impl ViewportLoader {
    pub fn new()->Result<Self,String>{ Self::with_policy(crate::resource_policy::get()) }
    pub fn with_policy(policy: &crate::resource_policy::ResourcePolicy)->Result<Self,String>{
        let(requests,rx)=mpsc::sync_channel::<(u64,WindowRequest,Arc<AtomicBool>)>(1);let(tx,results)=mpsc::sync_channel(1);
        std::thread::Builder::new().name("sleufbase-raster-camera".into()).spawn(move||while let Ok((generation,r,cancel))=rx.recv(){
            let result=std::panic::catch_unwind(||crate::raster::decode_window_with_cancel(&r.path,PreviewOptions{max_side:r.max_side,max_pixels:r.max_pixels,..Default::default()},crate::raster::PreviewWindow{uv:r.uv},&cancel)).unwrap_or_else(|_|Err("Rasterdecoder is onverwacht gestopt".into()));
            if tx.send((generation,WindowReady{id:r.id,path:r.path,result})).is_err(){break;}
        }).map_err(|e|e.to_string())?;
        Ok(Self{requests,results,generation:0,cancel:Arc::new(AtomicBool::new(false)),pixel_budget:detail_pixel_budget(policy),plans:Vec::new(),pending:VecDeque::new(),in_flight:false})
    }
    /// Returns true only when the camera needs a new generation of pixels.
    pub fn update(&mut self,requests:&[ViewportRequest])->bool{
        let count = requests.iter().filter(|r| r.visible()).count();
        let next=requests.iter().filter_map(|r|WindowRequest::plan_with_budget(r,count,self.pixel_budget)).collect::<Vec<_>>();
        if self.plans.len()==next.len()&&self.plans.iter().zip(&next).all(|(a,b)|a.covers(b)){return false;}
        self.cancel.store(true,Ordering::Relaxed);self.cancel=Arc::new(AtomicBool::new(false));self.generation=self.generation.wrapping_add(1);self.plans=next.clone();self.pending=next.into();self.in_flight=false;self.pump();true
    }
    fn pump(&mut self){if !self.in_flight{if let Some(r)=self.pending.front(){if self.requests.try_send((self.generation,r.clone(),self.cancel.clone())).is_ok(){self.pending.pop_front();self.in_flight=true;}}}}
    pub fn poll(&mut self)->Vec<WindowReady>{let mut ready=Vec::new();while let Ok((generation,result))=self.results.try_recv(){if generation==self.generation{self.in_flight=false;ready.push(result);}}self.pump();ready}
    pub fn remaining(&self)->usize{self.pending.len()+usize::from(self.in_flight)}
    pub fn pixel_budget(&self)->u64 { self.pixel_budget }
    pub fn layer_pixel_budget(&self)->u64 {
        (self.pixel_budget / self.plans.len().max(1) as u64).clamp(1, crate::raster::MAX_PREVIEW_PIXELS)
    }
}
impl Drop for ViewportLoader{fn drop(&mut self){self.cancel.store(true,Ordering::Relaxed);}}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::Path,
        time::{Duration, Instant},
    };
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/rasters")
            .join(name)
    }
    fn drain(loader: &mut RasterLoader) -> Vec<RasterReady> {
        let until = Instant::now() + Duration::from_secs(10);
        let mut results = Vec::new();
        while loader.remaining() > 0 {
            results.extend(loader.poll());
            assert!(Instant::now() < until, "raster loader did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
        results
    }
    #[test]
    fn replacement_never_returns_an_old_projects_pixels() {
        let mut loader = RasterLoader::new().unwrap();
        loader.replace(
            vec![
                (0, fixture("large-striped.tiff")),
                (1, fixture("rgb8-strips.tiff")),
            ],
            PreviewOptions::default(),
        );
        loader.replace(
            vec![(9, fixture("orientation6.tiff"))],
            PreviewOptions::default(),
        );
        let results = drain(&mut loader);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 9);
        assert_eq!(
            (
                results[0].result.as_ref().unwrap().width,
                results[0].result.as_ref().unwrap().height
            ),
            (2, 3)
        );
    }
    #[test]
    fn a_bad_file_does_not_block_later_previews() {
        let mut loader = RasterLoader::new().unwrap();
        loader.replace(
            vec![
                (0, fixture("does-not-exist.tiff")),
                (1, fixture("rgb8-strips.tiff")),
            ],
            PreviewOptions::default(),
        );
        let results = drain(&mut loader);
        assert_eq!(results.len(), 2);
        assert!(results[0].result.is_err());
        assert!(results[1].result.is_ok());
    }
    #[test]
    fn aggregate_texture_budget_scales_to_many_rasters() {
        for count in [1, 16, 17, 100, 1000, 100_000] {
            let options = preview_options_for_layers(count);
            assert!(options.max_pixels * 4 * count as u64 <= RASTER_TEXTURE_BUDGET as u64);
            assert!(options.max_pixels > 0);
        }
    }
}

#[cfg(test)] mod viewport_tests{
    use super::*;
    fn request(x:f64,scale:f64)->ViewportRequest{ViewportRequest{id:1,path:PathBuf::from("fixture.tiff"),corners:[[x,0.2],[x+scale,0.18],[x+scale+0.02,0.2+scale],[x,0.2+scale]],screen_pixels:[800,600]}}
    #[test]
    fn hardware_detail_budget_matches_original_renderer_policy_oracles() {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/resource-policy/reference.json"
        )).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 175);
        for row in rows.as_array().unwrap() {
            let policy = crate::resource_policy::build(
                serde_json::from_value(row["snapshot"].clone()).unwrap(),
                &serde_json::from_value(row["env"].clone()).unwrap(),
            );
            // renderer.py: max(4_000_000, get_resource_policy().virtual_tiff_pixel_budget).
            let expected = row["expected"]["virtual_tiff_pixel_budget"].as_u64().unwrap().max(4_000_000);
            assert_eq!(detail_pixel_budget(&policy), expected, "{row}");
            for count in [1, 2, 16, 1000] {
                let plan = WindowRequest::plan_with_budget(&request(0.2, 0.3), count, expected).unwrap();
                assert!(plan.max_pixels * count as u64 <= expected);
                assert!(plan.max_pixels <= crate::raster::MAX_PREVIEW_PIXELS);
            }
        }
    }
    #[test]
    fn offscreen_edge_invalidates_completed_queued_result() {
        let mut loader = ViewportLoader::new().unwrap();
        let mut r = request(0.2, 0.3);
        r.path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/orientation6.tiff");
        assert!(loader.update(&[r.clone()]));
        // Capture a genuinely completed decode before simulating its late
        // delivery through the bounded result channel after the camera moves.
        let queued = loader.results.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert!(queued.1.result.is_ok());
        let (tx, results) = mpsc::sync_channel(1);
        loader.results = results;
        assert!(tx.send(queued).is_ok());
        let mut edge = r.clone();
        edge.corners = [[1.,0.], [2.,0.], [2.,1.], [1.,1.]];
        assert!(!edge.visible());
        assert!(loader.update(&[edge]));
        assert_eq!(loader.remaining(), 0);
        assert!(loader.poll().is_empty());
    }
    #[test]fn affine_camera_hysteresis_reuses_gutter_and_refines_zoom(){
        let a=WindowRequest::plan(&request(0.2,0.3),1).unwrap();let small_pan=WindowRequest::plan(&request(0.21,0.3),1).unwrap();assert!(a.covers(&small_pan));
        let far=WindowRequest::plan(&request(0.6,0.3),1).unwrap();assert!(!a.covers(&far));let zoom=WindowRequest::plan(&request(0.2,0.03),1).unwrap();assert!(!a.covers(&zoom));assert!(a.uv[2]>0.52);
    }
    #[test]fn replacement_rejects_stale_window_results(){
        let mut loader=ViewportLoader::new().unwrap();let mut r=request(0.,1.);r.path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/orientation6.tiff");
        assert!(loader.update(&[r.clone()]));assert!(!loader.update(&[r.clone()]));r.id=9;assert!(loader.update(&[r]));
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);let mut results=Vec::new();while loader.remaining()>0{results.extend(loader.poll());assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(2));}
        assert_eq!(results.len(),1);assert_eq!(results[0].id,9);assert!(results[0].result.is_ok());
    }
}
