//! Mixed template-slot operations from the active source dialog callbacks.
//! Only reviewed Choice fields change; source identity, legacy archive identity,
//! photos, comments and profile associations travel with their entire row.
use crate::{
    geotiff::GeoTiffInfo,
    template_order,
    template_project::{Choice, Source},
    template_scale,
    virtual_trench::VirtualTrench,
};
#[derive(Clone, Debug)]
struct Label {
    number: String,
    variant: String,
}
pub struct Editor {
    pub selected: Vec<bool>,
    pub anchor: usize,
    pub number: String,
    pub variant: String,
    pub scale: String,
    labels: Vec<Label>,
}
impl Editor {
    pub fn new(
        choices: &mut [Choice],
        rasters: &[GeoTiffInfo],
        trenches: &[VirtualTrench],
    ) -> Self {
        let labels = choices
            .iter_mut()
            .enumerate()
            .map(|(i, c)| {
                if matches!(c.source, Source::Empty(_)) {
                    return Label {
                        number: String::new(),
                        variant: String::new(),
                    };
                }
                let label = c.label.as_deref().and_then(split_label);
                let metadata = match c.source {
                    Source::Virtual(n) => trenches.get(n).map(|t| &t.metadata),
                    _ => c.raster.and_then(|n| rasters.get(n)).map(|r| &r.metadata),
                };
                let path = std::path::Path::new(&c.title);
                let number = label
                    .as_ref()
                    .map(|l| l.number.clone())
                    .or_else(|| {
                        metadata
                            .and_then(|m| m.get("template_export_ps_number"))
                            .and_then(|v| crate::python_values::string(Some(v)).ok())
                            .and_then(|s| template_order::number(&s))
                    })
                    .unwrap_or_else(|| template_order::default_number(path, i + 1));
                let variant = label
                    .map(|l| l.variant)
                    .or_else(|| {
                        metadata
                            .and_then(|m| {
                                m.get("template_export_variant_code")
                                    .or_else(|| m.get("template_export_variant"))
                            })
                            .map(template_order::variant)
                    })
                    .unwrap_or_else(|| template_order::default_variant(path));
                c.label = Some(format!("PS{number}{variant}"));
                Label { number, variant }
            })
            .collect();
        let mut selected = vec![false; choices.len()];
        if let Some(v) = selected.first_mut() {
            *v = true;
        }
        let mut result = Self {
            selected,
            anchor: 0,
            number: String::new(),
            variant: String::new(),
            scale: String::new(),
            labels,
        };
        result.sync(choices);
        result
    }
    pub fn sync(&mut self, choices: &[Choice]) {
        let indices = self
            .selected
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                (*s && !matches!(choices[i].source, Source::Empty(_))).then_some(i)
            })
            .collect::<Vec<_>>();
        fn common(mut values: impl Iterator<Item = String>, empty: &str) -> String {
            let Some(first) = values.next() else {
                return empty.into();
            };
            if values.any(|v| v != first) {
                "Gemengd".into()
            } else {
                first
            }
        }
        self.number = common(indices.iter().map(|&i| self.labels[i].number.clone()), "");
        self.variant = common(indices.iter().map(|&i| self.labels[i].variant.clone()), "");
        self.scale = common(
            indices.iter().map(|&i| {
                choices[i]
                    .manual_scale
                    .map_or("Auto".into(), |n| format!("1:{n}"))
            }),
            "Auto",
        );
    }
    pub fn select(&mut self, choices: &[Choice], index: usize, control: bool, shift: bool) {
        if index >= choices.len() {
            return;
        }
        if shift {
            if !control {
                self.selected.fill(false);
            }
            for i in self.anchor.min(index)..=self.anchor.max(index).min(choices.len() - 1) {
                self.selected[i] = true;
            }
        } else {
            if !control {
                self.selected.fill(false);
            }
            self.selected[index] = !self.selected[index];
            self.anchor = index;
        }
        self.sync(choices);
    }
    pub fn select_all(&mut self, choices: &[Choice], value: bool) {
        self.selected.fill(value);
        self.sync(choices);
    }
    pub fn included(&mut self, choices: &mut [Choice], value: bool) {
        for (c, s) in choices.iter_mut().zip(&self.selected) {
            if *s && !matches!(c.source, Source::Empty(_)) {
                c.include = value;
            }
        }
        self.sync(choices);
    }
    pub fn preview(&self, choices: &[Choice]) -> (Vec<Option<String>>, Vec<usize>) {
        let state = template_order::State {
            rows: choices
                .iter()
                .zip(&self.labels)
                .map(|(c, l)| template_order::Row {
                    raster: (!matches!(c.source, Source::Empty(_))).then(|| "source".into()),
                    included: c.include,
                    number: l.number.clone(),
                    variant: l.variant.clone(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        state.preview_labels()
    }
    pub fn apply_label(&mut self, choices: &mut [Choice]) -> Result<(), String> {
        if !self.selected.iter().any(|s| *s) {
            return Ok(());
        }
        let number = if self.number.trim().eq_ignore_ascii_case("gemengd") {
            None
        } else {
            Some(
                template_order::number(&self.number)
                    .ok_or("Vul een positief of nul PS-nummer in, bijvoorbeeld 5 of PS5.")?,
            )
        };
        let variant = if self.variant.trim().eq_ignore_ascii_case("gemengd") {
            None
        } else {
            let raw = self.variant.trim();
            let raw = if raw.eq_ignore_ascii_case("geen variant") {
                String::new()
            } else if raw.to_uppercase().starts_with("VARIANT ") {
                raw[8..].into()
            } else if raw.eq_ignore_ascii_case("automatisch (b/c/d...)") {
                "AUTO".into()
            } else {
                raw.into()
            };
            Some(template_order::variant(&serde_json::Value::String(raw)))
        };
        for (i, c) in choices.iter_mut().enumerate() {
            if self.selected[i] && c.include && !matches!(c.source, Source::Empty(_)) {
                if let Some(n) = &number {
                    self.labels[i].number = n.clone();
                }
                if let Some(v) = &variant {
                    self.labels[i].variant = v.clone();
                }
                c.label = Some(format!(
                    "PS{}{}",
                    self.labels[i].number, self.labels[i].variant
                ));
            }
        }
        self.sync(choices);
        Ok(())
    }
    pub fn apply_scale(&mut self, choices: &mut [Choice]) -> Result<(), String> {
        if !self.selected.iter().any(|s| *s) {
            return Ok(());
        }
        let scale = template_scale::parse(&self.scale)?;
        for (c, s) in choices.iter_mut().zip(&self.selected) {
            if *s && !matches!(c.source, Source::Empty(_)) {
                c.manual_scale = scale;
            }
        }
        self.sync(choices);
        Ok(())
    }
    /// Sort complete project slots; explicit empty slots keep their position.
    /// The returned new-index -> old-index permutation also reorders GUI caches.
    pub fn sort_choices(
        &mut self,
        choices: &mut [Choice],
        mode: template_order::SortMode,
    ) -> Vec<usize> {
        let mut order=(0..choices.len()).collect::<Vec<_>>();
        let mut loaded=order.iter().copied()
            .filter(|&i| !matches!(choices[i].source,Source::Empty(_)))
            .collect::<Vec<_>>();
        loaded.sort_by(|&a,&b|{
            let (left,right)=(&self.labels[a],&self.labels[b]);
            mode.compare(
                &left.number,&left.variant,&choices[a].title,
                &right.number,&right.variant,&choices[b].title,
            )
        });
        let mut sorted=loaded.into_iter();
        for (i,choice) in choices.iter().enumerate(){
            if !matches!(choice.source,Source::Empty(_)){
                order[i]=sorted.next().unwrap();
            }
        }
        template_order::reorder(choices,&order);
        template_order::reorder(&mut self.labels,&order);
        template_order::reorder(&mut self.selected,&order);
        self.anchor=self.selected.iter().position(|s|*s).unwrap_or(0);
        self.sync(choices);
        order
    }
    /// Return swaps so non-persisted UI preview caches can follow the same rows.
    pub fn move_selected(&mut self, choices: &mut [Choice], offset: i32) -> Vec<(usize, usize)> {
        let mut swaps = Vec::new();
        let indices: Vec<_> = if offset < 0 {
            (1..choices.len()).collect()
        } else {
            (0..choices.len().saturating_sub(1)).rev().collect()
        };
        if offset != 0 {
            for i in indices {
                let j = if offset < 0 { i - 1 } else { i + 1 };
                if self.selected[i] && !self.selected[j] {
                    choices.swap(i, j);
                    self.labels.swap(i, j);
                    self.selected.swap(i, j);
                    swaps.push((i, j));
                }
            }
        }
        self.anchor = self.selected.iter().position(|v| *v).unwrap_or(0);
        self.sync(choices);
        swaps
    }
    pub fn insert_empty(&mut self, choices: &mut Vec<Choice>) -> Result<usize, String> {
        if choices.len() >= 20_000 {
            return Err("Te veel sjabloonvakken".into());
        }
        let id = choices
            .iter()
            .filter_map(|c| {
                if let Source::Empty(n) = c.source {
                    Some(n)
                } else {
                    None
                }
            })
            .max()
            .map_or(Some(0), |n| n.checked_add(1))
            .ok_or("Geen vrije identiteit voor leeg vak")?;
        let at = self
            .selected
            .iter()
            .position(|s| *s)
            .unwrap_or(choices.len());
        choices.insert(
            at,
            Choice {
                source: Source::Empty(id),
                raster: None,
                include: true,
                title: String::new(),
                label: None,
                legacy_slot: None,
                profile: false,
                manual_scale: None,
                reverse: false,
                comments: String::new(),
                photos: Vec::new(),
            },
        );
        self.labels.insert(
            at,
            Label {
                number: String::new(),
                variant: String::new(),
            },
        );
        self.selected = vec![false; choices.len()];
        self.selected[at] = true;
        self.anchor = at;
        self.sync(choices);
        Ok(at)
    }
    /// Remove only selected empty rows, returning descending indices for caches.
    pub fn remove_empty(&mut self, choices: &mut Vec<Choice>) -> Vec<usize> {
        let removed = choices
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                (self.selected[i] && matches!(c.source, Source::Empty(_))).then_some(i)
            })
            .rev()
            .collect::<Vec<_>>();
        if removed.is_empty() {
            return removed;
        }
        let first = *removed.last().unwrap();
        for &i in &removed {
            choices.remove(i);
            self.labels.remove(i);
        }
        self.selected = vec![false; choices.len()];
        self.anchor = first.min(choices.len().saturating_sub(1));
        if let Some(v) = self.selected.get_mut(self.anchor) {
            *v = true;
        }
        self.sync(choices);
        removed
    }
}
fn split_label(raw: &str) -> Option<Label> {
    let raw = crate::python_values::uppercase(crate::python_values::trim(raw));
    let raw = raw.strip_prefix("PS").unwrap_or(&raw).trim_start();
    let end = raw
        .char_indices()
        .find(|(_, c)| crate::object_layout::decimal(*c).is_none())
        .map_or(raw.len(), |(i, _)| i);
    let number = template_order::number(&raw[..end])?;
    let variant = template_order::variant(&serde_json::Value::String(raw[end..].into()));
    Some(Label { number, variant })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn choice(i: usize) -> Choice {
        Choice {
            source: Source::Job(i),
            raster: Some(i),
            include: true,
            title: format!("PS{}.tif", i + 1),
            label: Some(format!("PS{}", i + 1)),
            legacy_slot: Some(i + 10),
            profile: i % 2 == 0,
            manual_scale: None,
            reverse: i % 2 == 1,
            comments: format!("private metadata {i}"),
            photos: Vec::new(),
        }
    }

    #[test]
    fn natural_sort_moves_complete_choices_and_preserves_blank_slots_and_selection() {
        let mut choices=vec![choice(9),choice(1),choice(2),choice(0)];
        choices[2].label=Some("PS2B".into());
        choices[2].title="PS2B.tif".into();
        choices[3].source=Source::Empty(1);
        choices[3].label=None;
        choices.swap(2,3);
        // Input: PS10, PS2, empty, PS2B.
        let mut editor=Editor::new(&mut choices,&[],&[]);
        editor.selected=vec![true,false,false,false];
        let order=editor.sort_choices(&mut choices,template_order::SortMode::NumberAsc);
        assert_eq!(order,vec![1,3,2,0]);
        // The empty slot stays at index 2.
        assert_eq!(choices.iter().map(|c|c.label.as_deref()).collect::<Vec<_>>(),
                   [Some("PS2"),Some("PS2B"),None,Some("PS10")]);
        assert_eq!(choices[3].comments,"private metadata 9");
        assert_eq!(choices[3].legacy_slot,Some(19));
        assert_eq!(editor.selected,[false,false,false,true]);
        let order=editor.sort_choices(&mut choices,template_order::SortMode::NumberDesc);
        assert_eq!(order,vec![3,1,2,0]);
        assert_eq!(choices.iter().map(|c|c.label.as_deref()).collect::<Vec<_>>(),
                   [Some("PS10"),Some("PS2B"),None,Some("PS2")]);
        assert_eq!(editor.selected,[true,false,false,false]);
    }
    #[test]
    fn matches_isolated_original_move_callbacks() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/template-order/reference.json"
        ))
        .unwrap();
        for case in fixture["moves"].as_array().unwrap() {
            let count = case["count"].as_u64().unwrap() as usize;
            let mut choices = (0..count).map(choice).collect::<Vec<_>>();
            let mut editor = Editor::new(&mut choices, &[], &[]);
            editor.selected.fill(false);
            for index in case["selection"].as_array().unwrap() {
                editor.selected[index.as_u64().unwrap() as usize] = true;
            }
            editor.move_selected(&mut choices, case["offset"].as_i64().unwrap() as i32);
            let expected = case["order"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect::<Vec<_>>();
            for (c, i) in choices.iter().zip(expected) {
                assert_eq!(c.source, Source::Job(i));
                assert_eq!(c.legacy_slot, Some(i + 10));
                assert_eq!(c.comments, format!("private metadata {i}"));
                assert_eq!(c.raster, Some(i));
                assert_eq!(c.profile, i % 2 == 0);
                assert_eq!(c.reverse, i % 2 == 1);
            }
            let selection = editor
                .selected
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.then_some(i))
                .collect::<Vec<_>>();
            assert_eq!(
                serde_json::to_value(selection).unwrap(),
                case["expected_selection"]
            );
        }
    }
    #[test]
    fn mixed_selection_bulk_labels_scale_and_blank_identity() {
        let mut choices = (0..4).map(choice).collect::<Vec<_>>();
        let mut editor = Editor::new(&mut choices, &[], &[]);
        editor.select(&choices, 2, false, false);
        editor.select(&choices, 0, false, true);
        assert_eq!(editor.selected, vec![true, true, true, false]);
        editor.select(&choices, 1, true, false);
        assert_eq!(editor.selected, vec![true, false, true, false]);
        editor.included(&mut choices, false);
        editor.number = "9".into();
        editor.variant = "AUTO".into();
        editor.apply_label(&mut choices).unwrap();
        assert_eq!(choices[0].label.as_deref(), Some("PS1"));
        editor.scale = "1:250".into();
        editor.apply_scale(&mut choices).unwrap();
        assert_eq!(choices[0].manual_scale, Some(250));
        assert_eq!(choices[2].manual_scale, Some(250));
        editor.included(&mut choices, true);
        editor.number = "9".into();
        editor.variant = "AUTO".into();
        editor.apply_label(&mut choices).unwrap();
        let (labels, invalid) = editor.preview(&choices);
        assert!(invalid.is_empty());
        assert_eq!(labels[0].as_deref(), Some("PS9B"));
        assert_eq!(labels[2].as_deref(), Some("PS9C"));
        let at = editor.insert_empty(&mut choices).unwrap();
        assert_eq!(at, 0);
        assert_eq!(choices[at].source, Source::Empty(0));
        editor.included(&mut choices, false);
        assert!(choices[at].include);
        assert_eq!(editor.remove_empty(&mut choices), vec![0]);
        assert_eq!(choices[0].legacy_slot, Some(10));
        editor.select_all(&choices, false);
        editor.number = "invalid".into();
        editor.scale = "invalid".into();
        assert!(editor.apply_label(&mut choices).is_ok());
        assert!(editor.apply_scale(&mut choices).is_ok());
    }
}
