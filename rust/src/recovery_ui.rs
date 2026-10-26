//! Independent recovery selector. Parent dispatches Restore through its normal
//! cancellable project loader and unsaved-changes workflow, preserving Save As.
use crate::recovery::{Catalog, ScanWorker};
use eframe::egui;
use std::path::PathBuf;
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Close,
    Restore(PathBuf),
    OpenDirectory(PathBuf),
}
pub struct Dialog {
    directories: Vec<PathBuf>,
    worker: Option<ScanWorker>,
    catalog: Catalog,
    selected: Option<usize>,
    error: Option<String>,
}
impl Dialog {
    pub fn new(directories: Vec<PathBuf>) -> Self {
        let worker = Some(ScanWorker::start(directories.clone()));
        Self {
            directories,
            worker,
            catalog: Catalog::default(),
            selected: None,
            error: None,
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        if let Some(result) = self.worker.as_ref().and_then(ScanWorker::poll) {
            self.worker = None;
            match result {
                Ok(c) => {
                    self.selected = if c.entries.is_empty() { None } else { Some(0) };
                    self.catalog = c;
                }
                Err(e) => self.error = Some(e),
            }
        }
        let mut action = None;
        let mut open = true;
        egui::Window::new("Autosave herstellen").open(&mut open).default_width(760.).default_height(430.).show(ctx,|ui|{
            ui.label("Kies een back-up om het project te herstellen. Sla het herstelde project daarna op onder een gewone projectnaam.");
            if self.worker.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label("Back-ups zoeken…");});ctx.request_repaint_after(std::time::Duration::from_millis(40));}
            if let Some(error)=&self.error{ui.colored_label(egui::Color32::RED,error);}
            for warning in &self.catalog.warnings{ui.colored_label(egui::Color32::YELLOW,warning);}
            ui.label(format!("{} autosaves; nieuwste bovenaan",self.catalog.entries.len()));
            egui::ScrollArea::vertical().max_height(290.).show(ui,|ui|{
                egui::Grid::new("autosave_entries").striped(true).num_columns(3).show(ui,|ui|{
                    ui.strong("Datum/tijd (UTC)");ui.strong("Bestandsnaam");ui.strong("Grootte");ui.end_row();
                    for (i,entry) in self.catalog.entries.iter().enumerate(){
                        ui.label(modified_text(entry.modified));
                        let response=ui.selectable_label(self.selected==Some(i),entry.path.file_name().unwrap_or_default().to_string_lossy()).on_hover_text(entry.path.display().to_string());
                        if response.clicked(){self.selected=Some(i);}if response.double_clicked()&&self.worker.is_none(){action=Some(Action::Restore(entry.path.clone()));}
                        ui.label(size_text(entry.bytes));ui.end_row();
                    }
                });
            });
            ui.separator();ui.horizontal(|ui|{
                if ui.button("Vernieuwen").clicked()&&self.worker.is_none(){self.worker=Some(ScanWorker::start(self.directories.clone()));}
                ui.menu_button("Back-upmap openen",|ui|{for directory in &self.directories{if ui.button(directory.display().to_string()).clicked(){action=Some(Action::OpenDirectory(directory.clone()));ui.close_menu();}}});
                if ui.button("Sluiten").clicked(){action=Some(Action::Close);}
                if ui.add_enabled(self.selected.is_some()&&self.worker.is_none(),egui::Button::new("Autosave laden")).clicked()||(self.worker.is_none()&&ui.input(|i|i.key_pressed(egui::Key::Enter))){
                    if let Some(entry)=self.selected.and_then(|i|self.catalog.entries.get(i)){action=Some(Action::Restore(entry.path.clone()));}
                }
            });
        });
        if !open {
            action = Some(Action::Close);
        }
        action
    }
}
fn size_text(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024. * 1024.))
    }
}
fn modified_text(time: Option<std::time::SystemTime>) -> String {
    let Some(seconds) = time
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
    else {
        return "Onbekend".into();
    };
    // Gregorian civil date from Unix days; no process timezone mutation.
    let z = (seconds / 86400) as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{day:02}-{month:02}-{year:04} {:02}:{:02}:{:02}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamp_and_sizes() {
        assert_eq!(
            modified_text(Some(std::time::UNIX_EPOCH)),
            "01-01-1970 00:00:00"
        );
        assert_eq!(size_text(1024), "1.0 KB");
        assert_eq!(size_text(1048576), "1.0 MB");
    }
}
