use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub hotkey: String,
    #[serde(default = "default_after_capture")]
    pub after_capture: AfterCapture,
    #[serde(default = "default_save_dir")]
    pub save_dir: String,
    #[serde(default)]
    pub tool_prefs: ToolPrefs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterCapture {
    Clipboard,
    Save,
    Pin,
    None,
}

/// 跨会话保存的工具偏好
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPrefs {
    pub active: String,
    pub stroke: [u8; 4],
    pub stroke_width: f32,
    pub fill: bool,
    pub text_size: f32,
    pub mosaic_block: u32,
}

impl Default for ToolPrefs {
    fn default() -> Self {
        Self {
            active: "rect".to_string(),
            stroke: [230, 60, 60, 255],
            stroke_width: 2.0,
            fill: false,
            text_size: 18.0,
            mosaic_block: 16,
        }
    }
}

fn default_after_capture() -> AfterCapture {
    AfterCapture::Clipboard
}

fn default_save_dir() -> String {
    dirs::picture_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fshot")
        .to_string_lossy()
        .into_owned()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+S".to_string(),
            after_capture: default_after_capture(),
            save_dir: default_save_dir(),
            tool_prefs: ToolPrefs::default(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("fshot")
            .join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Ok(c) = serde_json::from_str::<Config>(&s) {
                return c;
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}