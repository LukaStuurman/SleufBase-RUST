//! Transient desktop list selection, scoped to one project and one row order.
use std::collections::BTreeSet;

pub(super) struct Selection<K> {
    generation: Option<u64>,
    rows: Vec<K>,
    selected: BTreeSet<usize>,
    anchor: Option<usize>,
    active: Option<usize>,
    external: Option<usize>,
}

impl<K> Default for Selection<K> {
    fn default() -> Self {
        Self {
            generation: None,
            rows: Vec::new(),
            selected: BTreeSet::new(),
            anchor: None,
            active: None,
            external: None,
        }
    }
}

impl<K: PartialEq> Selection<K> {
    pub fn is_current(&self, generation: u64, rows: &[K]) -> bool {
        self.generation == Some(generation) && self.rows.as_slice() == rows
    }

    /// A changed project, row order or external map selection invalidates the
    /// old index selection. Never silently apply it to newly imported layers.
    pub fn sync(&mut self, generation: u64, rows: Vec<K>, active: Option<usize>) {
        let active = active.filter(|&index| index < rows.len());
        if !self.is_current(generation, &rows) || self.external != active {
            self.rows = rows;
            self.generation = Some(generation);
            self.select_one(active);
        }
    }

    fn select_one(&mut self, active: Option<usize>) {
        self.selected.clear();
        self.selected.extend(active);
        self.anchor = active;
        self.active = active;
        self.external = active;
    }

    pub fn click(&mut self, index: usize, control: bool, shift: bool) {
        if index >= self.rows.len() {
            return;
        }
        if shift {
            let anchor = self.anchor.or(self.active).unwrap_or(index);
            if !control {
                self.selected.clear();
            }
            self.selected.extend(anchor.min(index)..=anchor.max(index));
            self.anchor = Some(anchor);
            self.active = Some(index);
        } else if control {
            if !self.selected.remove(&index) {
                self.selected.insert(index);
                self.active = Some(index);
            } else if self.active == Some(index) {
                self.active = self.selected.iter().next_back().copied();
            }
            self.anchor = Some(index);
        } else {
            self.select_one(Some(index));
        }
        self.external = self.active;
    }

    /// Opening a menu on a selected row keeps its existing group intact.
    pub fn context(&mut self, index: usize) {
        if index >= self.rows.len() {
            return;
        }
        if !self.selected.contains(&index) {
            self.select_one(Some(index));
        } else {
            self.active = Some(index);
            self.external = Some(index);
        }
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    pub fn contains(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    pub fn len(&self) -> usize {
        self.selected.len()
    }

    pub fn indices(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection() -> Selection<&'static str> {
        let mut selection = Selection::default();
        selection.sync(1, vec!["a", "b", "c", "d", "e"], None);
        selection
    }

    #[test]
    fn control_toggles_shift_ranges_and_control_shift_keeps_other_rows() {
        let mut selection = selection();
        selection.click(0, false, false);
        selection.click(3, true, false);
        assert_eq!(selection.indices(), [0, 3]);
        selection.click(4, true, true);
        assert_eq!(selection.indices(), [0, 3, 4]);
        selection.click(1, false, true);
        assert_eq!(selection.indices(), [1, 2, 3]);
        selection.click(2, true, false);
        assert_eq!(selection.indices(), [1, 3]);
        selection.click(4, false, false);
        assert_eq!(selection.indices(), [4]);
        selection.click(4, true, false);
        assert_eq!(selection.active(), None);
        assert_eq!(selection.len(), 0);
    }

    #[test]
    fn context_on_a_selected_row_preserves_group_and_other_row_replaces_it() {
        let mut selection = selection();
        selection.click(0, false, false);
        selection.click(2, true, false);
        selection.context(0);
        assert_eq!(selection.indices(), [0, 2]);
        assert_eq!(selection.active(), Some(0));
        selection.context(3);
        assert_eq!(selection.indices(), [3]);
    }

    #[test]
    fn changed_rows_project_or_map_selection_cannot_retarget_old_groups() {
        let mut selection = selection();
        selection.click(0, false, false);
        selection.click(2, true, false);
        selection.sync(1, vec!["a", "b", "c", "d", "e"], Some(2));
        assert_eq!(selection.indices(), [0, 2]);
        selection.sync(1, vec!["a", "b", "c", "d", "e"], Some(4));
        assert_eq!(selection.indices(), [4]);
        selection.click(1, true, false);
        selection.sync(2, vec!["a", "b", "c", "d", "e"], None);
        assert!(selection.indices().is_empty());
        selection.click(0, false, false);
        selection.click(2, true, false);
        selection.sync(2, vec!["b", "c", "d", "e"], None);
        assert!(selection.indices().is_empty());
    }
}
