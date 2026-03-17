use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub sessions: SessionsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    #[serde(default = "default_true")]
    pub sidebar_visible: bool,
    #[serde(default = "default_true")]
    pub diff_visible: bool,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_theme")]
    pub name: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_ai_color")]
    pub ai_diff_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsConfig {
    #[serde(default = "default_dir")]
    pub default_directory: String,
    #[serde(default)]
    pub custom_flags: Vec<SessionFlagsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFlagsConfig {
    pub directory: String,
    pub flags: Vec<String>,
}

fn default_true() -> bool { true }
fn default_sidebar_width() -> u16 { 22 }
fn default_theme() -> String { "dark".to_string() }
fn default_accent() -> String { "magenta".to_string() }
fn default_ai_color() -> String { "purple".to_string() }
fn default_dir() -> String { "~".to_string() }

impl Default for Config {
    fn default() -> Self {
        Self {
            layout: LayoutConfig::default(),
            theme: ThemeConfig::default(),
            sessions: SessionsConfig::default(),
        }
    }
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            diff_visible: true,
            sidebar_width: 22,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "dark".to_string(),
            accent: "magenta".to_string(),
            ai_diff_color: "purple".to_string(),
        }
    }
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            default_directory: "~".to_string(),
            custom_flags: Vec::new(),
        }
    }
}

impl Config {
    fn config_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".claudetui").join("config.toml")
    }

    /// Load config. Returns (config, optional warning message).
    pub fn load() -> (Self, Option<String>) {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => return (config, None),
                    Err(e) => return (Self::default(), Some(format!("Config parse error: {}", e))),
                },
                Err(e) => return (Self::default(), Some(format!("Config read error: {}", e))),
            }
        }
        (Self::default(), None)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            // Restrict config dir to owner-only (0700)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o700);
                let _ = std::fs::set_permissions(parent, perms);
            }
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, &contents)?;
        // Restrict config file to owner-only (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }
        Ok(())
    }

    /// Get the theme colors
    pub fn theme_colors(&self) -> ThemeColors {
        match self.theme.name.as_str() {
            "light" => ThemeColors::light(),
            "solarized" => ThemeColors::solarized(),
            _ => ThemeColors::dark(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub bg: ratatui::style::Color,
    pub fg: ratatui::style::Color,
    pub accent: ratatui::style::Color,
    pub border_focused: ratatui::style::Color,
    pub border_unfocused: ratatui::style::Color,
    pub diff_add: ratatui::style::Color,
    pub diff_del: ratatui::style::Color,
    pub diff_hunk: ratatui::style::Color,
    pub diff_ai: ratatui::style::Color,
    pub status_bg: ratatui::style::Color,
    pub mode_bg: ratatui::style::Color,
}

use ratatui::style::Color;

impl ThemeColors {
    pub fn dark() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::White,
            accent: Color::Magenta,
            border_focused: Color::Magenta,
            border_unfocused: Color::DarkGray,
            diff_add: Color::Green,
            diff_del: Color::Red,
            diff_hunk: Color::Cyan,
            diff_ai: Color::Rgb(180, 120, 255), // purple
            status_bg: Color::Black,
            mode_bg: Color::Yellow,
        }
    }

    pub fn light() -> Self {
        Self {
            bg: Color::White,
            fg: Color::Black,
            accent: Color::Rgb(200, 50, 150),
            border_focused: Color::Rgb(200, 50, 150),
            border_unfocused: Color::Gray,
            diff_add: Color::Rgb(0, 128, 0),
            diff_del: Color::Rgb(200, 0, 0),
            diff_hunk: Color::Rgb(0, 128, 128),
            diff_ai: Color::Rgb(128, 0, 255),
            status_bg: Color::Rgb(240, 240, 240),
            mode_bg: Color::Rgb(255, 220, 0),
        }
    }

    pub fn solarized() -> Self {
        Self {
            bg: Color::Rgb(0, 43, 54),
            fg: Color::Rgb(131, 148, 150),
            accent: Color::Rgb(211, 54, 130),
            border_focused: Color::Rgb(211, 54, 130),
            border_unfocused: Color::Rgb(88, 110, 117),
            diff_add: Color::Rgb(133, 153, 0),
            diff_del: Color::Rgb(220, 50, 47),
            diff_hunk: Color::Rgb(42, 161, 152),
            diff_ai: Color::Rgb(108, 113, 196),
            status_bg: Color::Rgb(7, 54, 66),
            mode_bg: Color::Rgb(181, 137, 0),
        }
    }
}
