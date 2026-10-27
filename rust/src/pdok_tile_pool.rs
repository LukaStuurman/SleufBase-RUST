//! Four in-flight tile requests, with assembly and progress on the caller thread.
use super::*;
use std::sync::{atomic::{AtomicBool, Ordering}, mpsc, Mutex};

pub(super) fn parallel(
    bounds: Bounds, width:u32, height:u32, cancel:&AtomicBool,
    fetch:impl Fn(Bounds,u32,u32)->Result<PdokRaster,String>+Sync,
    progress:Option<&mut dyn FnMut(Progress<'_>)->Result<(),String>>,
)->Result<image::RgbaImage,String>{
    if progress.is_none() || (width<=512 && height<=512) {
        return tiled_progress(bounds,width,height,cancel,fetch,progress);
    }
    crate::input::check(cancel)?;
    validate_frame(bounds,width,height)?;
    let mut tasks=Vec::new();
    for top in (0..height).step_by(512) {
        for left in (0..width).step_by(512) {
            let w=(width-left).min(512);let h=(height-top).min(512);
            let x=|n|bounds.min_x+f64::from(n)/f64::from(width)*(bounds.max_x-bounds.min_x);
            let y=|n|bounds.max_y-f64::from(n)/f64::from(height)*(bounds.max_y-bounds.min_y);
            tasks.push((Bounds{min_x:x(left),min_y:y(top+h),max_x:x(left+w),max_y:y(top)},w,h));
        }
    }
    let stop=AtomicBool::new(false);
    let (jobs_tx,jobs_rx)=mpsc::channel::<usize>();
    let jobs_rx=Mutex::new(jobs_rx);
    let (results_tx,results_rx)=mpsc::channel();
    std::thread::scope(|scope|{
        let workers=tasks.len().min(4);
        for _ in 0..workers {
            let tasks=&tasks;let fetch=&fetch;let jobs_rx=&jobs_rx;
            let results_tx=results_tx.clone();let stop=&stop;
            scope.spawn(move||{
                loop {
                    let job={jobs_rx.lock().unwrap().recv()};
                    let Ok(index)=job else {break};
                    if stop.load(Ordering::Relaxed)||cancel.load(Ordering::Relaxed){break;}
                    let (b,w,h)=tasks[index];
                    if results_tx.send((index,fetch(b,w,h))).is_err(){break;}
                }
            });
        }
        drop(results_tx);
        for index in 0..workers {jobs_tx.send(index).map_err(|_|"Kaartwerkers gestopt")?;}
        let mut next=0;
        let mut pending=std::collections::BTreeMap::new();
        let result=tiled_progress(bounds,width,height,cancel,|_,_,_|{
            // Replenish only when the caller requests another tile: cancellation
            // in the progress callback cannot enqueue a new request.
            if next>0 && next+workers-1<tasks.len(){
                jobs_tx.send(next+workers-1).map_err(|_|"Kaartwerkers gestopt")?;
            }
            while !pending.contains_key(&next){
                let (index,result)=results_rx.recv().map_err(|_|"Kaartdownload onderbroken")?;
                pending.insert(index,result);
            }
            let result=pending.remove(&next).unwrap();next+=1;result
        },progress);
        stop.store(true,Ordering::Relaxed);
        drop(jobs_tx);
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Condvar,atomic::AtomicUsize};
    #[test]
    fn four_requests_overlap_and_out_of_order_pixels_assemble_correctly(){
        let gate=(Mutex::new(0),Condvar::new());let calls=AtomicUsize::new(0);
        let cancel=AtomicBool::new(false);let mut updates=Vec::new();
        let mut progress=|p:Progress<'_>|{updates.push((p.completed,p.total));Ok(())};
        let image=parallel(Bounds{min_x:0.,min_y:0.,max_x:1024.,max_y:1024.},1024,1024,&cancel,|b,w,h|{
            calls.fetch_add(1,Ordering::SeqCst);
            let mut arrived=gate.0.lock().unwrap();*arrived+=1;gate.1.notify_all();
            let (arrived,_)=gate.1.wait_timeout_while(arrived,std::time::Duration::from_secs(5),|n|*n<4).unwrap();
            assert_eq!(*arrived,4,"tile requests did not overlap");drop(arrived);
            Ok(PdokRaster{bounds:b,width:w,height:h,rgba:[(b.min_x/512.) as u8,(b.min_y/512.) as u8,50,255].repeat((w*h) as usize)})
        },Some(&mut progress)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst),4);
        assert_eq!(image.get_pixel(0,0).0,[0,1,50,255]);
        assert_eq!(image.get_pixel(700,700).0,[1,0,50,255]);
        assert_eq!(updates.last(),Some(&(4,4)));
    }
    #[test]
    fn cancelling_progress_does_not_schedule_the_remaining_tiles(){
        let cancel=AtomicBool::new(false);let calls=AtomicUsize::new(0);
        let mut progress=|_:Progress<'_>|{cancel.store(true,Ordering::SeqCst);Ok(())};
        let result=parallel(Bounds{min_x:0.,min_y:0.,max_x:3072.,max_y:512.},3072,512,&cancel,|b,w,h|{
            calls.fetch_add(1,Ordering::SeqCst);
            Ok(PdokRaster{bounds:b,width:w,height:h,rgba:[1,2,3,255].repeat((w*h) as usize)})
        },Some(&mut progress));
        assert!(result.is_err());assert!(calls.load(Ordering::SeqCst)<=4);
    }
}
