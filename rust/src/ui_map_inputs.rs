//! File drops and startup paths share the guarded asynchronous loader.
use super::*;
impl SleufBaseApp {
    pub fn with_input_paths(paths: Vec<PathBuf>) -> Self {
        let mut app = Self::default();
        app.queue_input_paths(paths);
        app
    }
    fn queue_input_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if paths.len() > 4096 || self.input_paths.is_some() {
            self.message =
                "Rond de vorige bestandsselectie eerst af; selecteer maximaal 4096 bestanden."
                    .into();
            return;
        }
        self.input_paths = Some((self.project_generation, paths));
    }
    pub(super) fn capture_file_drop(&mut self, ctx: &egui::Context) {
        let paths = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        self.queue_input_paths(paths);
    }
    pub(super) fn process_input_paths(&mut self, ctx: &egui::Context) {
        if self.pending_project_action.is_some() || self.file_intent.is_some() {
            return;
        }
        let Some((generation, paths)) = self.input_paths.take() else {
            return;
        };
        if generation != self.project_generation {
            self.message =
                "Project gewijzigd tijdens het laden. Sleep de bestanden opnieuw naar dit project."
                    .into();
            return;
        }
        let plan = match sleufbase_rust::map_inputs::plan(paths) {
            Ok(plan) => plan,
            Err(e) => {
                self.message = e;
                return;
            }
        };
        if let Some(path) = plan.project {
            self.input_notice = if plan.ignored.is_empty() {
                None
            } else {
                Some(format!(
                    "Naast het projectbestand zijn {} extra bestanden overgeslagen.",
                    plan.ignored.len()
                ))
            };
            self.request_project_action(ProjectAction::OpenInput(path), ctx);
        } else {
            self.load_map_inputs(plan.maps, plan.ignored.len());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn drain(app: &mut SleufBaseApp, ctx: &egui::Context) {
        let until = Instant::now() + Duration::from_secs(15);
        while app.file_intent.is_some() {
            app.poll_file_jobs(ctx);
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
    }
    #[test]
    fn real_drop_event_uses_unsaved_guard_and_keeps_project_identity_until_acceptance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Straat Ø.json");
        save_project(
            &path,
            &NativeProject {
                name: "Dropped project".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let mut app = SleufBaseApp {
            dirty: true,
            project_name: "Edited project".into(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                dropped_files: vec![egui::DroppedFile {
                    path: Some(path.clone()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            |ctx| {
                app.capture_file_drop(ctx);
                app.process_input_paths(ctx);
            },
        );
        assert_eq!(app.project_name, "Edited project");
        assert!(app.file_intent.is_none());
        assert_eq!(
            app.pending_project_action,
            Some(ProjectAction::OpenInput(path.clone()))
        );
        app.finish_project_action(false, &ctx);
        assert!(app.dirty);
        assert_eq!(app.project_name, "Edited project");
        app.queue_input_paths(vec![path.clone()]);
        app.process_input_paths(&ctx);
        app.finish_project_action(true, &ctx);
        drain(&mut app, &ctx);
        assert_eq!(app.project_name, "Dropped project");
        assert_eq!(app.project_path, Some(path));
        assert!(!app.dirty);
    }
    #[test]
    fn mixed_drop_loads_tiff_and_dxf_in_one_batch_and_retains_valid_files_after_error() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let tiff = root.join("rasters/gray16.tiff");
        let dxf = root.join("splines/splines-binary.dxf");
        let mut app = SleufBaseApp::with_input_paths(vec![
            dxf.clone(),
            tiff.clone(),
            dxf,
            root.join("missing.dxf"),
            "skip.exe".into(),
        ]);
        let ctx = egui::Context::default();
        app.process_input_paths(&ctx);
        assert_eq!(
            app.file_loader.as_ref().unwrap().progress().unwrap().total,
            3
        );
        assert!(app.geotiffs.is_empty());
        assert!(app.overlays.is_empty());
        drain(&mut app, &ctx);
        assert_eq!(app.geotiffs.len(), 1,"{}",app.message);
        assert_eq!(app.overlays.len(), 1);
        assert!(app.dirty);
        assert!(app.message.contains("missing.dxf"));
        assert!(app.message.contains("1 extra"));
    }
    #[test]
    fn queued_drop_cannot_enter_a_different_project_and_project_priority_skips_maps() {
        let mut app = SleufBaseApp::with_input_paths(vec!["first.json".into(), "map.tif".into()]);
        let ctx = egui::Context::default();
        app.execute_project_action(ProjectAction::New, &ctx);
        app.process_input_paths(&ctx);
        assert!(app.file_intent.is_none());
        assert!(app.message.contains("Sleep de bestanden opnieuw"));
        app.dirty = true;
        app.queue_input_paths(vec![
            "map.tif".into(),
            "first.klicproject".into(),
            "other.json".into(),
        ]);
        app.process_input_paths(&ctx);
        assert_eq!(
            app.pending_project_action,
            Some(ProjectAction::OpenInput("first.klicproject".into()))
        );
        assert!(app.input_notice.as_ref().unwrap().contains("2 extra"));
        assert!(app.file_intent.is_none());
    }
    #[test]
    fn cancelling_after_the_tiff_is_staged_publishes_none_of_a_mixed_drop() {
        use sleufbase_rust::file_loader::{FileContent, FileLoader};
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let tiff = root.join("rasters/gray16.tiff");
        let dxf = root.join("splines/splines-binary.dxf");
        let (start_tx, start_rx) = mpsc::sync_channel(1);
        let (gate_tx, gate_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let mut app = SleufBaseApp::with_input_paths(vec![tiff, dxf]);
        app.file_loader = Some(
            FileLoader::with_processor(move |_, path, _| {
                if path.extension().unwrap() == "dxf" {
                    start_tx.send(()).unwrap();
                    gate_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                    done_tx.send(()).unwrap();
                    Err("cancelled".into())
                } else {
                    sleufbase_rust::geotiff::read_geotiff_metadata(path)
                        .map(|t| FileContent::GeoTiff(Box::new(t)))
                }
            })
            .unwrap(),
        );
        let ctx = egui::Context::default();
        app.process_input_paths(&ctx);
        start_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(app.geotiffs.is_empty());
        app.cancel_file_load();
        gate_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        app.poll_file_jobs(&ctx);
        assert!(app.geotiffs.is_empty());
        assert!(app.overlays.is_empty());
        assert!(!app.dirty);
    }
    #[test]
    fn a_python_json_backup_restores_as_new_and_never_becomes_a_native_save_target() {
        let source =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python-v1.klicproject");
        let bytes = std::fs::read(source).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let backup = dir.path().join("autosave_legacy.json");
        std::fs::write(&backup, &bytes).unwrap();
        let mut app = SleufBaseApp::default();
        let ctx = egui::Context::default();
        app.request_project_action(ProjectAction::Restore(backup.clone()), &ctx);
        drain(&mut app, &ctx);
        assert!(app.legacy_archive.is_some());
        assert!(app.project_path.is_none());
        assert!(app.dirty);
        assert_eq!(std::fs::read(backup).unwrap(), bytes);
    }
    #[test]
    fn a_failed_restore_preserves_active_map_selection() {
        let mut app = SleufBaseApp::default();
        app.virtual_trenches.push(
            VirtualTrench::new(
                "Keep selection".into(),
                Vertex {
                    x: 1.,
                    y: 2.,
                    z: None,
                },
                Vertex {
                    x: 3.,
                    y: 2.,
                    z: None,
                },
                0.6,
            )
            .unwrap(),
        );
        app.virtual_map.selected = Some(0);
        app.raster_points.selected = Some("active.tiff".into());
        app.raster_points.mode = Some(raster_point_ui::Mode::Reference);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("autosave_broken.json");
        std::fs::write(&path, b"broken json").unwrap();
        let ctx = egui::Context::default();
        app.request_project_action(ProjectAction::Restore(path), &ctx);
        drain(&mut app, &ctx);
        assert_eq!(app.virtual_map.selected, Some(0));
        assert_eq!(app.raster_points.selected, Some("active.tiff".into()));
        assert!(matches!(
            app.raster_points.mode,
            Some(raster_point_ui::Mode::Reference)
        ));
        assert!(app.message.contains("Project niet geladen"));
    }
    #[test]
    fn browser_capture_blocks_all_other_account_actions_before_consuming_credentials() {
        let (tx, rx) = mpsc::channel();
        let mut app = SleufBaseApp {
            live_email: "synthetic@example.test".into(),
            live_password: "test-only".into(),
            ..Default::default()
        };
        app.browser_ui.capturing = Some(rx);
        app.login_kickthemap();
        app.refresh_live_jobs();
        app.download_live_job(42, RemoteFileKind::GeoTiff);
        assert_eq!(app.live_password, "test-only");
        assert!(app.live_rx.is_none());
        assert!(app.live_download_rx.is_none());
        assert!(!app.live_busy);
        assert!(app.browser_ui.capturing.is_some());
        drop(tx);
    }
}
