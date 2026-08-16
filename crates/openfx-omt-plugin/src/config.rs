use openmediatransport::Quality;

pub const PLUGIN_IDENTIFIER: &str = "jp.mikanseilaboratory.OpenFXOMT";
pub const PLUGIN_LABEL: &str = "OMT Output";
pub const PLUGIN_GROUPING: &str = "Mikansei Laboratory";
pub const PLUGIN_AUTHOR: &str = "未完成成果物研究所";
pub const DEFAULT_SOURCE_NAME: &str = "DaVinci Resolve";
pub const MAX_SOURCE_NAME_LEN: usize = 63;
pub const DEFAULT_QUEUE_DEPTH: usize = 4;
pub const MIN_VIDEO_DIM: u32 = 16;
pub const TICKS_PER_SECOND: i64 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QualitySetting {
    #[default]
    Default,
    Low,
    Medium,
    High,
}

impl QualitySetting {
    pub const ALL: [Self; 4] = [Self::Default, Self::Low, Self::Medium, Self::High];

    pub fn as_label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Low,
            2 => Self::Medium,
            3 => Self::High,
            _ => Self::Default,
        }
    }

    pub fn to_omt(self) -> Quality {
        match self {
            Self::Default => Quality::Default,
            Self::Low => Quality::Low,
            Self::Medium => Quality::Medium,
            Self::High => Quality::High,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginConfig {
    pub enabled: bool,
    pub source_name: String,
    pub quality: QualitySetting,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            source_name: DEFAULT_SOURCE_NAME.to_string(),
            quality: QualitySetting::Default,
        }
    }
}

impl PluginConfig {
    pub fn clamped(mut self) -> Self {
        let trimmed = self.source_name.trim();
        if trimmed.is_empty() {
            self.source_name = DEFAULT_SOURCE_NAME.to_string();
        } else if trimmed.chars().count() > MAX_SOURCE_NAME_LEN {
            self.source_name = trimmed.chars().take(MAX_SOURCE_NAME_LEN).collect();
        } else {
            self.source_name = trimmed.to_string();
        }
        self
    }
}

pub fn fps_to_rational(fps: f64) -> (i32, i32) {
    if !fps.is_finite() || fps <= 0.0 {
        return (60, 1);
    }
    const KNOWN: [(f64, i32, i32); 8] = [
        (24_000.0 / 1_001.0, 24_000, 1_001),
        (24.0, 24, 1),
        (25.0, 25, 1),
        (30_000.0 / 1_001.0, 30_000, 1_001),
        (30.0, 30, 1),
        (50.0, 50, 1),
        (60_000.0 / 1_001.0, 60_000, 1_001),
        (60.0, 60, 1),
    ];
    for (value, num, den) in KNOWN {
        if (fps - value).abs() < 0.02 {
            return (num, den);
        }
    }
    if (fps - fps.round()).abs() < 0.001 {
        let n = fps.round() as i32;
        if n > 0 {
            return (n, 1);
        }
    }
    (60, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_source_name() {
        let config = PluginConfig {
            enabled: true,
            source_name: "  ".into(),
            quality: QualitySetting::Default,
        }
        .clamped();
        assert_eq!(config.source_name, DEFAULT_SOURCE_NAME);

        let long = "あ".repeat(80);
        let config = PluginConfig {
            source_name: long,
            ..PluginConfig::default()
        }
        .clamped();
        assert_eq!(config.source_name.chars().count(), MAX_SOURCE_NAME_LEN);
    }

    #[test]
    fn fps_known_rates() {
        assert_eq!(fps_to_rational(29.97), (30_000, 1_001));
        assert_eq!(fps_to_rational(60.0), (60, 1));
        assert_eq!(fps_to_rational(0.0), (60, 1));
        assert_eq!(fps_to_rational(f64::NAN), (60, 1));
    }
}
