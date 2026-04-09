pub mod models;

use models::*;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn load_app_data(data_dir: &Path) -> Result<AppData> {
    todo!()
}
