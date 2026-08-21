use std::sync::Mutex;
use crate::storage::store::Store;

pub struct AppState {
    pub store: Mutex<Store>,
}

impl AppState {
    pub fn new(data_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            store: Mutex::new(Store::new(data_dir)),
        }
    }
}
