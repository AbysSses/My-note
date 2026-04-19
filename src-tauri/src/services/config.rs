use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

const MAX_RECENT: usize = 10;
const CONFIG_FILENAME: &str = "app-config.json";

/// App-wide config stored in the OS app-config dir (not inside any vault).
/// Holds the recent-vault list and future global preferences (theme, hotkeys, etc.).
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ConfigStore {
    #[serde(default)]
    pub recent_vaults: Vec<String>,
    #[serde(skip)]
    config_path: PathBuf,
}

impl ConfigStore {
    pub fn load_or_init(config_dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(config_dir)?;
        let path = config_dir.join(CONFIG_FILENAME);

        let mut store = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<Self>(&raw).unwrap_or_default()
        } else {
            Self::default()
        };
        store.config_path = path;
        Ok(store)
    }

    pub fn recent_vaults(&self) -> Vec<String> {
        self.recent_vaults.clone()
    }

    pub fn record_recent_vault(&mut self, path: &Path) -> AppResult<()> {
        let s = path.to_string_lossy().to_string();
        self.recent_vaults.retain(|p| p != &s);
        self.recent_vaults.insert(0, s);
        if self.recent_vaults.len() > MAX_RECENT {
            self.recent_vaults.truncate(MAX_RECENT);
        }
        self.persist()
    }

    fn persist(&self) -> AppResult<()> {
        let body = serde_json::to_string_pretty(self)?;
        std::fs::write(&self.config_path, body)?;
        Ok(())
    }
}
