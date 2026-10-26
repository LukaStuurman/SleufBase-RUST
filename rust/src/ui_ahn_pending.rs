//! Resume pending automatic heights on open/import, retaining manual edits.
use super::*;
use sleufbase_rust::ahn_pending::{self, Worker};
impl SleufBaseApp {
    pub(super) fn resume_pending_ahn(&mut self) {
        let requests = ahn_pending::plan(&self.virtual_trenches, self.project_generation);
        if requests.is_empty() {
            return;
        }
        #[cfg(test)]
        if self.pending_ahn.is_none() {
            return;
        }
        if self.pending_ahn.is_none() {
            match Worker::new() {
                Ok(worker) => self.pending_ahn = Some(worker),
                Err(e) => {
                    self.message = format!("Automatische AHN-aanvulling kon niet starten: {e}");
                    return;
                }
            }
        }
        self.pending_ahn.as_mut().unwrap().start(requests);
    }
    pub(super) fn poll_pending_ahn(&mut self, ctx: &egui::Context) {
        if self.project_sources_owned() || self.trench_editor.is_some() {
            return;
        }
        let result = self.pending_ahn.as_mut().and_then(Worker::poll);
        if self.pending_ahn.as_ref().is_some_and(Worker::busy) {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        let Some(result) = result else {
            return;
        };
        let responses = match result {
            Ok(responses) => responses,
            Err(e) => {
                self.message = format!("Automatische AHN-aanvulling: {e}");
                return;
            }
        };
        let mut changed = std::collections::BTreeSet::new();
        let mut errors = Vec::new();
        for response in responses {
            let index = response.request.index;
            if let Some(error) = response.error.as_ref() {
                if errors.len() < 3 {
                    errors.push(error.clone());
                }
            }
            match ahn_pending::apply(
                &mut self.virtual_trenches,
                self.project_generation,
                response,
            ) {
                Ok(true) => {
                    changed.insert(index);
                }
                Ok(false) => {}
                Err(e) => {
                    if errors.len() < 3 {
                        errors.push(e);
                    }
                }
            }
        }
        if !changed.is_empty() {
            if self.trench_markers.len() != self.virtual_trenches.len() {
                self.trench_markers = self
                    .virtual_trenches
                    .iter()
                    .map(sleufbase_rust::cross_section::markers)
                    .collect();
            } else {
                for index in changed {
                    self.trench_markers[index] =
                        sleufbase_rust::cross_section::markers(&self.virtual_trenches[index]);
                }
            }
            self.dirty = true;
            ctx.request_repaint();
        }
        if !errors.is_empty() {
            self.message = format!(
                "Niet alle automatische maaiveldhoogten konden worden aangevuld: {}",
                errors.join("; ")
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_height_is_visible_before_later_request_finishes_and_retains_manual_edit() {
        let ctx = egui::Context::default();
        let mut trench = VirtualTrench::new(
            "stream".into(),
            Vertex { x:155000., y:463000., z:None },
            Vertex { x:155010., y:463000., z:None },
            0.6,
        ).unwrap();
        trench.start_metadata = serde_json::json!({"surface_mode":"auto","surface_status":"pending","sleufbase_depth_meters":0.4});
        trench.end_metadata = trench.start_metadata.clone();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut calls = 0;
        let worker = Worker::with_streaming_fetcher(move |_,_,_| {
            calls += 1;
            if calls == 2 { release_rx.recv_timeout(Duration::from_secs(5)).map_err(|e|e.to_string())?; }
            Ok(Some(4.))
        }).unwrap();
        let mut app = SleufBaseApp { pending_ahn:Some(worker), ..Default::default() };
        app.apply_project(NativeProject {virtual_trenches:vec![trench],..Default::default()},None,false,vec![vec![]],&ctx);
        let deadline = Instant::now()+Duration::from_secs(5);
        while app.virtual_trenches[0].start.z.is_none() {
            app.poll_pending_ahn(&ctx);
            assert!(Instant::now()<deadline);
            std::thread::yield_now();
        }
        assert_eq!(app.virtual_trenches[0].start.z,Some(3.6));
        assert!(app.pending_ahn.as_ref().unwrap().busy());
        assert_eq!(app.virtual_trenches[0].end.z,None);
        app.virtual_trenches[0].end.z=Some(7.);
        app.virtual_trenches[0].end_metadata["surface_mode"]=serde_json::json!("manual");
        release_tx.send(()).unwrap();
        while app.pending_ahn.as_ref().unwrap().busy() {
            app.poll_pending_ahn(&ctx);
            assert!(Instant::now()<deadline);
            std::thread::yield_now();
        }
        assert_eq!(app.virtual_trenches[0].end.z,Some(7.));
    }
    #[test]
    fn opening_resumes_auto_heights_and_project_change_discards_late_results() {
        let ctx = egui::Context::default();
        let mut trench = VirtualTrench::new(
            "pending".into(),
            Vertex {
                x: 155000.,
                y: 463000.,
                z: None,
            },
            Vertex {
                x: 155010.,
                y: 463000.,
                z: None,
            },
            0.6,
        )
        .unwrap();
        trench.start_metadata = serde_json::json!({"surface_mode":"auto","surface_status":"pending","sleufbase_depth_meters":0.4});
        let mut app = SleufBaseApp {
            pending_ahn: Some(
                Worker::with_processor(|requests, cancel| {
                    ahn_pending::resolve_with(requests, cancel, |_, _, _| Ok(Some(4.)))
                })
                .unwrap(),
            ),
            ..Default::default()
        };
        app.apply_project(
            NativeProject {
                virtual_trenches: vec![trench.clone()],
                ..Default::default()
            },
            Some("project.json".into()),
            false,
            vec![vec![]],
            &ctx,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.pending_ahn.as_ref().unwrap().busy() {
            app.poll_pending_ahn(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(app.virtual_trenches[0].start.z, Some(3.6));
        assert!(app.dirty);
        assert_eq!(app.project_path, Some("project.json".into()));
        app.apply_project(
            NativeProject {
                virtual_trenches: vec![trench],
                ..Default::default()
            },
            None,
            true,
            vec![vec![]],
            &ctx,
        );
        app.execute_project_action(ProjectAction::New, &ctx);
        app.poll_pending_ahn(&ctx);
        assert!(app.virtual_trenches.is_empty());
        assert!(!app.dirty);
    }
}
