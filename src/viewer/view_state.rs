use crate::utils;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StoredViewState {
    pub(super) scale: f64,
    pub(super) offset_x: f64,
    pub(super) offset_y: f64,
}

#[derive(Default)]
pub(super) struct ViewStateStore {
    states: HashMap<String, StoredViewState>,
}

impl ViewStateStore {
    pub(super) fn save(
        &mut self,
        path: &Path,
        state: StoredViewState,
        remember_zoom: bool,
        remember_position: bool,
    ) {
        let key = utils::canonical_path_key(path);
        if !remember_zoom && !remember_position {
            self.states.remove(&key);
            return;
        }

        let previous = self.states.get(&key).copied().unwrap_or(state);
        self.states.insert(
            key,
            StoredViewState {
                scale: if remember_zoom {
                    state.scale
                } else {
                    previous.scale
                },
                offset_x: if remember_position {
                    state.offset_x
                } else {
                    previous.offset_x
                },
                offset_y: if remember_position {
                    state.offset_y
                } else {
                    previous.offset_y
                },
            },
        );
    }

    pub(super) fn get(&self, path: &Path) -> Option<StoredViewState> {
        self.states.get(&utils::canonical_path_key(path)).copied()
    }

    pub(super) fn contains(&self, path: &Path) -> bool {
        self.states.contains_key(&utils::canonical_path_key(path))
    }

    pub(super) fn clear(&mut self) {
        self.states.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn state(scale: f64, offset_x: f64, offset_y: f64) -> StoredViewState {
        StoredViewState {
            scale,
            offset_x,
            offset_y,
        }
    }

    #[test]
    fn save_merges_only_enabled_view_preferences() {
        let path = PathBuf::from("image.png");
        let mut store = ViewStateStore::default();
        store.save(&path, state(2.0, 10.0, 20.0), true, true);
        store.save(&path, state(3.0, 30.0, 40.0), true, false);

        assert_eq!(store.get(&path), Some(state(3.0, 10.0, 20.0)));
    }

    #[test]
    fn save_removes_state_when_remembering_is_disabled() {
        let path = PathBuf::from("image.png");
        let mut store = ViewStateStore::default();
        store.save(&path, state(2.0, 10.0, 20.0), true, true);
        store.save(&path, state(3.0, 30.0, 40.0), false, false);

        assert!(!store.contains(&path));
    }

    #[test]
    fn clear_removes_all_stored_states() {
        let mut store = ViewStateStore::default();
        let first = PathBuf::from("first.png");
        let second = PathBuf::from("second.png");
        store.save(&first, state(1.0, 0.0, 0.0), true, true);
        store.save(&second, state(2.0, 10.0, 20.0), true, true);

        store.clear();

        assert!(!store.contains(&first));
        assert!(!store.contains(&second));
    }
}
