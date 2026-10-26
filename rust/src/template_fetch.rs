//! Bounded ordered service batches from the active template fetch scheduler.
//! Dispatch advances only after ordered acceptance, so a slow first request
//! cannot accumulate a whole export's later results in memory.
use crate::export_control::Control;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    time::Duration,
};
pub fn ordered<T: Send>(
    count: usize,
    workers: usize,
    control: &Control,
    work: impl Fn(usize, usize, &AtomicBool) -> Result<T, String> + Sync,
    mut accept: impl FnMut(T) -> Result<(), String>,
    abort: impl Fn() + Sync,
) -> Result<(), String> {
    control.check()?;
    if count == 0 {
        return Ok(());
    }
    let workers = workers.max(1).min(count);
    let window = workers.saturating_mul(2).min(count);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let (jobs_tx, jobs_rx) = mpsc::sync_channel::<usize>(window);
        let jobs = std::sync::Arc::new(Mutex::new(jobs_rx));
        let (results_tx, results_rx) = mpsc::sync_channel::<(usize, Result<T, String>)>(window);
        for worker in 0..workers {
            let jobs = jobs.clone();
            let tx = results_tx.clone();
            let stop = &stop;
            let work = &work;
            scope.spawn(move || loop {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let job = match jobs.lock() {
                    Ok(rx) => rx.recv(),
                    Err(_) => break,
                };
                let Ok(index) = job else {
                    break;
                };
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    work(worker, index, stop)
                }))
                .unwrap_or_else(|_| Err("Sjabloonaanvraag is onverwacht gestopt".into()));
                if tx.send((index, result)).is_err() {
                    break;
                }
            });
        }
        drop(results_tx);
        let outcome = (|| {
            let mut dispatched = 0usize;
            let mut accepted = 0usize;
            let mut ready = BTreeMap::new();
            while dispatched < window {
                jobs_tx
                    .send(dispatched)
                    .map_err(|_| "Sjabloonfetch-worker gesloten")?;
                dispatched += 1;
            }
            while accepted < count {
                control.check()?;
                match results_rx.recv_timeout(Duration::from_millis(20)) {
                    Ok((index, result)) => {
                        ready.insert(index, result?);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("Sjabloonfetch-worker gesloten vóór afronding".into())
                    }
                }
                while let Some(result) = ready.remove(&accepted) {
                    control.check()?;
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| accept(result)))
                        .unwrap_or_else(|_| {
                            Err("Sjabloonverwerking is onverwacht gestopt".into())
                        })?;
                    accepted += 1;
                    if dispatched < count {
                        jobs_tx
                            .send(dispatched)
                            .map_err(|_| "Sjabloonfetch-worker gesloten")?;
                        dispatched += 1;
                    }
                }
            }
            Ok(())
        })();
        stop.store(true, Ordering::Release);
        if outcome.is_err() {
            abort();
        }
        drop(jobs_tx);
        // Every worker owns at most one request and either sends an outcome or
        // observes stop. The channel can hold the entire bounded outstanding
        // window, including when callback validation rejects a result.
        outcome
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    fn wait_until(mut condition: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !condition() {
            assert!(
                std::time::Instant::now() < deadline,
                "worker handshake timed out"
            );
            std::thread::yield_now();
        }
    }
    #[test]
    fn slow_first_limits_dispatch_and_preserves_order() {
        let control = Control::default();
        let started = AtomicUsize::new(0);
        let mut accepted = Vec::new();
        ordered(
            100,
            3,
            &control,
            |_, index, _| {
                started.fetch_add(1, Ordering::SeqCst);
                if index == 0 {
                    wait_until(|| started.load(Ordering::SeqCst) >= 6);
                    std::thread::sleep(Duration::from_millis(30));
                    assert_eq!(started.load(Ordering::SeqCst), 6);
                }
                Ok(index)
            },
            |v| {
                accepted.push(v);
                Ok(())
            },
            || {},
        )
        .unwrap();
        assert_eq!(accepted, (0..100).collect::<Vec<_>>());
    }
    #[test]
    fn quota_rejection_cancels_inflight_sibling() {
        let aborted = AtomicBool::new(false);
        let sibling_started = AtomicBool::new(false);
        let control = Control::default();
        let result = ordered(
            100,
            2,
            &control,
            |_, index, stop| {
                if index == 0 {
                    wait_until(|| sibling_started.load(Ordering::Acquire));
                    Ok(index)
                } else {
                    sibling_started.store(true, Ordering::Release);
                    wait_until(|| stop.load(Ordering::Acquire));
                    Err("sibling cancelled".into())
                }
            },
            |_| Err("quota exceeded".into()),
            || {
                aborted.store(true, Ordering::Release);
            },
        );
        assert_eq!(result.unwrap_err(), "quota exceeded");
        assert!(aborted.load(Ordering::Acquire));
    }
    #[test]
    fn worker_and_consumer_panics_abort() {
        for consumer in [false, true] {
            let aborted = AtomicBool::new(false);
            let control = Control::default();
            let result = ordered(
                2,
                2,
                &control,
                |_, index, _| {
                    if !consumer {
                        panic!("worker failure");
                    }
                    Ok(index)
                },
                |_| panic!("accept failure"),
                || {
                    aborted.store(true, Ordering::Release);
                },
            );
            assert!(result.is_err());
            assert!(aborted.load(Ordering::Acquire));
        }
    }
    #[test]
    fn external_cancel_stops_batch() {
        let control = Control::default();
        let aborted = AtomicBool::new(false);
        let result = ordered(
            2,
            2,
            &control,
            |_, _, stop| {
                control.cancel();
                wait_until(|| stop.load(Ordering::Acquire));
                Ok(())
            },
            |_| Ok(()),
            || {
                aborted.store(true, Ordering::Release);
            },
        );
        assert!(result.is_err());
        assert!(aborted.load(Ordering::Acquire));
    }
}
