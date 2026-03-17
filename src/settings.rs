use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub const DEFAULT_MODEL: &str = "openai/gpt-audio-mini";
pub const DEFAULT_PROMPT: &str =
    "Transcribe this audio verbatim. Return only the spoken words as plain text.";

pub type SharedSettings = Arc<RwLock<AppSettings>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub openrouter_api_key: String,
    pub openrouter_model: String,
    pub prompt: String,
    pub app_title: String,
    pub http_referer: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            openrouter_api_key: String::new(),
            openrouter_model: DEFAULT_MODEL.into(),
            prompt: DEFAULT_PROMPT.into(),
            app_title: "tvoice".into(),
            http_referer: None,
        }
    }
}

impl AppSettings {
    pub fn load() -> Result<Self> {
        let path = Self::storage_path()?;
        if !path.exists() {
            return Ok(Self::from_env());
        }

        match fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))
            .and_then(|contents| {
                serde_json::from_str(&contents)
                    .with_context(|| format!("failed to parse {}", path.display()))
            }) {
            Ok(settings) => Ok(settings),
            Err(error) => {
                eprintln!("failed to load saved settings, using environment values: {error:#}");
                Ok(Self::from_env())
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::storage_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let payload =
            serde_json::to_vec_pretty(self).context("failed to serialize tvoice settings")?;
        fs::write(&path, payload).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn storage_path() -> Result<PathBuf> {
        let base_dir = dirs::data_local_dir()
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
            .context("failed to resolve a settings directory")?;

        Ok(base_dir.join("tvoice").join("settings.json"))
    }

    pub fn from_env() -> Self {
        let mut settings = Self::default();

        if let Ok(api_key) = env::var("OPENROUTER_API_KEY") {
            settings.openrouter_api_key = api_key;
        }

        if let Ok(model) = env::var("TVOICE_OPENROUTER_MODEL") {
            settings.openrouter_model = Self::normalize_model(&model);
        }

        if let Ok(prompt) = env::var("TVOICE_OPENROUTER_PROMPT") {
            settings.prompt = Self::normalize_prompt(&prompt);
        }

        if let Ok(app_title) = env::var("TVOICE_APP_TITLE") {
            settings.app_title = Self::normalize_app_title(&app_title);
        }

        settings.http_referer = env::var("TVOICE_HTTP_REFERER")
            .ok()
            .as_deref()
            .and_then(Self::normalize_optional);

        settings
    }

    pub fn normalize_for_save(&mut self) {
        self.openrouter_api_key = self.openrouter_api_key.trim().to_owned();
        self.openrouter_model = Self::normalize_model(&self.openrouter_model);
        self.prompt = Self::normalize_prompt(&self.prompt);
        self.app_title = Self::normalize_app_title(&self.app_title);
        self.http_referer = self
            .http_referer
            .as_deref()
            .and_then(Self::normalize_optional);
    }

    pub fn normalize_model(value: &str) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            DEFAULT_MODEL.into()
        } else {
            trimmed.to_owned()
        }
    }

    pub fn normalize_prompt(value: &str) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            DEFAULT_PROMPT.into()
        } else {
            trimmed.to_owned()
        }
    }

    pub fn normalize_app_title(value: &str) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            "tvoice".into()
        } else {
            trimmed.to_owned()
        }
    }

    fn normalize_optional(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    }
}
