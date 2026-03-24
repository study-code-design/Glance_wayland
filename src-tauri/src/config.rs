use std::path::PathBuf;

use tokio::fs;

use crate::error::AppResult;
use crate::models::{TranslationHistoryItem, TranslatorSettings};

#[derive(Debug, Clone)]
pub struct ConfigStore {
    base_dir: PathBuf,
    settings_file: PathBuf,
    history_file: PathBuf,
}

impl ConfigStore {
    pub fn new(base_dir: PathBuf) -> Self {
        let settings_file = base_dir.join("settings.json");
        let history_file = base_dir.join("history.json");
        Self {
            base_dir,
            settings_file,
            history_file,
        }
    }

    pub async fn ensure(&self) -> AppResult<()> {
        fs::create_dir_all(&self.base_dir).await?;
        if fs::metadata(&self.settings_file).await.is_err() {
            let bytes = serde_json::to_vec_pretty(&TranslatorSettings::default())?;
            fs::write(&self.settings_file, bytes).await?;
        }
        if fs::metadata(&self.history_file).await.is_err() {
            fs::write(&self.history_file, b"[]").await?;
        }
        Ok(())
    }

    pub async fn load_settings(&self) -> AppResult<TranslatorSettings> {
        self.ensure().await?;
        let bytes = fs::read(&self.settings_file).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn save_settings(&self, settings: &TranslatorSettings) -> AppResult<()> {
        self.ensure().await?;
        let bytes = serde_json::to_vec_pretty(settings)?;
        fs::write(&self.settings_file, bytes).await?;
        Ok(())
    }

    pub async fn load_history(&self) -> AppResult<Vec<TranslationHistoryItem>> {
        self.ensure().await?;
        let bytes = fs::read(&self.history_file).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn save_history(&self, history: &[TranslationHistoryItem]) -> AppResult<()> {
        self.ensure().await?;
        let bytes = serde_json::to_vec_pretty(history)?;
        fs::write(&self.history_file, bytes).await?;
        Ok(())
    }
}
