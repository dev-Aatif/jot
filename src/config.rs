use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const VALID_PRESETS: &[&str] = &[
    "dracula",
    "catppuccin-mocha",
    "nord",
    "tokyo-night",
    "monochrome",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub editor: Option<String>,
    pub db_path: Option<String>,
    pub syntax_highlighting: Option<bool>,
    pub frontmatter: Option<bool>,
    pub wrap_lines: Option<bool>,
    pub theme: ThemeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ThemeConfig {
    pub preset: Option<String>,
    // Semantic palette overrides
    pub bg_surface: Option<String>,
    pub bg_elevated: Option<String>,
    pub border_muted: Option<String>,
    pub border_active: Option<String>,
    pub accent_primary: Option<String>,
    pub accent_secondary: Option<String>,
    pub text_primary: Option<String>,
    pub text_muted: Option<String>,
    pub danger: Option<String>,
    pub success: Option<String>,
    // Legacy v0.4.x fields preserved for backward compatibility
    pub active_border: Option<String>,
    pub highlight_bg: Option<String>,
    pub highlight_fg: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePalette {
    pub preset_name: String,
    pub bg_surface: String,
    pub bg_elevated: String,
    pub border_muted: String,
    pub border_active: String,
    pub accent_primary: String,
    pub accent_secondary: String,
    pub text_primary: String,
    pub text_muted: String,
    pub danger: String,
    pub success: String,
}

impl ThemePalette {
    pub fn from_preset(name: &str) -> Self {
        match name.trim().to_lowercase().as_str() {
            "catppuccin-mocha" | "catppuccin" => Self {
                preset_name: "catppuccin-mocha".to_string(),
                bg_surface: "#1e1e2e".to_string(),
                bg_elevated: "#313244".to_string(),
                border_muted: "#585b70".to_string(),
                border_active: "#cba6f7".to_string(),
                accent_primary: "#cba6f7".to_string(),
                accent_secondary: "#89dceb".to_string(),
                text_primary: "#cdd6f4".to_string(),
                text_muted: "#a6adc8".to_string(),
                danger: "#f38ba8".to_string(),
                success: "#a6e3a1".to_string(),
            },
            "nord" => Self {
                preset_name: "nord".to_string(),
                bg_surface: "#2e3440".to_string(),
                bg_elevated: "#3b4252".to_string(),
                border_muted: "#4c566a".to_string(),
                border_active: "#88c0d0".to_string(),
                accent_primary: "#88c0d0".to_string(),
                accent_secondary: "#81a1c1".to_string(),
                text_primary: "#eceff4".to_string(),
                text_muted: "#7b88a1".to_string(),
                danger: "#bf616a".to_string(),
                success: "#a3be8c".to_string(),
            },
            "tokyo-night" | "tokyonight" => Self {
                preset_name: "tokyo-night".to_string(),
                bg_surface: "#1a1b26".to_string(),
                bg_elevated: "#292e42".to_string(),
                border_muted: "#414868".to_string(),
                border_active: "#7aa2f7".to_string(),
                accent_primary: "#7aa2f7".to_string(),
                accent_secondary: "#7dcfff".to_string(),
                text_primary: "#c0caf5".to_string(),
                text_muted: "#565f89".to_string(),
                danger: "#f7768e".to_string(),
                success: "#9ece6a".to_string(),
            },
            "monochrome" | "mono" => Self {
                preset_name: "monochrome".to_string(),
                bg_surface: "reset".to_string(),
                bg_elevated: "dark_gray".to_string(),
                border_muted: "dark_gray".to_string(),
                border_active: "white".to_string(),
                accent_primary: "white".to_string(),
                accent_secondary: "gray".to_string(),
                text_primary: "reset".to_string(),
                text_muted: "gray".to_string(),
                danger: "white".to_string(),
                success: "white".to_string(),
            },
            _ => Self {
                preset_name: "dracula".to_string(),
                bg_surface: "#282a36".to_string(),
                bg_elevated: "#44475a".to_string(),
                border_muted: "#6272a4".to_string(),
                border_active: "#bd93f9".to_string(),
                accent_primary: "#bd93f9".to_string(),
                accent_secondary: "#8be9fd".to_string(),
                text_primary: "#f8f8f2".to_string(),
                text_muted: "#6272a4".to_string(),
                danger: "#ff5555".to_string(),
                success: "#50fa7b".to_string(),
            },
        }
    }
}

impl ThemeConfig {
    pub fn is_preset_known(name: &str) -> bool {
        matches!(
            name.trim().to_lowercase().as_str(),
            "dracula"
                | "catppuccin-mocha"
                | "catppuccin"
                | "nord"
                | "tokyo-night"
                | "tokyonight"
                | "monochrome"
                | "mono"
        )
    }

    pub fn resolve(&self) -> ThemePalette {
        let mut palette = ThemePalette::from_preset(self.preset.as_deref().unwrap_or("dracula"));

        // Apply legacy v0.4.x overrides first
        if let Some(ref c) = self.active_border {
            palette.border_active = c.clone();
        }
        if let Some(ref c) = self.highlight_bg {
            palette.bg_elevated = c.clone();
        }
        if let Some(ref c) = self.highlight_fg {
            palette.text_primary = c.clone();
        }

        // Apply explicit semantic overrides
        if let Some(ref c) = self.bg_surface {
            palette.bg_surface = c.clone();
        }
        if let Some(ref c) = self.bg_elevated {
            palette.bg_elevated = c.clone();
        }
        if let Some(ref c) = self.border_muted {
            palette.border_muted = c.clone();
        }
        if let Some(ref c) = self.border_active {
            palette.border_active = c.clone();
        }
        if let Some(ref c) = self.accent_primary {
            palette.accent_primary = c.clone();
        }
        if let Some(ref c) = self.accent_secondary {
            palette.accent_secondary = c.clone();
        }
        if let Some(ref c) = self.text_primary {
            palette.text_primary = c.clone();
        }
        if let Some(ref c) = self.text_muted {
            palette.text_muted = c.clone();
        }
        if let Some(ref c) = self.danger {
            palette.danger = c.clone();
        }
        if let Some(ref c) = self.success {
            palette.success = c.clone();
        }

        palette
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            editor: None,
            db_path: None,
            syntax_highlighting: Some(true),
            frontmatter: Some(true),
            wrap_lines: Some(true),
            theme: ThemeConfig {
                preset: Some("dracula".to_string()),
                ..ThemeConfig::default()
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
        let config: Config = toml::from_str(&content).context("Failed to parse config.toml")?;
        Ok(config)
    }

    pub fn get_path() -> Result<PathBuf> {
        if let Ok(p) = std::env::var("JOTUN_CONFIG_PATH")
            && !p.trim().is_empty()
        {
            return Ok(PathBuf::from(p));
        }
        let mut path =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
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
        assert_eq!(config.frontmatter, Some(true));
        assert_eq!(config.wrap_lines, Some(true));
        let palette = config.theme.resolve();
        assert_eq!(palette.preset_name, "dracula");
        assert_eq!(palette.border_active, "#bd93f9");
        assert_eq!(palette.bg_elevated, "#44475a");
        assert_eq!(palette.text_primary, "#f8f8f2");
    }

    #[test]
    fn test_partial_legacy_toml_uses_defaults() {
        let partial = r#"
editor = "nvim"
[theme]
active_border = "cyan"
"#;
        let config: Config = toml::from_str(partial).expect("partial config should parse");
        assert_eq!(config.editor.as_deref(), Some("nvim"));
        assert_eq!(config.syntax_highlighting, Some(true));
        let palette = config.theme.resolve();
        assert_eq!(palette.border_active, "cyan");
        assert_eq!(palette.bg_elevated, "#44475a");
        assert_eq!(palette.text_primary, "#f8f8f2");
    }

    #[test]
    fn test_theme_presets_and_semantic_overrides() {
        let toml_cfg = r##"
[theme]
preset = "nord"
danger = "#ff0000"
"##;
        let config: Config = toml::from_str(toml_cfg).expect("preset config should parse");
        let palette = config.theme.resolve();
        assert_eq!(palette.preset_name, "nord");
        assert_eq!(palette.border_active, "#88c0d0");
        assert_eq!(palette.bg_surface, "#2e3440");
        assert_eq!(palette.danger, "#ff0000");

        for preset in VALID_PRESETS {
            assert!(ThemeConfig::is_preset_known(preset));
            let p = ThemePalette::from_preset(preset);
            assert_eq!(p.preset_name, *preset);
        }
    }

    #[test]
    fn test_malformed_toml_returns_error() {
        let malformed = "editor = [unclosed";
        assert!(toml::from_str::<Config>(malformed).is_err());

        let wrong_type = "syntax_highlighting = \"yes\"";
        assert!(toml::from_str::<Config>(wrong_type).is_err());
    }
}
