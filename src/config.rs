use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::{Result, Context};
use std::fs;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub editor: Option<String>,
    pub db_path: Option<String>,
    pub syntax_highlighting: Option<bool>,
    pub theme: ThemeConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub active_border: String,
    pub highlight_bg: String,
    pub highlight_fg: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            editor: None,
            db_path: None,
            syntax_highlighting: Some(true),
            theme: ThemeConfig {
                active_border: "#bd93f9".to_string(), // Purple
                highlight_bg: "#44475a".to_string(), // Selection
                highlight_fg: "#f8f8f2".to_string(), // Foreground
            },
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::get_path()?;
        if !config_path.exists() {
            let default_config = Config::default();
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let toml = toml::to_string_pretty(&default_config)?;
            fs::write(&config_path, toml)?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&content)
            .context("Failed to parse config.toml")?;
        Ok(config)
    }

    pub fn get_path() -> Result<PathBuf> {
        let mut path = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        path.push("jotun");
        path.push("config.toml");
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.syntax_highlighting, Some(true));
        assert_eq!(config.theme.active_border, "#FF007F");
    }
}
