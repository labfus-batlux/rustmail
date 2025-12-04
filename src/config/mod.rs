use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub email: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let app_dir = home.join(".config").join("rustmail");
        fs::create_dir_all(&app_dir)?;
        Ok(app_dir.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            anyhow::bail!(
                "Config file not found at {:?}. Please create it with your Google OAuth credentials.\n\
                Example config:\n\
                email = \"your.email@gmail.com\"\n\
                client_id = \"your-client-id.apps.googleusercontent.com\"\n\
                client_secret = \"your-client-secret\"",
                path
            );
        }
        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}
