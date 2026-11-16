//! Per-layer working-map visibility, stored with each layer in its project.
//! Export preparation does not read this UI-only preference.
use super::*;
use serde_json::Value;

const VISIBLE_KEY: &str = "sleufbase_workmap_visible";

pub(super) fn raster_is_visible(raster: &GeoTiffInfo) -> bool {
    visible(&raster.metadata)
}

pub(super) fn trench_is_visible(trench: &VirtualTrench) -> bool {
    visible(&trench.metadata)
}

fn visible(metadata: &Value) -> bool {
    metadata.get(VISIBLE_KEY).and_then(Value::as_bool).unwrap_or(true)
}

/// Preserve all survey metadata and omit the preference for visible layers.
pub(super) fn set_visible(metadata: &mut Value, show: bool) -> Result<bool, String> {
    sleufbase_rust::raster_points::validate_metadata(metadata)?;
    let mut object = metadata.as_object().cloned().unwrap_or_default();
    if show {
        if object.remove(VISIBLE_KEY).is_none() { return Ok(false); }
    } else if object.insert(VISIBLE_KEY.into(), Value::Bool(false)) == Some(Value::Bool(false)) {
        return Ok(false);
    }
    let next = if object.is_empty() { Value::Null } else { Value::Object(object) };
    sleufbase_rust::raster_points::validate_metadata(&next)?;
    *metadata = next;
    Ok(true)
}

impl SleufBaseApp {
    fn check_metadata_growth(&self, growth: usize) -> Result<(), String> {
        if growth == 0 { return Ok(()); }
        let mut total = growth;
        for trench in &self.virtual_trenches {
            total = total.saturating_add(trench.point_metadata_bytes()?);
        }
        for raster in &self.geotiffs {
            total = total.saturating_add(sleufbase_rust::raster_points::validate_metadata(&raster.metadata)?);
        }
        if total > 16 * 1024 * 1024 {
            return Err("Te veel laagmetadata in project; zichtbaarheid niet gewijzigd.".into());
        }
        Ok(())
    }

    pub(super) fn check_trench_metadata_replacement(&self, index: usize, next: &VirtualTrench) -> Result<(), String> {
        let current = self.virtual_trenches.get(index).ok_or("Proefsleuf is niet meer beschikbaar")?;
        self.check_metadata_growth(next.point_metadata_bytes()?.saturating_sub(current.point_metadata_bytes()?))
    }

    fn cancel_hidden_layer_tools(&mut self) {
        self.cancel_virtual_map();
        self.stop_raster_rotation();
        self.close_maaiveld_mode();
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
    }

    pub(super) fn set_raster_visibility(&mut self, index: usize, show: bool) {
        if self.project_sources_owned() || self.file_intent.is_some() {
            self.message = "Rond eerst de lopende projectbewerking af.".into();
            return;
        }
        let Some(raster) = self.geotiffs.get(index) else { return; };
        let selected = self.raster_points.selected.as_ref() == Some(&raster.path);
        let mut next = raster.metadata.clone();
        let result = set_visible(&mut next, show).and_then(|changed| {
            if changed {
                let before = sleufbase_rust::raster_points::validate_metadata(&raster.metadata)?;
                let after = sleufbase_rust::raster_points::validate_metadata(&next)?;
                self.check_metadata_growth(after.saturating_sub(before))?;
            }
            Ok(changed)
        });
        match result {
            Ok(changed) => {
                if changed { self.geotiffs[index].metadata = next; }
                self.dirty |= changed;
                if changed && !show {
                    if let Some(crop) = self.navigation.raster_crops.get_mut(index) { *crop = None; }
                    if selected { self.cancel_hidden_layer_tools(); }
                }
            }
            Err(error) => self.message = error,
        }
    }

    pub(super) fn set_trench_visibility(&mut self, index: usize, show: bool) {
        if self.project_sources_owned() || self.file_intent.is_some() {
            self.message = "Rond eerst de lopende projectbewerking af.".into();
            return;
        }
        let Some(trench) = self.virtual_trenches.get(index) else { return; };
        let mut next = trench.metadata.clone();
        let result = set_visible(&mut next, show).and_then(|changed| {
            if changed {
                let before = sleufbase_rust::raster_points::validate_metadata(&trench.metadata)?;
                let after = sleufbase_rust::raster_points::validate_metadata(&next)?;
                let growth = after.saturating_sub(before);
                if trench.point_metadata_bytes()?.saturating_add(growth) > 2 * 1024 * 1024 {
                    return Err("Virtuele laag- en puntmetadata is groter dan 2 MB; zichtbaarheid niet gewijzigd.".into());
                }
                self.check_metadata_growth(growth)?;
            }
            Ok(changed)
        });
        match result {
            Ok(changed) => {
                if changed { self.virtual_trenches[index].metadata = next; }
                self.dirty |= changed;
                if changed {
                    if !show && self.virtual_map.selected == Some(index) {
                        self.cancel_hidden_layer_tools();
                    }
                    self.refresh_virtual_references();
                }
            }
            Err(error) => self.message = error,
        }
    }
}
